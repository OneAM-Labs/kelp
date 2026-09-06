mod local;

pub use local::LocalStorage;

use crate::object::Object;
use crate::schema::Schema;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageSummary {
    /// A human readable storage backend label.
    pub storage_backend: String,
    /// Total disk or memory usage for the currently loaded database state.
    pub total_size_bytes: u64,
    /// Number of object types / schemas currently defined.
    pub schema_count: usize,
    /// Number of known object kinds.
    pub object_type_count: usize,
    /// Total number of stored objects across all types.
    pub object_count: usize,
    /// Named query artifacts that have been precomputed for this database.
    pub precomputed_queries: Vec<String>,
}

/// Abstract storage backend trait.
///
/// This abstraction allows different storage implementations (local files, remote servers, etc.)
/// without changing the object model.
pub trait StorageBackend: Send + Sync {
    /// Create or update a schema.
    fn put_schema(&mut self, schema: &Schema) -> Result<()>;

    /// Retrieve a schema by name.
    fn get_schema(&self, name: &str) -> Result<Option<Schema>>;

    /// List all schema names.
    fn list_schemas(&self) -> Result<Vec<String>>;

    /// Create or update an object.
    fn put_object(&mut self, object: &Object) -> Result<()>;

    /// Retrieve an object by type and ID.
    fn get_object(&self, type_name: &str, object_id: &str) -> Result<Option<Object>>;

    /// Delete an object.
    fn delete_object(&mut self, type_name: &str, object_id: &str) -> Result<()>;

    /// List all objects of a given type.
    fn list_objects(&self, type_name: &str) -> Result<Vec<Object>>;

    /// Check if an object exists.
    fn object_exists(&self, type_name: &str, object_id: &str) -> Result<bool>;

    /// Get a compact summary for the storage layer.
    fn inspect(&self) -> Result<StorageSummary>;

    /// Store a user-friendly database name.
    fn set_database_name(&mut self, name: &str) -> Result<()>;

    /// Read the current database name, if any.
    fn database_name(&self) -> Result<String>;

    /// Store a named precomputed query for later reuse.
    fn save_precomputed_query(&mut self, name: &str, query: &str) -> Result<()>;

    /// List all named precomputed queries.
    fn list_precomputed_queries(&self) -> Result<Vec<String>>;

    /// Begin a transaction (simple version for V1).
    fn begin_transaction(&mut self) -> Result<()>;

    /// Commit current transaction.
    fn commit(&mut self) -> Result<()>;

    /// Rollback current transaction.
    fn rollback(&mut self) -> Result<()>;
}

/// In-memory storage implementation for testing.
///
/// This backend stores all data in memory and is useful for testing the database
/// without persistence concerns.
type TransactionSnapshot = (
    HashMap<String, Schema>,
    HashMap<String, HashMap<String, Object>>,
);

pub struct MemoryStorage {
    schemas: HashMap<String, Schema>,
    objects: HashMap<String, HashMap<String, Object>>,
    in_transaction: bool,
    transaction_snapshot: Option<TransactionSnapshot>,
    precomputed_queries: HashMap<String, String>,
    database_name: String,
}

impl MemoryStorage {
    /// Create a new in-memory storage.
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
            objects: HashMap::new(),
            in_transaction: false,
            transaction_snapshot: None,
            precomputed_queries: HashMap::new(),
            database_name: "kelp-db".to_string(),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageBackend for MemoryStorage {
    fn put_schema(&mut self, schema: &Schema) -> Result<()> {
        schema.validate()?;
        self.schemas.insert(schema.name.clone(), schema.clone());
        Ok(())
    }

    fn get_schema(&self, name: &str) -> Result<Option<Schema>> {
        Ok(self.schemas.get(name).cloned())
    }

    fn list_schemas(&self) -> Result<Vec<String>> {
        Ok(self.schemas.keys().cloned().collect())
    }

    fn put_object(&mut self, object: &Object) -> Result<()> {
        let type_map = self.objects.entry(object.id.type_name.clone()).or_default();
        type_map.insert(object.id.object_id.clone(), object.clone());
        Ok(())
    }

    fn get_object(&self, type_name: &str, object_id: &str) -> Result<Option<Object>> {
        Ok(self
            .objects
            .get(type_name)
            .and_then(|m| m.get(object_id).cloned()))
    }

    fn delete_object(&mut self, type_name: &str, object_id: &str) -> Result<()> {
        if let Some(type_map) = self.objects.get_mut(type_name) {
            type_map.remove(object_id);
        }
        Ok(())
    }

    fn list_objects(&self, type_name: &str) -> Result<Vec<Object>> {
        Ok(self
            .objects
            .get(type_name)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default())
    }

    fn object_exists(&self, type_name: &str, object_id: &str) -> Result<bool> {
        Ok(self
            .objects
            .get(type_name)
            .map(|m| m.contains_key(object_id))
            .unwrap_or(false))
    }

    fn inspect(&self) -> Result<StorageSummary> {
        let object_count = self.objects.values().map(|m| m.len()).sum();
        let schema_count = self.schemas.len();
        Ok(StorageSummary {
            storage_backend: "memory".to_string(),
            total_size_bytes: 0,
            schema_count,
            object_type_count: self.objects.len(),
            object_count,
            precomputed_queries: self.precomputed_queries.keys().cloned().collect(),
        })
    }

    fn set_database_name(&mut self, name: &str) -> Result<()> {
        self.database_name = if name.trim().is_empty() {
            "kelp-db".to_string()
        } else {
            name.to_string()
        };
        Ok(())
    }

    fn database_name(&self) -> Result<String> {
        Ok(self.database_name.clone())
    }

    fn save_precomputed_query(&mut self, name: &str, query: &str) -> Result<()> {
        self.precomputed_queries
            .insert(name.to_string(), query.to_string());
        Ok(())
    }

    fn list_precomputed_queries(&self) -> Result<Vec<String>> {
        Ok(self.precomputed_queries.keys().cloned().collect())
    }

    fn begin_transaction(&mut self) -> Result<()> {
        if self.in_transaction {
            return Err(crate::Error::TransactionError {
                reason: "Transaction already in progress".to_string(),
            });
        }
        self.transaction_snapshot = Some((self.schemas.clone(), self.objects.clone()));
        self.in_transaction = true;
        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        if !self.in_transaction {
            return Err(crate::Error::TransactionError {
                reason: "No transaction in progress".to_string(),
            });
        }
        self.in_transaction = false;
        self.transaction_snapshot = None;
        Ok(())
    }

    fn rollback(&mut self) -> Result<()> {
        if !self.in_transaction {
            return Err(crate::Error::TransactionError {
                reason: "No transaction in progress".to_string(),
            });
        }
        if let Some((schemas, objects)) = self.transaction_snapshot.take() {
            self.schemas = schemas;
            self.objects = objects;
        }
        self.in_transaction = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{FieldDef, FieldType};

    #[test]
    fn test_memory_storage_schema() {
        let mut storage = MemoryStorage::new();
        let schema =
            Schema::new("Customer").add_field(FieldDef::new("id", FieldType::String).required());

        assert!(storage.put_schema(&schema).is_ok());
        let retrieved = storage.get_schema("Customer").unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_memory_storage_object() {
        let mut storage = MemoryStorage::new();
        let mut obj = Object::new("Customer", "c1");
        obj.set_field("name", crate::Value::String("Alice".to_string()));

        assert!(storage.put_object(&obj).is_ok());
        let retrieved = storage.get_object("Customer", "c1").unwrap();
        assert!(retrieved.is_some());
    }
}
