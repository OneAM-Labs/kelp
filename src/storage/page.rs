/// Page-based storage system constants and types.
///
/// This module defines the fundamental page-based storage model used by Kelp.
/// All persistent data is organized into fixed-size pages.
use std::fmt;

/// Default physical page size: 16 KiB
/// This is a sweet spot for modern storage (SSD/NVMe performance).
pub const PAGE_SIZE: usize = 16384; // 16 KiB

/// Maximum number of slots on a single page
/// Reserve some space for metadata, so actual limit is slightly less
pub const MAX_SLOTS_PER_PAGE: usize = 200;

/// Magic bytes for database file format validation
pub const MAGIC_BYTES: &[u8] = b"KELP";

/// Current database version
pub const DB_VERSION: u32 = 1;

/// Page type identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PageType {
    /// Superblock (database header)
    Superblock = 1,
    /// Object data pages
    Object = 2,
    /// Index pages
    Index = 3,
    /// Overflow pages (for large values)
    Overflow = 4,
    /// Free-space management pages
    FreeSpace = 5,
}

impl PageType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(PageType::Superblock),
            2 => Some(PageType::Object),
            3 => Some(PageType::Index),
            4 => Some(PageType::Overflow),
            5 => Some(PageType::FreeSpace),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// A logical page identifier.
/// These remain stable after compaction and aren't filesystem offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageId(pub u32);

impl PageId {
    pub const SUPERBLOCK: PageId = PageId(0);

    pub fn new(id: u32) -> Self {
        PageId(id)
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for PageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Page#{}", self.0)
    }
}

/// A slot identifier within a page.
/// Slots hold variable-sized records in a slotted-page layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotId(pub u16);

impl SlotId {
    pub fn new(id: u16) -> Self {
        SlotId(id)
    }

    pub fn as_u16(self) -> u16 {
        self.0
    }
}

impl fmt::Display for SlotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Slot#{}", self.0)
    }
}

/// Location of a record on the physical storage.
/// This maps a logical ObjectID to a physical page and slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordLocation {
    pub page_id: PageId,
    pub slot_id: SlotId,
}

impl RecordLocation {
    pub fn new(page_id: PageId, slot_id: SlotId) -> Self {
        Self { page_id, slot_id }
    }
}

/// In-memory representation of a page.
/// The data buffer is always PAGE_SIZE bytes.
#[derive(Debug, Clone)]
pub struct Page {
    pub page_id: PageId,
    pub page_type: PageType,
    pub data: Vec<u8>,
    /// Whether this page has been modified since last write
    pub is_dirty: bool,
}

impl Page {
    /// Create a new empty page of a given type.
    pub fn new(page_id: PageId, page_type: PageType) -> Self {
        let data = vec![0u8; PAGE_SIZE];
        Self {
            page_id,
            page_type,
            data,
            is_dirty: true,
        }
    }

    /// Get a slice of the page data.
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// Get a mutable slice of the page data.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.is_dirty = true;
        &mut self.data
    }

    /// Mark page as clean (not dirty).
    pub fn mark_clean(&mut self) {
        self.is_dirty = false;
    }

    /// Mark page as dirty.
    pub fn mark_dirty(&mut self) {
        self.is_dirty = true;
    }

    /// Serialize a page to its exact on-disk representation.
    pub fn serialize(&self) -> crate::Result<[u8; PAGE_SIZE]> {
        if self.data.len() != PAGE_SIZE {
            return Err(crate::Error::StorageError {
                reason: format!(
                    "Page {} has {} bytes; expected exactly {} bytes",
                    self.page_id,
                    self.data.len(),
                    PAGE_SIZE
                ),
            });
        }

        let mut bytes = [0u8; PAGE_SIZE];
        bytes.copy_from_slice(&self.data);
        Ok(bytes)
    }

    /// Deserialize a page from an exact fixed-size on-disk buffer.
    pub fn deserialize(page_id: PageId, page_type: PageType, bytes: &[u8]) -> crate::Result<Self> {
        if bytes.len() != PAGE_SIZE {
            return Err(crate::Error::StorageError {
                reason: format!(
                    "Page {} is {} bytes; expected exactly {} bytes",
                    page_id,
                    bytes.len(),
                    PAGE_SIZE
                ),
            });
        }

        let mut data = vec![0u8; PAGE_SIZE];
        data.copy_from_slice(bytes);

        if page_id == PageId::SUPERBLOCK && page_type == PageType::Superblock {
            Superblock::from_bytes(bytes)?;
        }

        Ok(Self {
            page_id,
            page_type,
            data,
            is_dirty: false,
        })
    }

    /// Compute the on-disk byte offset for a page.
    pub fn physical_offset(&self) -> u64 {
        self.page_id.as_u32() as u64 * PAGE_SIZE as u64
    }
}

impl PageId {
    /// Offset, in bytes, where this page starts in the database file.
    pub fn physical_offset(self) -> u64 {
        self.as_u32() as u64 * PAGE_SIZE as u64
    }
}

/// Superblock header - contains database metadata.
/// Located at PageId::SUPERBLOCK.
#[derive(Debug, Clone)]
pub struct Superblock {
    /// Database version
    pub version: u32,
    /// Page size
    pub page_size: u32,
    /// Next available page ID (for allocations)
    pub next_page_id: u32,
    /// Root of object location index
    pub location_index_root: u32,
    /// Root of schema metadata
    pub schema_root: u32,
    /// Tail of WAL
    pub wal_tail: u64,
    /// Last committed transaction ID
    pub last_committed_txn: u64,
    /// Database name
    pub database_name: [u8; 64],
    /// Creation timestamp
    pub created_at: u64,
    /// Last modified timestamp
    pub modified_at: u64,
    /// Reserved for future use
    pub reserved: [u8; 256],
}

impl Superblock {
    pub fn new() -> Self {
        Self {
            version: DB_VERSION,
            page_size: PAGE_SIZE as u32,
            next_page_id: 1, // Page 0 is superblock
            location_index_root: 0,
            schema_root: 0,
            wal_tail: 0,
            last_committed_txn: 0,
            database_name: [0u8; 64],
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            modified_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            reserved: [0u8; 256],
        }
    }

    /// Serialize superblock to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(512);
        bytes.extend_from_slice(b"KELP");
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&self.page_size.to_le_bytes());
        bytes.extend_from_slice(&self.next_page_id.to_le_bytes());
        bytes.extend_from_slice(&self.location_index_root.to_le_bytes());
        bytes.extend_from_slice(&self.schema_root.to_le_bytes());
        bytes.extend_from_slice(&self.wal_tail.to_le_bytes());
        bytes.extend_from_slice(&self.last_committed_txn.to_le_bytes());
        bytes.extend_from_slice(&self.database_name);
        bytes.extend_from_slice(&self.created_at.to_le_bytes());
        bytes.extend_from_slice(&self.modified_at.to_le_bytes());
        bytes.extend_from_slice(&self.reserved);

        // Ensure a fixed header size for persistence.
        if bytes.len() < 512 {
            bytes.resize(512, 0);
        }
        bytes
    }

    /// Deserialize superblock from bytes.
    pub fn from_bytes(bytes: &[u8]) -> crate::Result<Self> {
        if bytes.len() < 512 {
            return Err(crate::Error::StorageError {
                reason: "Superblock too small".to_string(),
            });
        }

        if &bytes[0..4] != b"KELP" {
            return Err(crate::Error::StorageError {
                reason: "Invalid database magic bytes".to_string(),
            });
        }

        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if version != DB_VERSION {
            return Err(crate::Error::StorageError {
                reason: format!("Unsupported database version: {}", version),
            });
        }

        let mut database_name = [0u8; 64];
        database_name.copy_from_slice(&bytes[56..120]);

        Ok(Self {
            version,
            page_size: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            next_page_id: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
            location_index_root: u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
            schema_root: u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
            wal_tail: u64::from_le_bytes([
                bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30],
                bytes[31],
            ]),
            last_committed_txn: u64::from_le_bytes([
                bytes[32], bytes[33], bytes[34], bytes[35], bytes[36], bytes[37], bytes[38],
                bytes[39],
            ]),
            database_name,
            created_at: u64::from_le_bytes([
                bytes[120], bytes[121], bytes[122], bytes[123], bytes[124], bytes[125], bytes[126],
                bytes[127],
            ]),
            modified_at: u64::from_le_bytes([
                bytes[128], bytes[129], bytes[130], bytes[131], bytes[132], bytes[133], bytes[134],
                bytes[135],
            ]),
            reserved: [0u8; 256],
        })
    }
}

impl Default for Superblock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_id() {
        let id = PageId::new(42);
        assert_eq!(id.as_u32(), 42);
        assert_eq!(id, PageId(42));
    }

    #[test]
    fn test_slot_id() {
        let id = SlotId::new(7);
        assert_eq!(id.as_u16(), 7);
    }

    #[test]
    fn test_superblock_roundtrip() {
        let mut sb = Superblock::new();
        sb.next_page_id = 100;
        sb.last_committed_txn = 42;

        let bytes = sb.to_bytes();
        let restored = Superblock::from_bytes(&bytes).unwrap();

        assert_eq!(restored.version, DB_VERSION);
        assert_eq!(restored.next_page_id, 100);
        assert_eq!(restored.last_committed_txn, 42);
    }
}
