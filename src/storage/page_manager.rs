/// Page management abstraction.
///
/// The PageManager is responsible for:
/// - Reading/writing pages
/// - Allocating new pages
/// - Managing page lifecycle
/// - Delegating to concrete storage backends
use crate::storage::page::{Page, PageId, PageType, Superblock, PAGE_SIZE};
use crate::Result;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    RwLock,
};

/// Abstract page storage backend.
///
/// Implementations can target local files, memory, remote storage, etc.
pub trait PageStorage: Send + Sync {
    /// Read a page from storage.
    fn read_page(&self, page_id: PageId) -> Result<Page>;

    /// Write a page to storage.
    fn write_page(&mut self, page: &Page) -> Result<()>;

    /// Allocate a new page.
    fn allocate_page(&mut self, page_type: PageType) -> Result<PageId>;

    /// Free a page (may not be immediately reclaimed).
    fn free_page(&mut self, page_id: PageId) -> Result<()>;

    /// Synchronize all pending writes to durable storage.
    fn sync(&mut self) -> Result<()>;

    /// Get the current database size.
    fn size(&self) -> Result<u64>;
}

/// Physical database file backend.
///
/// Database layout:
///   Page 0: superblock / metadata
///   Page 1: first user page
///   Page 2: next user page
///   ...
///
/// Every page occupies exactly `PAGE_SIZE` bytes on disk. The file size is thus
/// always a multiple of `PAGE_SIZE`, with one reserved superblock page at page 0.
pub struct FilePageStorage {
    path: PathBuf,
    file: RwLock<File>,
    next_page_id: AtomicU32,
}

impl FilePageStorage {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| crate::Error::StorageError {
                reason: format!(
                    "Failed to create database directory {}: {}",
                    parent.display(),
                    e
                ),
            })?;
        }

        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to open database file {}: {}", path.display(), e),
            })?;

        let storage = Self {
            path,
            file: RwLock::new(file),
            next_page_id: AtomicU32::new(1),
        };
        storage.initialize()?;
        Ok(storage)
    }

    fn initialize(&self) -> Result<()> {
        let mut file = self.file.write().unwrap();
        let len = file
            .metadata()
            .map_err(|e| crate::Error::StorageError {
                reason: format!(
                    "Failed to stat database file {}: {}",
                    self.path.display(),
                    e
                ),
            })?
            .len();

        if len == 0 {
            let mut page0 = Page::new(PageId::SUPERBLOCK, PageType::Superblock);
            let superblock = Superblock::new();
            let superblock_bytes = superblock.to_bytes();
            page0.data[..superblock_bytes.len()].copy_from_slice(&superblock_bytes);
            Self::write_page_locked(&mut file, &page0)?;
            self.next_page_id.store(1, Ordering::Release);
            return Ok(());
        }

        if len % PAGE_SIZE as u64 != 0 {
            return Err(crate::Error::StorageError {
                reason: format!(
                    "Database file size {} is not a multiple of PAGE_SIZE {}",
                    len, PAGE_SIZE
                ),
            });
        }

        if len < PAGE_SIZE as u64 {
            return Err(crate::Error::StorageError {
                reason: format!(
                    "Database file {} is too small for a valid superblock: {} bytes < {} bytes",
                    self.path.display(),
                    len,
                    PAGE_SIZE
                ),
            });
        }

        let mut bytes = vec![0u8; PAGE_SIZE];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut bytes)
            .map_err(|e| crate::Error::StorageError {
                reason: format!(
                    "Failed to read superblock page from {}: {}",
                    self.path.display(),
                    e
                ),
            })?;

        let superblock = Superblock::from_bytes(&bytes)?;
        if superblock.page_size as usize != PAGE_SIZE {
            return Err(crate::Error::StorageError {
                reason: format!(
                    "Database page size mismatch: stored={}, current={}",
                    superblock.page_size, PAGE_SIZE
                ),
            });
        }

        self.next_page_id
            .store(superblock.next_page_id.max(1), Ordering::Release);
        Ok(())
    }

    fn page_offset(page_id: PageId) -> u64 {
        page_id.physical_offset()
    }

    fn read_page_locked(mut file: &File, page_id: PageId) -> Result<Page> {
        let offset = Self::page_offset(page_id);
        let file_len = file
            .metadata()
            .map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to stat database file during read: {}", e),
            })?
            .len();

        if file_len < offset + PAGE_SIZE as u64 {
            return Err(crate::Error::StorageError {
                reason: format!(
                    "Attempted to read page {} at offset {} beyond EOF (file size: {} bytes)",
                    page_id, offset, file_len
                ),
            });
        }

        let mut bytes = vec![0u8; PAGE_SIZE];
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| crate::Error::StorageError {
                reason: format!(
                    "Failed to seek to page {} at offset {}: {}",
                    page_id, offset, e
                ),
            })?;

        file.read_exact(&mut bytes)
            .map_err(|e| crate::Error::StorageError {
                reason: format!(
                    "Failed to read page {} at offset {}: {}",
                    page_id, offset, e
                ),
            })?;

        let page_type = if page_id == PageId::SUPERBLOCK {
            PageType::Superblock
        } else {
            PageType::Object
        };

        Page::deserialize(page_id, page_type, &bytes)
    }

    fn write_page_locked(file: &mut File, page: &Page) -> Result<()> {
        let bytes = page.serialize()?;
        let offset = Self::page_offset(page.page_id);
        let required_len = offset + PAGE_SIZE as u64;
        let current_len = file
            .metadata()
            .map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to stat database file during write: {}", e),
            })?
            .len();

        if current_len < required_len {
            file.set_len(required_len)
                .map_err(|e| crate::Error::StorageError {
                    reason: format!(
                        "Failed to grow database file for page {} at offset {}: {}",
                        page.page_id, offset, e
                    ),
                })?;
        }

        file.seek(SeekFrom::Start(offset))
            .map_err(|e| crate::Error::StorageError {
                reason: format!(
                    "Failed to seek to page {} at offset {}: {}",
                    page.page_id, offset, e
                ),
            })?;

        file.write_all(&bytes)
            .map_err(|e| crate::Error::StorageError {
                reason: format!(
                    "Failed to write page {} at offset {}: {}",
                    page.page_id, offset, e
                ),
            })?;

        Ok(())
    }

    fn update_superblock_next_page(&self, next_page_id: u32) -> Result<()> {
        let mut file = self.file.write().unwrap();
        let mut superblock_page = Page::new(PageId::SUPERBLOCK, PageType::Superblock);
        let mut superblock = Superblock::new();
        superblock.next_page_id = next_page_id;
        superblock.page_size = PAGE_SIZE as u32;
        let superblock_bytes = superblock.to_bytes();
        superblock_page.data[..superblock_bytes.len()].copy_from_slice(&superblock_bytes);
        Self::write_page_locked(&mut file, &superblock_page)?;
        Ok(())
    }
}

impl PageStorage for FilePageStorage {
    fn read_page(&self, page_id: PageId) -> Result<Page> {
        let file = self.file.read().unwrap();
        Self::read_page_locked(&file, page_id)
    }

    fn write_page(&mut self, page: &Page) -> Result<()> {
        let mut file = self.file.write().unwrap();
        Self::write_page_locked(&mut file, page)
    }

    fn allocate_page(&mut self, page_type: PageType) -> Result<PageId> {
        let next = self
            .next_page_id
            .fetch_add(1, Ordering::SeqCst)
            .max(PageId::SUPERBLOCK.as_u32() + 1);
        let page_id = PageId::new(next);

        if page_id == PageId::SUPERBLOCK {
            return Err(crate::Error::StorageError {
                reason: "Page 0 is reserved for the database superblock".to_string(),
            });
        }

        let mut page = Page::new(page_id, page_type);
        page.mark_dirty();

        {
            let mut file = self.file.write().unwrap();
            Self::write_page_locked(&mut file, &page)?;
        }

        self.update_superblock_next_page(page_id.as_u32() + 1)?;
        Ok(page_id)
    }

    fn free_page(&mut self, page_id: PageId) -> Result<()> {
        if page_id == PageId::SUPERBLOCK {
            return Err(crate::Error::StorageError {
                reason: "Page 0 is reserved and cannot be freed".to_string(),
            });
        }

        Ok(())
    }

    fn sync(&mut self) -> Result<()> {
        let file = self.file.write().unwrap();
        file.sync_all().map_err(|e| crate::Error::StorageError {
            reason: format!(
                "Failed to sync database file {}: {}",
                self.path.display(),
                e
            ),
        })?;
        Ok(())
    }

    fn size(&self) -> Result<u64> {
        let file = self.file.read().unwrap();
        Ok(file
            .metadata()
            .map_err(|e| crate::Error::StorageError {
                reason: format!(
                    "Failed to stat database file {}: {}",
                    self.path.display(),
                    e
                ),
            })?
            .len())
    }
}

/// Page manager coordinates page allocation, reading, writing.
pub struct PageManager {
    storage: Box<dyn PageStorage>,
}

impl PageManager {
    pub fn new(storage: Box<dyn PageStorage>) -> Self {
        Self { storage }
    }

    /// Read a page from storage.
    pub fn read_page(&self, page_id: PageId) -> Result<Page> {
        self.storage.read_page(page_id)
    }

    /// Write a page to storage.
    pub fn write_page(&mut self, page: &Page) -> Result<()> {
        self.storage.write_page(page)
    }

    /// Allocate a new page of a given type.
    pub fn allocate_page(&mut self, page_type: PageType) -> Result<PageId> {
        self.storage.allocate_page(page_type)
    }

    /// Free a page (for compaction/cleanup).
    pub fn free_page(&mut self, page_id: PageId) -> Result<()> {
        self.storage.free_page(page_id)
    }

    /// Sync all pending writes.
    pub fn sync(&mut self) -> Result<()> {
        self.storage.sync()
    }

    /// Get database size in bytes.
    pub fn size(&self) -> Result<u64> {
        self.storage.size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::page::{Page, PageType};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct MockPageStorage {
        pages: std::collections::HashMap<u32, Page>,
        next_page_id: u32,
    }

    impl MockPageStorage {
        fn new() -> Self {
            Self {
                pages: std::collections::HashMap::new(),
                next_page_id: 1,
            }
        }
    }

    impl PageStorage for MockPageStorage {
        fn read_page(&self, page_id: PageId) -> Result<Page> {
            self.pages
                .get(&page_id.0)
                .cloned()
                .ok_or(crate::Error::StorageError {
                    reason: format!("Page not found: {}", page_id),
                })
        }

        fn write_page(&mut self, page: &Page) -> Result<()> {
            self.pages.insert(page.page_id.0, page.clone());
            Ok(())
        }

        fn allocate_page(&mut self, page_type: PageType) -> Result<PageId> {
            let id = PageId::new(self.next_page_id);
            self.next_page_id += 1;
            let page = Page::new(id, page_type);
            self.pages.insert(id.0, page);
            Ok(id)
        }

        fn free_page(&mut self, page_id: PageId) -> Result<()> {
            self.pages.remove(&page_id.0);
            Ok(())
        }

        fn sync(&mut self) -> Result<()> {
            Ok(())
        }

        fn size(&self) -> Result<u64> {
            Ok((self.pages.len() as u64) * (PAGE_SIZE as u64))
        }
    }

    fn unique_db_path(prefix: &str) -> std::path::PathBuf {
        let start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}_{}.db", prefix, std::process::id(), start))
    }

    #[test]
    fn test_page_manager_allocate() {
        let storage = Box::new(MockPageStorage::new());
        let mut pm = PageManager::new(storage);

        let page1 = pm.allocate_page(PageType::Object).unwrap();
        let page2 = pm.allocate_page(PageType::Object).unwrap();

        assert_ne!(page1, page2);
        assert_eq!(page1.as_u32(), 1);
        assert_eq!(page2.as_u32(), 2);
    }

    #[test]
    fn test_page_manager_read_write() {
        let storage = Box::new(MockPageStorage::new());
        let mut pm = PageManager::new(storage);

        let page_id = pm.allocate_page(PageType::Object).unwrap();
        let mut page = pm.read_page(page_id).unwrap();

        assert_eq!(page.page_id, page_id);

        page.mark_dirty();
        pm.write_page(&page).unwrap();

        let read_back = pm.read_page(page_id).unwrap();
        assert_eq!(read_back.page_id, page_id);
    }

    #[test]
    fn test_file_page_storage_allocate_and_size() {
        let path = unique_db_path("allocate");
        let mut storage = FilePageStorage::new(&path).unwrap();

        let page_id = storage.allocate_page(PageType::Object).unwrap();
        assert_eq!(page_id, PageId::new(1));
        assert_eq!(storage.size().unwrap(), (2 * PAGE_SIZE) as u64);

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_file_page_storage_write_read_and_reopen() {
        let path = unique_db_path("persist");

        {
            let mut storage = FilePageStorage::new(&path).unwrap();
            let page_id = storage.allocate_page(PageType::Object).unwrap();
            let mut page = storage.read_page(page_id).unwrap();
            page.data[..16].copy_from_slice(b"persist-data-123");
            storage.write_page(&page).unwrap();
            storage.sync().unwrap();
        }

        {
            let storage = FilePageStorage::new(&path).unwrap();
            let read_back = storage.read_page(PageId::new(1)).unwrap();
            assert_eq!(&read_back.data[..16], b"persist-data-123");
        }

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_file_page_storage_multiple_pages() {
        let path = unique_db_path("multi");
        let mut storage = FilePageStorage::new(&path).unwrap();

        let page1 = storage.allocate_page(PageType::Object).unwrap();
        let page2 = storage.allocate_page(PageType::Object).unwrap();
        let page3 = storage.allocate_page(PageType::Object).unwrap();

        assert_ne!(page1, page2);
        assert_ne!(page2, page3);

        let mut page2_data = storage.read_page(page2).unwrap();
        page2_data.data[..8].copy_from_slice(b"page-two");
        storage.write_page(&page2_data).unwrap();

        let page1_data = storage.read_page(page1).unwrap();
        let page2_after = storage.read_page(page2).unwrap();
        let page3_data = storage.read_page(page3).unwrap();

        assert_ne!(&page1_data.data[..8], &page2_after.data[..8]);
        assert_ne!(&page2_after.data[..8], &page3_data.data[..8]);

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_file_page_storage_rejects_truncated_page() {
        let path = unique_db_path("truncated");
        fs::write(&path, vec![0u8; PAGE_SIZE - 7]).unwrap();

        let result = FilePageStorage::new(&path);
        assert!(result.is_err(), "truncated database should be rejected");

        fs::remove_file(&path).unwrap();
    }
}
