use crate::model::Model;
use crate::model::ModelProjector;
use crate::object::Object;
use crate::query::{Predicate, QueryResult};
use crate::reference::{Reference, ReferenceTracker};
use crate::schema::Schema;
use crate::storage::{LocalStorage, StorageBackend};
use crate::transaction::Transaction;
use crate::validation::Validator;
use crate::value::Value;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A compact overview of the current database state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSummary {
    /// User-facing name of the database, derived from the folder name.
    pub name: String,
    /// Absolute filesystem path to the database root.
    pub path: String,
    /// Storage backend label.
    pub storage_backend: String,
    /// Total bytes consumed by the database on disk.
    pub total_size_bytes: u64,
    /// Number of schema definitions.
    pub schema_count: usize,
    /// Number of object types currently stored.
    pub object_type_count: usize,
    /// Number of objects stored across all types.
    pub object_count: usize,
    /// Named precomputed queries persisted for this database.
    pub precomputed_queries: Vec<String>,
}

/// The main database API.
///
/// This is the primary interface for Kelp. Applications use Database to:
/// - Define object types (schemas)
/// - Create, read, update, delete objects
/// - Query objects
/// - Define and use models/projections
/// - Manage transactions
///
/// # Examples
///
/// ## Creating a database and defining a schema
///
/// ```ignore
/// use kelp_db::{Database, Schema, FieldDef, FieldType, Object, Value};
///
/// // Open or create a local database
/// let db = Database::open_local("./my_app.db")?;
///
/// // Define a schema
/// let user_schema = Schema::new("User")
///     .add_field(FieldDef::new("name", FieldType::String).required())
///     .add_field(FieldDef::new("email", FieldType::String).required())
///     .add_field(FieldDef::new("age", FieldType::Integer));
///
/// db.create_type(&user_schema)?;
/// ```
///
/// ## Creating and retrieving objects
///
/// ```ignore
/// // Create a new user object
/// let mut user = Object::new("User", "u123");
/// user.set_field("name", Value::String("Alice".to_string()));
/// user.set_field("email", Value::String("alice@example.com".to_string()));
/// user.set_field("age", Value::Integer(30));
///
/// db.create_object(&user)?;
///
/// // Retrieve it
/// if let Some(retrieved) = db.get_object("User", "u123")? {
///     println!("User: {:?}", retrieved);
/// }
/// ```
///
/// ## Querying objects
///
/// ```ignore
/// use kelp_db::Predicate;
///
/// // Find all users with age > 25
/// let results = db.query("User", &Predicate::gt("age", Value::Integer(25)))?;
/// for user in &results.objects {
///     println!("User: {}", user.id);
/// }
/// ```
#[derive(Clone)]
pub struct Database {
    storage: Arc<Mutex<Box<dyn StorageBackend>>>,
    transaction: Arc<Mutex<Transaction>>,
    path: std::path::PathBuf,
}

impl Database {
    /// Open a local database at a given path.
    ///
    /// Creates a new database rooted at the supplied directory. The directory is
    /// created automatically if it does not exist.
    pub fn open_local(data_dir: impl AsRef<std::path::Path>) -> Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        let mut storage = Box::new(LocalStorage::new(&data_dir)?);
        let default_name = Self::name_for_path(&data_dir);
        if storage.database_name().is_ok() {
            // keep the persisted name if present; otherwise initialize from the directory name.
        } else {
            let _ = storage.set_database_name(&default_name);
        }
        let name = storage.database_name().unwrap_or_else(|_| default_name.clone());
        if name == "kelp-db" || storage.database_name().unwrap_or_default() == "kelp-db" {
            let _ = storage.set_database_name(&default_name);
        }
        Ok(Self {
            storage: Arc::new(Mutex::new(storage)),
            transaction: Arc::new(Mutex::new(Transaction::new())),
            path: data_dir,
        })
    }

    /// Create a new in-memory database (useful for testing).
    pub fn memory() -> Result<Self> {
        let mut storage = Box::new(crate::storage::MemoryStorage::new());
        let _ = storage.set_database_name("memory");
        Ok(Self {
            storage: Arc::new(Mutex::new(storage)),
            transaction: Arc::new(Mutex::new(Transaction::new())),
            path: std::path::PathBuf::from("memory://kelp"),
        })
    }

    /// Resolve a database name from its on-disk path.
    pub fn name_for_path(path: impl AsRef<Path>) -> String {
        let path = path.as_ref();
        path.file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "kelp-db".to_string())
    }

    /// Return the path this database is bound to.
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Return a compact health summary for this database.
    pub fn inspect(&self) -> Result<DatabaseSummary> {
        let summary = self.storage.lock().unwrap().inspect()?;
        let name = self.name().unwrap_or_else(|_| Self::name_for_path(&self.path));
        Ok(DatabaseSummary {
            name,
            path: self.path.display().to_string(),
            storage_backend: summary.storage_backend,
            total_size_bytes: summary.total_size_bytes,
            schema_count: summary.schema_count,
            object_type_count: summary.object_type_count,
            object_count: summary.object_count,
            precomputed_queries: summary.precomputed_queries,
        })
    }

    /// Save a named query so it can be reused and surfaced in inspections.
    pub fn save_precomputed_query(&self, name: &str, query: &str) -> Result<()> {
        let mut storage = self.storage.lock().unwrap();
        storage.save_precomputed_query(name, query)
    }

    /// List the names of saved precomputed queries.
    pub fn list_precomputed_queries(&self) -> Result<Vec<String>> {
        let storage = self.storage.lock().unwrap();
        storage.list_precomputed_queries()
    }

    /// Return the current human-facing database name.
    pub fn name(&self) -> Result<String> {
        let storage = self.storage.lock().unwrap();
        storage.database_name()
    }

    /// Assign a user-friendly database name and persist it in the database metadata.
    pub fn set_name(&self, name: &str) -> Result<()> {
        let mut storage = self.storage.lock().unwrap();
        storage.set_database_name(name)
    }

    /// Query objects by field value.
    ///
    /// Finds all objects of a given type where a specific field matches a value.
    /// The query is performed with an equality predicate.
    ///
    /// # Arguments
    /// * `type_name` - The object type to query
    /// * `field` - The field name to match on
    /// * `value` - The value to match exactly
    ///
    /// # Returns
    /// A `QueryResult` containing all matching objects.
    ///
    /// # Example
    /// ```ignore
    /// let results = db.query_by_field("User", "email", 
    ///     Value::String("alice@example.com".to_string()))?;
    /// for user in &results.objects {
    ///     println!("Found: {}", user.id);
    /// }
    /// ```
    pub fn query_by_field(&self, type_name: &str, field: &str, value: Value) -> Result<QueryResult> {
        self.query(type_name, &Predicate::equals(field, value))
    }

    /// Return the first object whose field matches an exact value.
    pub fn find_by_field(&self, type_name: &str, field: &str, value: Value) -> Result<Option<Object>> {
        self.query_one(type_name, &Predicate::equals(field, value))
    }

    /// Update one field on an existing object without rewriting the whole record manually.
    pub fn update_field(
        &self,
        type_name: &str,
        object_id: &str,
        field: &str,
        value: Value,
    ) -> Result<()> {
        let mut object = self
            .get_object(type_name, object_id)?
            .ok_or_else(|| Error::ObjectNotFound {
                type_name: type_name.to_string(),
                object_id: object_id.to_string(),
            })?;
        object.set_field(field, value);
        self.update_object(&object)
    }

    // ===== Schema Operations =====

    /// Create or update an object type schema.
    ///
    /// Defines the structure of objects of a particular type. Each schema specifies
    /// the fields, their types, and constraints (required, nullable, etc.).
    ///
    /// # Arguments
    /// * `schema` - The schema definition to register
    ///
    /// # Errors
    /// Returns an error if the schema is invalid or storage fails.
    ///
    /// # Example
    /// ```ignore
    /// let schema = Schema::new("Product")
    ///     .add_field(FieldDef::new("name", FieldType::String).required())
    ///     .add_field(FieldDef::new("price", FieldType::Float).required())
    ///     .add_field(FieldDef::new("stock", FieldType::Integer));
    /// db.create_type(&schema)?;
    /// ```
    pub fn create_type(&self, schema: &Schema) -> Result<()> {
        schema.validate()?;
        let mut storage = self.storage.lock().unwrap();
        storage.put_schema(schema)?;
        Ok(())
    }

    /// Retrieve a schema by type name.
    ///
    /// Returns the full schema definition including all fields and constraints.
    ///
    /// # Arguments
    /// * `name` - The name of the schema to retrieve
    ///
    /// # Returns
    /// - `Some(Schema)` if the schema exists
    /// - `None` if the schema was not found
    ///
    /// # Example
    /// ```ignore
    /// if let Some(schema) = db.get_type("User")? {
    ///     println!("Schema for User type:");
    ///     for field_name in schema.field_names() {
    ///         if let Some(field) = schema.get_field(field_name) {
    ///             println!("  {}: {}", field.name, field.field_type.as_str());
    ///         }
    ///     }
    /// }
    /// ```
    pub fn get_type(&self, name: &str) -> Result<Option<Schema>> {
        let storage = self.storage.lock().unwrap();
        storage.get_schema(name)
    }

    /// List all defined object types.
    ///
    /// Returns the names of all schemas that have been defined in this database.
    ///
    /// # Returns
    /// A vector of type names.
    ///
    /// # Example
    /// ```ignore
    /// let types = db.list_types()?;
    /// for type_name in types {
    ///     println!("Type: {}", type_name);
    /// }
    /// ```
    pub fn list_types(&self) -> Result<Vec<String>> {
        let storage = self.storage.lock().unwrap();
        storage.list_schemas()
    }

    // ===== Object Operations =====

    /// Create a new object.
    ///
    /// Validates the object against its type schema before saving.
    /// The object's type must have been defined first via `create_type`.
    ///
    /// # Arguments
    /// * `object` - The object to store
    ///
    /// # Errors
    /// Returns an error if:
    /// - The object's type schema is not defined
    /// - The object fails validation against the schema
    /// - Storage operations fail
    ///
    /// # Example
    /// ```ignore
    /// let mut product = Object::new("Product", "p123");
    /// product.set_field("name", Value::String("Laptop".to_string()));
    /// product.set_field("price", Value::Float(999.99));
    /// db.create_object(&product)?;
    /// ```
    pub fn create_object(&self, object: &Object) -> Result<()> {
        let schema = self
            .get_type(&object.id.type_name)?
            .ok_or_else(|| Error::TypeNotFound {
                type_name: object.id.type_name.clone(),
            })?;

        Validator::validate(object, &schema)?;

        let mut storage = self.storage.lock().unwrap();
        if storage.object_exists(&object.id.type_name, &object.id.object_id)? {
            return Err(Error::Internal(format!(
                "Object {}/{} already exists",
                object.id.type_name, object.id.object_id
            )));
        }

        storage.put_object(object)?;
        Ok(())
    }

    /// Retrieve an object by type and ID.
    pub fn get_object(&self, type_name: &str, object_id: &str) -> Result<Option<Object>> {
        let storage = self.storage.lock().unwrap();
        storage.get_object(type_name, object_id)
    }

    /// Update an existing object.
    ///
    /// Validates against the schema before saving.
    pub fn update_object(&self, object: &Object) -> Result<()> {
        let schema = self
            .get_type(&object.id.type_name)?
            .ok_or_else(|| Error::TypeNotFound {
                type_name: object.id.type_name.clone(),
            })?;

        Validator::validate(object, &schema)?;

        let storage = self.storage.lock().unwrap();
        if !storage.object_exists(&object.id.type_name, &object.id.object_id)? {
            return Err(Error::ObjectNotFound {
                type_name: object.id.type_name.clone(),
                object_id: object.id.object_id.clone(),
            });
        }

        let mut storage = storage;
        storage.put_object(object)?;
        Ok(())
    }

    /// Delete an object.
    pub fn delete_object(&self, type_name: &str, object_id: &str) -> Result<()> {
        let mut storage = self.storage.lock().unwrap();
        if !storage.object_exists(type_name, object_id)? {
            return Err(Error::ObjectNotFound {
                type_name: type_name.to_string(),
                object_id: object_id.to_string(),
            });
        }
        storage.delete_object(type_name, object_id)?;
        Ok(())
    }

    /// List all objects of a type.
    pub fn list_objects(&self, type_name: &str) -> Result<QueryResult> {
        let storage = self.storage.lock().unwrap();
        let objects = storage.list_objects(type_name)?;
        Ok(QueryResult::new(objects))
    }

    // ===== Query Operations =====

    /// Query objects of a type using a predicate.
    pub fn query(&self, type_name: &str, predicate: &Predicate) -> Result<QueryResult> {
        let storage = self.storage.lock().unwrap();
        let objects = storage.list_objects(type_name)?;

        let mut results = Vec::new();
        for obj in objects {
            if predicate.matches(&obj)? {
                results.push(obj);
            }
        }

        Ok(QueryResult::new(results))
    }

    /// Return the first matching object for a predicate, if any.
    pub fn query_one(&self, type_name: &str, predicate: &Predicate) -> Result<Option<Object>> {
        Ok(self.query(type_name, predicate)?.objects.into_iter().next())
    }

    /// Return the number of objects in a type.
    pub fn count_objects(&self, type_name: &str) -> Result<usize> {
        Ok(self.list_objects(type_name)?.len())
    }

    // ===== Reference Operations =====

    /// Resolve a reference to get the target object.
    pub fn resolve_reference(&self, reference: &Reference) -> Result<Option<Object>> {
        self.get_object(&reference.target.type_name, &reference.target.object_id)
    }

    /// Resolve a reference from a field value.
    pub fn resolve_reference_value(&self, value: &Value) -> Result<Option<Object>> {
        if let Some(ref_str) = value.as_reference() {
            let reference = Reference::parse(ref_str)?;
            self.resolve_reference(&reference)
        } else {
            Err(Error::Internal("Value is not a reference".to_string()))
        }
    }

    /// Resolve nested references in an object (e.g., order.customer.name).
    pub fn resolve_nested(&self, object: &Object, path: &[&str]) -> Result<Option<Value>> {
        if path.is_empty() {
            return Err(Error::Internal("Empty path".to_string()));
        }

        let mut current_obj = object.clone();
        let mut tracker = ReferenceTracker::new(10);

        for (i, field_name) in path.iter().enumerate() {
            tracker.visit(&current_obj.id.to_string())?;

            if i == path.len() - 1 {
                return Ok(current_obj.get_field(field_name).cloned());
            }

            if let Some(Value::Reference(ref_str)) = current_obj.get_field(field_name) {
                let reference = Reference::parse(ref_str)?;
                if let Some(target) = self.resolve_reference(&reference)? {
                    current_obj = target;
                } else {
                    return Err(Error::InvalidReference {
                        type_name: reference.target.type_name.clone(),
                        object_id: reference.target.object_id.clone(),
                    });
                }
            } else {
                return Ok(None);
            }
        }

        Ok(None)
    }

    // ===== Model Operations =====

    /// Define a new model/projection.
    pub fn create_model(&self, model: &Model) -> Result<()> {
        model.validate()?;

        self.get_type(&model.base_type)?
            .ok_or_else(|| Error::TypeNotFound {
                type_name: model.base_type.clone(),
            })?;

        Ok(())
    }

    /// Project an object using a model.
    pub fn project_object(&self, object: &Object, model: &Model) -> Result<serde_json::Value> {
        if object.id.type_name != model.base_type {
            return Err(Error::ModelError {
                reason: format!(
                    "Object type {} does not match model base type {}",
                    object.id.type_name, model.base_type
                ),
            });
        }

        let storage = Arc::clone(&self.storage);
        ModelProjector::project(object, model, move |ref_str| {
            let reference = Reference::parse(ref_str)?;
            let storage = storage.lock().unwrap();
            storage.get_object(&reference.target.type_name, &reference.target.object_id)
        })
    }

    // ===== Transaction Operations =====

    /// Begin a transaction.
    pub fn begin_transaction(&self) -> Result<()> {
        let mut storage = self.storage.lock().unwrap();
        storage.begin_transaction()?;

        let mut tx = self.transaction.lock().unwrap();
        tx.begin()?;

        Ok(())
    }

    /// Commit the current transaction.
    pub fn commit(&self) -> Result<()> {
        let mut storage = self.storage.lock().unwrap();
        storage.commit()?;

        let mut tx = self.transaction.lock().unwrap();
        tx.commit()?;

        Ok(())
    }

    /// Rollback the current transaction.
    pub fn rollback(&self) -> Result<()> {
        let mut storage = self.storage.lock().unwrap();
        storage.rollback()?;

        let mut tx = self.transaction.lock().unwrap();
        tx.rollback()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::FieldDef;
    use crate::schema::FieldType;

    #[test]
    fn test_database_create_type() {
        let db = Database::memory().unwrap();
        let schema =
            Schema::new("Customer").add_field(FieldDef::new("id", FieldType::String).required());

        assert!(db.create_type(&schema).is_ok());
        let retrieved = db.get_type("Customer").unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_database_create_object() {
        let db = Database::memory().unwrap();

        let schema = Schema::new("Customer")
            .add_field(FieldDef::new("id", FieldType::String).required())
            .add_field(FieldDef::new("name", FieldType::String).required());
        db.create_type(&schema).unwrap();

        let mut obj = Object::new("Customer", "c1");
        obj.set_field("id", Value::String("c1".to_string()));
        obj.set_field("name", Value::String("Alice".to_string()));

        assert!(db.create_object(&obj).is_ok());
        let retrieved = db.get_object("Customer", "c1").unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_database_query() {
        let db = Database::memory().unwrap();

        let schema = Schema::new("Order")
            .add_field(FieldDef::new("id", FieldType::String).required())
            .add_field(FieldDef::new("status", FieldType::String).required());
        db.create_type(&schema).unwrap();

        let mut o1 = Object::new("Order", "o1");
        o1.set_field("id", Value::String("o1".to_string()));
        o1.set_field("status", Value::String("pending".to_string()));

        let mut o2 = Object::new("Order", "o2");
        o2.set_field("id", Value::String("o2".to_string()));
        o2.set_field("status", Value::String("shipped".to_string()));

        db.create_object(&o1).unwrap();
        db.create_object(&o2).unwrap();

        let pred = Predicate::equals("status", Value::String("pending".to_string()));
        let results = db.query("Order", &pred).unwrap();

        assert_eq!(results.len(), 1);
    }
}
