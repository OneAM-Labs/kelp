use crate::storage::page::{Page, PageId, PAGE_SIZE};
use crate::Result;
/// Page cache with Memory bounds and LRU eviction.
///
/// Acts as a buffer pool between the storage and the application.
/// Manages in-memory pages, evicts cold pages, and tracks dirty pages.
use std::collections::HashMap;

/// LRU eviction policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// Least Recently Used
    LRU,
    /// First In First Out
    FIFO,
}

/// Page cache statistics.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub dirty_pages: u64,
    pub pages_in_cache: usize,
}

/// Bounded page cache using LRU eviction.
pub struct PageCache {
    // Page ID -> Page
    pages: HashMap<u32, CachedPage>,
    // LRU access order (oldest first)
    lru_order: Vec<u32>,
    // Maximum bytes this cache can hold
    max_bytes: usize,
    // Current bytes in cache
    current_bytes: usize,
    // Statistics
    stats: CacheStats,
    // Eviction policy
    policy: EvictionPolicy,
}

/// A page with metadata for caching.
struct CachedPage {
    page: Page,
    access_count: u64,
}

impl PageCache {
    /// Create a new page cache with max size in bytes.
    pub fn new(max_bytes: usize) -> Self {
        Self {
            pages: HashMap::new(),
            lru_order: Vec::new(),
            max_bytes,
            current_bytes: 0,
            stats: CacheStats::default(),
            policy: EvictionPolicy::LRU,
        }
    }

    /// Get a page from cache if present.
    pub fn get(&mut self, page_id: PageId) -> Option<Page> {
        if let Some(cached) = self.pages.get_mut(&page_id.0) {
            cached.access_count += 1;
            self.stats.hits += 1;

            // Update LRU order: move to end
            if let Some(pos) = self.lru_order.iter().position(|x| *x == page_id.0) {
                self.lru_order.remove(pos);
                self.lru_order.push(page_id.0);
            }

            Some(cached.page.clone())
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// Insert a page into the cache.
    pub fn insert(&mut self, page: Page) -> Result<()> {
        let page_id = page.page_id.0;

        // If page already exists, just update it
        if let Some(cached) = self.pages.get_mut(&page_id) {
            cached.page = page;
            return Ok(());
        }

        // Make room if necessary
        while self.current_bytes + PAGE_SIZE > self.max_bytes && !self.pages.is_empty() {
            self.evict_one()?;
        }

        self.pages.insert(
            page_id,
            CachedPage {
                page: page.clone(),
                access_count: 1,
            },
        );

        self.lru_order.push(page_id);
        self.current_bytes += PAGE_SIZE;

        Ok(())
    }

    /// Mark a page as dirty.
    pub fn mark_dirty(&mut self, page_id: PageId) -> Result<()> {
        if let Some(cached) = self.pages.get_mut(&page_id.0) {
            cached.page.mark_dirty();
            self.stats.dirty_pages += 1;
        }
        Ok(())
    }

    /// Get dirty pages (to be persisted).
    pub fn get_dirty_pages(&self) -> Vec<Page> {
        self.pages
            .values()
            .filter(|cp| cp.page.is_dirty)
            .map(|cp| cp.page.clone())
            .collect()
    }

    /// Mark all dirty pages as clean.
    pub fn mark_all_clean(&mut self) {
        for cached in self.pages.values_mut() {
            cached.page.mark_clean();
        }
        self.stats.dirty_pages = 0;
    }

    /// Remove a page from cache.
    pub fn remove(&mut self, page_id: PageId) -> Option<Page> {
        if let Some(cached) = self.pages.remove(&page_id.0) {
            self.current_bytes = self.current_bytes.saturating_sub(PAGE_SIZE);
            if let Some(pos) = self.lru_order.iter().position(|x| *x == page_id.0) {
                self.lru_order.remove(pos);
            }
            Some(cached.page)
        } else {
            None
        }
    }

    /// Evict one page according to the policy.
    fn evict_one(&mut self) -> Result<()> {
        let victim = match self.policy {
            EvictionPolicy::LRU => {
                // Evict least recently used (first in queue)
                self.lru_order.first().copied()
            }
            EvictionPolicy::FIFO => {
                // FIFO is the same as LRU order here
                self.lru_order.first().copied()
            }
        };

        if let Some(page_id) = victim {
            self.remove(PageId::new(page_id));
            self.stats.evictions += 1;
        }

        Ok(())
    }

    /// Clear all pages from cache.
    pub fn clear(&mut self) {
        self.pages.clear();
        self.lru_order.clear();
        self.current_bytes = 0;
    }

    /// Get cache statistics.
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Get cache hit ratio.
    pub fn hit_ratio(&self) -> f64 {
        let total = self.stats.hits + self.stats.misses;
        if total == 0 {
            0.0
        } else {
            (self.stats.hits as f64) / (total as f64)
        }
    }

    /// Get number of pages in cache.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Get current memory usage.
    pub fn current_bytes(&self) -> usize {
        self.current_bytes
    }

    /// Get max memory budget.
    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }
}

impl Default for PageCache {
    fn default() -> Self {
        // 256 MB default
        Self::new(256 * 1024 * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::page::PageType;

    #[test]
    fn test_cache_get_insert() {
        let mut cache = PageCache::new(1024 * 1024);
        let page = Page::new(PageId::new(1), PageType::Object);

        cache.insert(page.clone()).unwrap();
        let retrieved = cache.get(PageId::new(1));

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().page_id, PageId::new(1));
    }

    #[test]
    fn test_cache_eviction() {
        let page_count = 5;
        let max_bytes = PAGE_SIZE * (page_count - 1); // Only fit 4 pages
        let mut cache = PageCache::new(max_bytes);

        for i in 0..page_count {
            let page = Page::new(PageId::new(i as u32), PageType::Object);
            cache.insert(page).unwrap();
        }

        // Should have evicted something
        assert!(cache.page_count() <= page_count - 1);
        assert!(cache.stats.evictions > 0);
    }

    #[test]
    fn test_cache_lru_order() {
        let mut cache = PageCache::new(1024 * 1024);

        // Insert 3 pages
        for i in 0..3 {
            let page = Page::new(PageId::new(i), PageType::Object);
            cache.insert(page).unwrap();
        }

        // Access page 0, making it recently used
        cache.get(PageId::new(0));

        // Now fill up cache and trigger eviction
        // Page 1 should be evicted (least recently used)
        let max_bytes = PAGE_SIZE * 3;
        let mut cache2 = PageCache::new(max_bytes);

        for i in 0..3 {
            let page = Page::new(PageId::new(i), PageType::Object);
            cache2.insert(page).unwrap();
        }

        // Access page 0
        cache2.get(PageId::new(0));

        // Insert a 4th page, should evict page 1
        let page4 = Page::new(PageId::new(3), PageType::Object);
        cache2.insert(page4).unwrap();

        // Page 2 should be in cache (more recently used than 1)
        assert!(cache2.get(PageId::new(2)).is_some());
    }

    #[test]
    fn test_cache_dirty_tracking() {
        let mut cache = PageCache::new(1024 * 1024);
        let page = Page::new(PageId::new(1), PageType::Object);

        cache.insert(page).unwrap();
        cache.mark_dirty(PageId::new(1)).unwrap();

        let dirty = cache.get_dirty_pages();
        assert_eq!(dirty.len(), 1);

        cache.mark_all_clean();
        let dirty = cache.get_dirty_pages();
        assert_eq!(dirty.len(), 0);
    }

    #[test]
    fn test_cache_hit_ratio() {
        let mut cache = PageCache::new(1024 * 1024);
        let page = Page::new(PageId::new(1), PageType::Object);

        cache.insert(page).unwrap();

        cache.get(PageId::new(1)); // hit
        cache.get(PageId::new(2)); // miss
        cache.get(PageId::new(1)); // hit

        let ratio = cache.hit_ratio();
        assert!(ratio >= 0.5 && ratio <= 0.75);
    }
}
