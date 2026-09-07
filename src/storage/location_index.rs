use crate::object::ObjectId;
use crate::storage::page::RecordLocation;
use crate::Result;
/// Object location index mapping.
///
/// Maps logical ObjectID to physical location (PageID, SlotID).
/// This allows objects to move during compaction without changing their logical identity.
use std::collections::HashMap;

/// Maps ObjectID to its physical location on storage.
pub struct ObjectLocationIndex {
    // Map of type_name -> (object_id -> location)
    index: HashMap<String, HashMap<String, RecordLocation>>,
}

impl ObjectLocationIndex {
    /// Create a new empty location index.
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
        }
    }

    /// Insert or update a location mapping.
    pub fn insert(&mut self, obj_id: &ObjectId, location: RecordLocation) {
        let type_map = self.index.entry(obj_id.type_name.clone()).or_default();
        type_map.insert(obj_id.object_id.clone(), location);
    }

    /// Look up an object's location.
    pub fn get(&self, obj_id: &ObjectId) -> Option<RecordLocation> {
        self.index
            .get(&obj_id.type_name)
            .and_then(|m| m.get(&obj_id.object_id).copied())
    }

    /// Remove a location mapping (e.g., when object is deleted).
    pub fn remove(&mut self, obj_id: &ObjectId) -> Option<RecordLocation> {
        self.index
            .get_mut(&obj_id.type_name)
            .and_then(|m| m.remove(&obj_id.object_id))
    }

    /// Check if object exists in index.
    pub fn exists(&self, obj_id: &ObjectId) -> bool {
        self.index
            .get(&obj_id.type_name)
            .map(|m| m.contains_key(&obj_id.object_id))
            .unwrap_or(false)
    }

    /// Get all objects of a given type.
    pub fn get_all_by_type(&self, type_name: &str) -> Vec<(String, RecordLocation)> {
        self.index
            .get(type_name)
            .map(|m| m.iter().map(|(id, loc)| (id.clone(), *loc)).collect())
            .unwrap_or_default()
    }

    /// List all object types.
    pub fn types(&self) -> Vec<String> {
        self.index.keys().cloned().collect()
    }

    /// Get total object count.
    pub fn total_objects(&self) -> usize {
        self.index.values().map(|m| m.len()).sum()
    }

    /// Get object count by type.
    pub fn object_count_by_type(&self, type_name: &str) -> usize {
        self.index.get(type_name).map(|m| m.len()).unwrap_or(0)
    }

    /// Update object location (used during compaction).
    pub fn update_location(
        &mut self,
        obj_id: &ObjectId,
        new_location: RecordLocation,
    ) -> Result<()> {
        if self.exists(obj_id) {
            self.insert(obj_id, new_location);
            Ok(())
        } else {
            Err(crate::Error::ObjectNotFound {
                type_name: obj_id.type_name.clone(),
                object_id: obj_id.object_id.clone(),
            })
        }
    }
}

impl Default for ObjectLocationIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::page::{PageId, SlotId};

    #[test]
    fn test_location_index_insert_get() {
        let mut index = ObjectLocationIndex::new();
        let obj_id = ObjectId::new("User", "u1");
        let location = RecordLocation::new(PageId::new(1), SlotId::new(0));

        index.insert(&obj_id, location);
        assert_eq!(index.get(&obj_id), Some(location));
    }

    #[test]
    fn test_location_index_update() {
        let mut index = ObjectLocationIndex::new();
        let obj_id = ObjectId::new("User", "u1");
        let loc1 = RecordLocation::new(PageId::new(1), SlotId::new(0));
        let loc2 = RecordLocation::new(PageId::new(2), SlotId::new(5));

        index.insert(&obj_id, loc1);
        assert_eq!(index.get(&obj_id), Some(loc1));

        index.insert(&obj_id, loc2);
        assert_eq!(index.get(&obj_id), Some(loc2));
    }

    #[test]
    fn test_location_index_remove() {
        let mut index = ObjectLocationIndex::new();
        let obj_id = ObjectId::new("User", "u1");
        let location = RecordLocation::new(PageId::new(1), SlotId::new(0));

        index.insert(&obj_id, location);
        assert!(index.exists(&obj_id));

        let removed = index.remove(&obj_id);
        assert_eq!(removed, Some(location));
        assert!(!index.exists(&obj_id));
    }

    #[test]
    fn test_location_index_by_type() {
        let mut index = ObjectLocationIndex::new();

        for i in 0..5 {
            let obj_id = ObjectId::new("User", &format!("u{}", i));
            let location = RecordLocation::new(PageId::new(1), SlotId::new(i));
            index.insert(&obj_id, location);
        }

        let users = index.get_all_by_type("User");
        assert_eq!(users.len(), 5);

        let count = index.object_count_by_type("User");
        assert_eq!(count, 5);
    }

    #[test]
    fn test_location_index_total() {
        let mut index = ObjectLocationIndex::new();

        for i in 0..3 {
            index.insert(
                &ObjectId::new("User", &format!("u{}", i)),
                RecordLocation::new(PageId::new(1), SlotId::new(i as u16)),
            );
        }

        for i in 0..2 {
            index.insert(
                &ObjectId::new("Order", &format!("o{}", i)),
                RecordLocation::new(PageId::new(2), SlotId::new(i as u16)),
            );
        }

        assert_eq!(index.total_objects(), 5);
        let types = index.types();
        assert_eq!(types.len(), 2);
    }
}
