use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::value::Value;

/// Uniquely identifies an object in the database.
///
/// An object is identified by its type name and object ID.
/// Example: "Customer/c1", "Order/o1"
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectId {
    /// The type name
    pub type_name: String,
    /// The object ID within that type
    pub object_id: String,
}

impl ObjectId {
    /// Create a new object ID.
    pub fn new(type_name: impl Into<String>, object_id: impl Into<String>) -> Self {
        Self {
            type_name: type_name.into(),
            object_id: object_id.into(),
        }
    }

    /// Parse an object ID from a string like "Type/id".
    pub fn parse(s: &str) -> crate::Result<Self> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() != 2 {
            return Err(crate::Error::Internal(format!(
                "Invalid object ID format: {}. Expected 'Type/id'",
                s
            )));
        }
        Ok(Self::new(parts[0], parts[1]))
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.type_name, self.object_id)
    }
}

/// Represents an object instance in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Object {
    /// Object identity
    pub id: ObjectId,
    /// Field values
    pub fields: HashMap<String, Value>,
}

impl Object {
    /// Create a new object with the given ID.
    pub fn new(type_name: impl Into<String>, object_id: impl Into<String>) -> Self {
        Self {
            id: ObjectId::new(type_name, object_id),
            fields: HashMap::new(),
        }
    }

    /// Create from an ObjectId.
    pub fn with_id(id: ObjectId) -> Self {
        Self {
            id,
            fields: HashMap::new(),
        }
    }

    /// Set a field value.
    pub fn set_field(&mut self, name: impl Into<String>, value: Value) {
        self.fields.insert(name.into(), value);
    }

    /// Get a field value.
    pub fn get_field(&self, name: &str) -> Option<&Value> {
        self.fields.get(name)
    }

    /// Check if a field exists in this object.
    pub fn has_field(&self, name: &str) -> bool {
        self.fields.contains_key(name)
    }

    /// Get a mutable reference to a field value.
    pub fn get_field_mut(&mut self, name: &str) -> Option<&mut Value> {
        self.fields.get_mut(name)
    }

    /// Remove a field.
    pub fn remove_field(&mut self, name: &str) -> Option<Value> {
        self.fields.remove(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_id_creation() {
        let id = ObjectId::new("Customer", "c1");
        assert_eq!(id.type_name, "Customer");
        assert_eq!(id.object_id, "c1");
        assert_eq!(id.to_string(), "Customer/c1");
    }

    #[test]
    fn test_object_id_parse() {
        let id = ObjectId::parse("Customer/c1").unwrap();
        assert_eq!(id.type_name, "Customer");
        assert_eq!(id.object_id, "c1");
    }

    #[test]
    fn test_object_id_parse_invalid() {
        assert!(ObjectId::parse("InvalidFormat").is_err());
        assert!(ObjectId::parse("A/B/C").is_err());
    }

    #[test]
    fn test_object_creation() {
        let mut obj = Object::new("Customer", "c1");
        obj.set_field("name", Value::String("Alice".to_string()));

        assert_eq!(obj.id.type_name, "Customer");
        assert_eq!(obj.id.object_id, "c1");
        assert_eq!(
            obj.get_field("name"),
            Some(&Value::String("Alice".to_string()))
        );
    }

    #[test]
    fn test_object_field_operations() {
        let mut obj = Object::new("Customer", "c1");
        obj.set_field("name", Value::String("Alice".to_string()));
        obj.set_field("age", Value::Integer(30));

        assert!(obj.has_field("name"));
        assert!(obj.has_field("age"));
        assert!(!obj.has_field("nonexistent"));

        let removed = obj.remove_field("age");
        assert!(removed.is_some());
        assert!(!obj.has_field("age"));
    }
}
