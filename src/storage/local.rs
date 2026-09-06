use crate::object::Object;
use crate::schema::Schema;
use crate::storage::StorageSummary;
use crate::Result;
use serde_json;
use std::fs;
use std::path::{Path, PathBuf};

use super::StorageBackend;

/// Local file-based storage implementation.
///
/// Stores schemas and objects as JSON files in a directory on the filesystem.
/// Directory structure:
///   data/
///     .kelp/
///       queries.json
///     schemas/
///       TypeName.json
///     objects/
///       TypeName/
///         object_id.json
pub struct LocalStorage {
    data_dir: PathBuf,
    in_transaction: bool,
    transaction_backup: Option<String>,
}

impl LocalStorage {
    /// Create a new local storage backend.
    ///
    /// The data directory will be created if it doesn't exist.
    pub fn new(data_dir: impl AsRef<Path>) -> Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();

        fs::create_dir_all(&data_dir).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to create data directory: {}", e),
        })?;

        fs::create_dir_all(data_dir.join("schemas")).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to create schemas directory: {}", e),
        })?;

        fs::create_dir_all(data_dir.join("objects")).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to create objects directory: {}", e),
        })?;

        fs::create_dir_all(data_dir.join(".kelp")).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to create metadata directory: {}", e),
        })?;

        Ok(Self {
            data_dir,
            in_transaction: false,
            transaction_backup: None,
        })
    }

    fn schemas_dir(&self) -> PathBuf {
        self.data_dir.join("schemas")
    }

    fn objects_dir(&self) -> PathBuf {
        self.data_dir.join("objects")
    }

    fn metadata_dir(&self) -> PathBuf {
        self.data_dir.join(".kelp")
    }

    fn schema_path(&self, name: &str) -> PathBuf {
        self.schemas_dir().join(format!("{}.json", name))
    }

    fn type_objects_dir(&self, type_name: &str) -> PathBuf {
        self.objects_dir().join(type_name)
    }

    fn object_path(&self, type_name: &str, object_id: &str) -> PathBuf {
        self.type_objects_dir(type_name)
            .join(format!("{}.json", object_id))
    }

    fn queries_path(&self) -> PathBuf {
        self.metadata_dir().join("precomputed_queries.json")
    }

    fn database_name_path(&self) -> PathBuf {
        self.metadata_dir().join("database_name.txt")
    }

    fn dir_size(path: &Path) -> Result<u64> {
        let mut total = 0u64;
        if path.is_file() {
            return Ok(fs::metadata(path)
                .map_err(|e| crate::Error::StorageError {
                    reason: format!("Failed to stat file {}: {}", path.display(), e),
                })?
                .len());
        }

        for entry in fs::read_dir(path).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to read directory {}: {}", path.display(), e),
        })? {
            let entry = entry.map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to read directory entry {}: {}", path.display(), e),
            })?;
            total += Self::dir_size(&entry.path())?;
        }

        Ok(total)
    }
}

impl StorageBackend for LocalStorage {
    fn put_schema(&mut self, schema: &Schema) -> Result<()> {
        schema.validate()?;
        let schema_path = self.schema_path(&schema.name);
        let json = serde_json::to_string_pretty(schema)?;
        fs::write(&schema_path, json).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to write schema: {}", e),
        })?;
        Ok(())
    }

    fn get_schema(&self, name: &str) -> Result<Option<Schema>> {
        let schema_path = self.schema_path(name);
        if !schema_path.exists() {
            return Ok(None);
        }

        let contents =
            fs::read_to_string(&schema_path).map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to read schema: {}", e),
            })?;

        let schema = serde_json::from_str(&contents)?;
        Ok(Some(schema))
    }

    fn list_schemas(&self) -> Result<Vec<String>> {
        let schemas_dir = self.schemas_dir();
        if !schemas_dir.exists() {
            return Ok(vec![]);
        }

        let mut names = vec![];
        for entry in fs::read_dir(&schemas_dir).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to list schemas: {}", e),
        })? {
            let entry = entry.map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to read directory entry: {}", e),
            })?;

            if let Some(name) = entry
                .file_name()
                .to_str()
                .map(|s| s.to_string())
                .filter(|s| s.ends_with(".json"))
                .map(|s| s.trim_end_matches(".json").to_string())
            {
                names.push(name);
            }
        }

        Ok(names)
    }

    fn put_object(&mut self, object: &Object) -> Result<()> {
        let type_dir = self.type_objects_dir(&object.id.type_name);
        fs::create_dir_all(&type_dir).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to create type directory: {}", e),
        })?;

        let object_path = self.object_path(&object.id.type_name, &object.id.object_id);
        let json = serde_json::to_string_pretty(object)?;
        fs::write(&object_path, json).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to write object: {}", e),
        })?;

        Ok(())
    }

    fn get_object(&self, type_name: &str, object_id: &str) -> Result<Option<Object>> {
        let object_path = self.object_path(type_name, object_id);
        if !object_path.exists() {
            return Ok(None);
        }

        let contents =
            fs::read_to_string(&object_path).map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to read object: {}", e),
            })?;

        let obj = serde_json::from_str(&contents)?;
        Ok(Some(obj))
    }

    fn delete_object(&mut self, type_name: &str, object_id: &str) -> Result<()> {
        let object_path = self.object_path(type_name, object_id);
        if object_path.exists() {
            fs::remove_file(&object_path).map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to delete object: {}", e),
            })?;
        }
        Ok(())
    }

    fn list_objects(&self, type_name: &str) -> Result<Vec<Object>> {
        let type_dir = self.type_objects_dir(type_name);
        if !type_dir.exists() {
            return Ok(vec![]);
        }

        let mut objects = vec![];
        for entry in fs::read_dir(&type_dir).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to list objects: {}", e),
        })? {
            let entry = entry.map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to read directory entry: {}", e),
            })?;

            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let contents =
                    fs::read_to_string(&path).map_err(|e| crate::Error::StorageError {
                        reason: format!("Failed to read object: {}", e),
                    })?;

                let obj = serde_json::from_str(&contents)?;
                objects.push(obj);
            }
        }

        Ok(objects)
    }

    fn object_exists(&self, type_name: &str, object_id: &str) -> Result<bool> {
        let object_path = self.object_path(type_name, object_id);
        Ok(object_path.exists())
    }

    fn inspect(&self) -> Result<StorageSummary> {
        let schemas = self.list_schemas()?;
        let mut object_types = 0usize;
        let mut object_count = 0usize;

        if self.objects_dir().exists() {
            for entry in
                fs::read_dir(self.objects_dir()).map_err(|e| crate::Error::StorageError {
                    reason: format!("Failed to list object types: {}", e),
                })?
            {
                let entry = entry.map_err(|e| crate::Error::StorageError {
                    reason: format!("Failed to read object type entry: {}", e),
                })?;

                if entry.path().is_dir() {
                    object_types += 1;
                    for child in
                        fs::read_dir(entry.path()).map_err(|e| crate::Error::StorageError {
                            reason: format!(
                                "Failed to list objects for type {}: {}",
                                entry.file_name().to_string_lossy(),
                                e
                            ),
                        })?
                    {
                        let child = child.map_err(|e| crate::Error::StorageError {
                            reason: format!("Failed to read child entry: {}", e),
                        })?;
                        if child.path().extension().is_some_and(|ext| ext == "json") {
                            object_count += 1;
                        }
                    }
                }
            }
        }

        let precomputed_queries = self.list_precomputed_queries()?;
        Ok(StorageSummary {
            storage_backend: "local-filesystem".to_string(),
            total_size_bytes: Self::dir_size(&self.data_dir)?,
            schema_count: schemas.len(),
            object_type_count: object_types,
            object_count,
            precomputed_queries,
        })
    }

    fn set_database_name(&mut self, name: &str) -> Result<()> {
        let clean = name.trim();
        if clean.is_empty() {
            return Ok(());
        }
        self.write_database_name(clean)
    }

    fn database_name(&self) -> Result<String> {
        self.read_database_name()
    }

    fn save_precomputed_query(&mut self, name: &str, query: &str) -> Result<()> {
        let queries_path = self.queries_path();
        let mut queries: std::collections::HashMap<String, String> = if queries_path.exists() {
            match fs::read_to_string(&queries_path) {
                Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
                Err(_) => std::collections::HashMap::new(),
            }
        } else {
            std::collections::HashMap::new()
        };

        queries.insert(name.to_string(), query.to_string());
        let json = serde_json::to_string_pretty(&queries)?;
        fs::write(&queries_path, json).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to save precomputed query: {}", e),
        })?;
        Ok(())
    }

    fn list_precomputed_queries(&self) -> Result<Vec<String>> {
        let queries_path = self.queries_path();
        if !queries_path.exists() {
            return Ok(vec![]);
        }

        let contents =
            fs::read_to_string(&queries_path).map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to read precomputed queries: {}", e),
            })?;

        let queries: std::collections::HashMap<String, String> =
            serde_json::from_str(&contents).unwrap_or_default();
        Ok(queries.keys().cloned().collect())
    }

    fn begin_transaction(&mut self) -> Result<()> {
        if self.in_transaction {
            return Err(crate::Error::TransactionError {
                reason: "Transaction already in progress".to_string(),
            });
        }

        let backup_dir = self.data_dir.join(".backup");
        if backup_dir.exists() {
            fs::remove_dir_all(&backup_dir).map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to clean old backup: {}", e),
            })?;
        }

        self.copy_dir_all(&self.data_dir, &backup_dir)?;

        self.in_transaction = true;
        self.transaction_backup = Some(backup_dir.to_string_lossy().to_string());

        Ok(())
    }

    fn commit(&mut self) -> Result<()> {
        if !self.in_transaction {
            return Err(crate::Error::TransactionError {
                reason: "No transaction in progress".to_string(),
            });
        }

        if let Some(backup_path) = &self.transaction_backup {
            let backup_dir = Path::new(backup_path);
            if backup_dir.exists() {
                fs::remove_dir_all(backup_dir).map_err(|e| crate::Error::StorageError {
                    reason: format!("Failed to delete backup: {}", e),
                })?;
            }
        }

        self.in_transaction = false;
        self.transaction_backup = None;
        Ok(())
    }

    fn rollback(&mut self) -> Result<()> {
        if !self.in_transaction {
            return Err(crate::Error::TransactionError {
                reason: "No transaction in progress".to_string(),
            });
        }

        if let Some(backup_path) = &self.transaction_backup {
            let backup_dir = Path::new(backup_path);
            fs::remove_dir_all(&self.data_dir).map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to remove current data: {}", e),
            })?;
            self.copy_dir_all(backup_dir, &self.data_dir)?;
            fs::remove_dir_all(backup_dir).map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to delete backup: {}", e),
            })?;
        }

        self.in_transaction = false;
        self.transaction_backup = None;
        Ok(())
    }
}

impl LocalStorage {
    fn copy_dir_all(&self, src: &Path, dst: &Path) -> Result<()> {
        fs::create_dir_all(dst).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to create directory: {}", e),
        })?;

        for entry in fs::read_dir(src).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to read directory: {}", e),
        })? {
            let entry = entry.map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to read entry: {}", e),
            })?;

            let path = entry.path();
            let file_name = entry.file_name();
            let dest_path = dst.join(&file_name);

            if path.is_dir() {
                self.copy_dir_all(&path, &dest_path)?;
            } else {
                fs::copy(&path, &dest_path).map_err(|e| crate::Error::StorageError {
                    reason: format!("Failed to copy file: {}", e),
                })?;
            }
        }

        Ok(())
    }

    fn write_database_name(&self, name: &str) -> Result<()> {
        let path = self.database_name_path();
        fs::write(path, name.trim()).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to write database name: {}", e),
        })?;
        Ok(())
    }

    fn read_database_name(&self) -> Result<String> {
        let path = self.database_name_path();
        if !path.exists() {
            return Ok(self
                .data_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("kelp-db")
                .to_string());
        }

        let name = fs::read_to_string(path).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to read database name: {}", e),
        })?;
        Ok(name.trim().to_string())
    }
}
