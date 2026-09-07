/// Slotted page layout for variable-sized records.
///
/// A slotted page stores multiple variable-sized records on a single page.
/// The layout is:
///
/// ┌──────────────────────────────┐
/// │ Page Header (8 bytes)        │ [slot_count, free_space_start]
/// ├──────────────────────────────┤
/// │ Slot Directory               │ [offset, length] pairs
/// │ (4 bytes per slot)           │
/// ├──────────────────────────────┤
/// │                              │
/// │  Free Space                  │
/// │                              │
/// ├──────────────────────────────┤
/// │ Records (grow upward)        │
/// │ [record_1][record_2]...      │
/// └──────────────────────────────┘
///
/// A slot is identified by its index in the directory.
/// Slots can be reused after deletion (slot becomes invalid).
use crate::storage::page::{Page, SlotId, PAGE_SIZE};
use crate::Result;
use std::fmt;

/// Maximum record size on a page.
/// Reserve space for header and slot directory.
const HEADER_SIZE: usize = 8;
const SLOT_ENTRY_SIZE: usize = 4; // 2 bytes offset + 2 bytes length

/// Header of a slotted page.
#[derive(Debug, Clone, Copy)]
struct SlottedPageHeader {
    /// Number of used slots
    slot_count: u16,
    /// Offset where free space ends (records grow from here)
    free_space_start: u16,
}

impl SlottedPageHeader {
    fn to_bytes(self) -> [u8; HEADER_SIZE] {
        let mut bytes = [0u8; HEADER_SIZE];
        bytes[0..2].copy_from_slice(&self.slot_count.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.free_space_start.to_le_bytes());
        bytes
    }

    fn from_bytes(bytes: &[u8; HEADER_SIZE]) -> Self {
        Self {
            slot_count: u16::from_le_bytes([bytes[0], bytes[1]]),
            free_space_start: u16::from_le_bytes([bytes[2], bytes[3]]),
        }
    }
}

/// Manager for a slotted page's record layout.
pub struct SlottedPageLayout {
    header: SlottedPageHeader,
}

impl SlottedPageLayout {
    /// Create a new slotted page layout.
    pub fn new() -> Self {
        Self {
            header: SlottedPageHeader {
                slot_count: 0,
                free_space_start: PAGE_SIZE as u16,
            },
        }
    }

    /// Load layout from page data.
    pub fn from_page(page: &Page) -> Result<Self> {
        if page.data.len() < HEADER_SIZE {
            return Err(crate::Error::StorageError {
                reason: "Page too small for slotted layout".to_string(),
            });
        }

        let mut header_bytes = [0u8; HEADER_SIZE];
        header_bytes.copy_from_slice(&page.data[0..HEADER_SIZE]);
        let header = SlottedPageHeader::from_bytes(&header_bytes);

        Ok(Self { header })
    }

    /// Write layout back to page data.
    pub fn to_page(&self, page: &mut Page) -> Result<()> {
        let header_bytes = self.header.to_bytes();
        page.data[0..HEADER_SIZE].copy_from_slice(&header_bytes);
        Ok(())
    }

    /// Calculate the offset of a slot in the directory.
    fn slot_offset(&self, slot_id: SlotId) -> usize {
        HEADER_SIZE + (slot_id.as_u16() as usize) * SLOT_ENTRY_SIZE
    }

    /// Get the directory entry for a slot.
    fn get_slot_entry(&self, page: &Page, slot_id: SlotId) -> Result<Option<(u16, u16)>> {
        let offset = self.slot_offset(slot_id);
        if offset + SLOT_ENTRY_SIZE > PAGE_SIZE {
            return Ok(None);
        }

        let record_offset = u16::from_le_bytes([page.data[offset], page.data[offset + 1]]);
        let record_length = u16::from_le_bytes([page.data[offset + 2], page.data[offset + 3]]);

        // A slot with offset 0 and length 0 is unused
        if record_offset == 0 && record_length == 0 {
            return Ok(None);
        }

        Ok(Some((record_offset, record_length)))
    }

    /// Set a slot directory entry.
    fn set_slot_entry(
        &mut self,
        page: &mut Page,
        slot_id: SlotId,
        offset: u16,
        length: u16,
    ) -> Result<()> {
        let dir_offset = self.slot_offset(slot_id);
        if dir_offset + SLOT_ENTRY_SIZE > PAGE_SIZE {
            return Err(crate::Error::StorageError {
                reason: "Slot directory overflow".to_string(),
            });
        }

        page.data[dir_offset..dir_offset + 2].copy_from_slice(&offset.to_le_bytes());
        page.data[dir_offset + 2..dir_offset + 4].copy_from_slice(&length.to_le_bytes());
        Ok(())
    }

    /// Insert a record and return the slot ID.
    pub fn insert_record(&mut self, page: &mut Page, record: &[u8]) -> Result<SlotId> {
        let record_len = record.len();

        if !self.can_fit(record_len) {
            return Err(crate::Error::StorageError {
                reason: format!(
                    "Page cannot fit record of size {} bytes (available free space: {} bytes)",
                    record_len,
                    self.free_space()
                ),
            });
        }

        let dir_entries_size = (self.header.slot_count as usize + 1) * SLOT_ENTRY_SIZE;
        let record_offset = self.header.free_space_start as usize - record_len;
        if record_offset < dir_entries_size + HEADER_SIZE {
            return Err(crate::Error::StorageError {
                reason: "Page full - insufficient space for record".to_string(),
            });
        }

        // Write the record
        page.data[record_offset..record_offset + record_len].copy_from_slice(record);

        // Add directory entry
        let slot_id = SlotId::new(self.header.slot_count);
        self.set_slot_entry(page, slot_id, record_offset as u16, record_len as u16)?;

        // Update header
        self.header.slot_count += 1;
        self.header.free_space_start = record_offset as u16;
        self.to_page(page)?;

        Ok(slot_id)
    }

    /// Read a record from a slot.
    pub fn read_record(&self, page: &Page, slot_id: SlotId) -> Result<Option<Vec<u8>>> {
        if let Some((offset, length)) = self.get_slot_entry(page, slot_id)? {
            let offset = offset as usize;
            let length = length as usize;

            if offset + length > PAGE_SIZE {
                return Err(crate::Error::StorageError {
                    reason: format!(
                        "Invalid slot entry: offset {} length {} exceeds page size",
                        offset, length
                    ),
                });
            }

            Ok(Some(page.data[offset..offset + length].to_vec()))
        } else {
            Ok(None)
        }
    }

    /// Delete a record (mark slot as unused).
    pub fn delete_record(&mut self, page: &mut Page, slot_id: SlotId) -> Result<()> {
        self.set_slot_entry(page, slot_id, 0, 0)?;
        self.to_page(page)?;
        Ok(())
    }

    /// Overwrite an existing record in-place if the new record's size is
    /// less than or equal to the existing slot length. This avoids relocating
    /// the record when it fits in the same space.
    pub fn overwrite_record(&mut self, page: &mut Page, slot_id: SlotId, record: &[u8]) -> Result<()> {
        if let Some((offset, length)) = self.get_slot_entry(page, slot_id)? {
            let existing_len = length as usize;
            let new_len = record.len();
            if new_len > existing_len {
                return Err(crate::Error::StorageError {
                    reason: format!("New record of size {} does not fit in existing slot of size {}", new_len, existing_len),
                });
            }

            let off = offset as usize;
            page.data[off..off + new_len].copy_from_slice(record);

            // If new record is smaller, update the slot length; do not move data.
            self.set_slot_entry(page, slot_id, offset, new_len as u16)?;
            self.to_page(page)?;
            Ok(())
        } else {
            Err(crate::Error::StorageError { reason: "Slot not found".to_string() })
        }
    }

    /// Calculate free space on this page.
    pub fn free_space(&self) -> usize {
        let dir_size = (self.header.slot_count as usize) * SLOT_ENTRY_SIZE;
        let used_space =
            dir_size + HEADER_SIZE + (PAGE_SIZE - self.header.free_space_start as usize);
        PAGE_SIZE.saturating_sub(used_space)
    }

    /// Can this page accommodate a record of this size?
    pub fn can_fit(&self, record_size: usize) -> bool {
        if record_size > PAGE_SIZE {
            return false;
        }

        let dir_size = (self.header.slot_count as usize) * SLOT_ENTRY_SIZE;
        let used_space =
            HEADER_SIZE + dir_size + (PAGE_SIZE - self.header.free_space_start as usize);
        let free_space = PAGE_SIZE.saturating_sub(used_space);
        free_space >= record_size
    }

    /// Get number of slots allocated.
    pub fn slot_count(&self) -> u16 {
        self.header.slot_count
    }
}

impl Default for SlottedPageLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for SlottedPageLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlottedPageLayout")
            .field("slot_count", &self.header.slot_count)
            .field("free_space_start", &self.header.free_space_start)
            .field("free_space", &self.free_space())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::page::PageType;

    #[test]
    fn test_slotted_page_insert_read() {
        let mut page = Page::new(crate::storage::page::PageId::new(1), PageType::Object);
        let mut layout = SlottedPageLayout::new();

        let record1 = b"Hello, World!";
        let slot1 = layout.insert_record(&mut page, record1).unwrap();
        assert_eq!(slot1.as_u16(), 0);

        let read_record1 = layout.read_record(&page, slot1).unwrap();
        assert_eq!(read_record1, Some(record1.to_vec()));
    }

    #[test]
    fn test_slotted_page_multiple_records() {
        let mut page = Page::new(crate::storage::page::PageId::new(1), PageType::Object);
        let mut layout = SlottedPageLayout::new();

        let records = vec![
            b"record1".to_vec(),
            b"record2".to_vec(),
            b"record3".to_vec(),
        ];

        let mut slots = Vec::new();
        for record in &records {
            let slot = layout.insert_record(&mut page, record).unwrap();
            slots.push(slot);
        }

        for (i, slot) in slots.iter().enumerate() {
            let read = layout.read_record(&page, *slot).unwrap();
            assert_eq!(read, Some(records[i].clone()));
        }
    }

    #[test]
    fn test_slotted_page_delete() {
        let mut page = Page::new(crate::storage::page::PageId::new(1), PageType::Object);
        let mut layout = SlottedPageLayout::new();

        let record = b"Hello";
        let slot = layout.insert_record(&mut page, record).unwrap();

        layout.delete_record(&mut page, slot).unwrap();
        let read = layout.read_record(&page, slot).unwrap();
        assert_eq!(read, None);
    }

    #[test]
    fn test_slotted_page_free_space() {
        let mut page = Page::new(crate::storage::page::PageId::new(1), PageType::Object);
        let mut layout = SlottedPageLayout::new();

        let initial_free = layout.free_space();
        assert!(initial_free > 0);

        let record = b"x".repeat(1000);
        layout.insert_record(&mut page, &record).unwrap();

        let remaining_free = layout.free_space();
        assert!(remaining_free < initial_free);
    }
}
