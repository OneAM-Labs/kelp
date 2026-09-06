use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The types that a field can have in a schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    /// String type
    String,
    /// Integer type (64-bit signed)
    Integer,
    /// Float type (64-bit)
    Float,
    /// Boolean type
    Boolean,
    /// Reference type (points to another object)
    Reference,
}

impl FieldType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FieldType::String => "String",
            FieldType::Integer => "Integer",
            FieldType::Float => "Float",
            FieldType::Boolean => "Boolean",
            FieldType::Reference => "Reference",
        }
    }
}

/// Definition of a single field in an object schema.
///
/// Fields define the structure and constraints for data stored in objects.
/// Use the builder pattern to configure field properties.
///
/// # Properties
/// - `name`: The field name
/// - `field_type`: The data type (String, Integer, Float, Boolean, Reference)
/// - `required`: Whether the field must always have a value
/// - `nullable`: Whether the field can be null
/// - `reference_type`: For Reference fields, the target object type
/// - `default`: A default value if not provided
///
/// # Examples
///
/// ```ignore
/// use kelp_db::{FieldDef, FieldType};
///
/// // Required string field
/// let name_field = FieldDef::new("name", FieldType::String).required();
///
/// // Optional integer field
/// let age_field = FieldDef::new("age", FieldType::Integer);
///
/// // Reference to another object
/// let author_field = FieldDef::new("author", FieldType::Reference)
///     .references("Author");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    /// Field name
    pub name: String,
    /// Field type
    pub field_type: FieldType,
    /// Whether this field is required (must have a value)
    pub required: bool,
    /// Whether this field can be null
    pub nullable: bool,
    /// Default value (if any)
    pub default: Option<String>,
    /// For Reference types, the target object type
    pub reference_type: Option<String>,
}

impl FieldDef {
    /// Create a new field definition.
    pub fn new(name: impl Into<String>, field_type: FieldType) -> Self {
        Self {
            name: name.into(),
            field_type,
            required: false,
            nullable: false,
            default: None,
            reference_type: None,
        }
    }

    /// Mark this field as required.
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Mark this field as nullable.
    pub fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    /// Set reference target type (for Reference fields).
    pub fn references(mut self, target_type: impl Into<String>) -> Self {
        self.reference_type = Some(target_type.into());
        self
    }

    /// Set a default value.
    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }
}

/// Schema definition for an object type.
///
/// A schema defines the structure of all objects of a particular type.
/// Each schema has a name and a set of fields with their types and constraints.
///
/// # Examples
///
/// ```ignore
/// use kelp_db::{Schema, FieldDef, FieldType};
///
/// // Create a schema for a User type
/// let user_schema = Schema::new("User")
///     .add_field(FieldDef::new("id", FieldType::String).required())
///     .add_field(FieldDef::new("name", FieldType::String).required())
///     .add_field(FieldDef::new("email", FieldType::String).required())
///     .add_field(FieldDef::new("age", FieldType::Integer))
///     .add_field(FieldDef::new("active", FieldType::Boolean));
///
/// db.create_type(&user_schema)?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    /// Name of the object type (e.g., "Customer", "Order")
    pub name: String,
    /// Fields in this schema
    pub fields: HashMap<String, FieldDef>,
}

impl Schema {
    /// Create a new schema with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fields: HashMap::new(),
        }
    }

    /// Add a field to this schema.
    pub fn add_field(mut self, field: FieldDef) -> Self {
        self.fields.insert(field.name.clone(), field);
        self
    }

    /// Get a field by name.
    pub fn get_field(&self, name: &str) -> Option<&FieldDef> {
        self.fields.get(name)
    }

    /// Check if a field exists.
    pub fn has_field(&self, name: &str) -> bool {
        self.fields.contains_key(name)
    }

    /// Get all field names.
    pub fn field_names(&self) -> impl Iterator<Item = &String> {
        self.fields.keys()
    }

    /// Validate schema for correctness.
    pub fn validate(&self) -> crate::Result<()> {
        if self.name.is_empty() {
            return Err(crate::Error::SchemaError {
                reason: "Schema name cannot be empty".to_string(),
            });
        }

        for field in self.fields.values() {
            // Reference fields must have a reference_type
            if field.field_type == FieldType::Reference && field.reference_type.is_none() {
                return Err(crate::Error::SchemaError {
                    reason: format!(
                        "Reference field '{}' must specify a reference_type",
                        field.name
                    ),
                });
            }

            // Non-nullable fields should not be nullable
            if field.required && field.nullable {
                return Err(crate::Error::SchemaError {
                    reason: format!(
                        "Field '{}' cannot be both required and nullable",
                        field.name
                    ),
                });
            }
        }

        Ok(())
    }
}

/// Reference to an object type (by name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectType {
    pub name: String,
}

impl ObjectType {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl AsRef<str> for ObjectType {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_def_creation() {
        let field = FieldDef::new("name", FieldType::String)
            .required()
            .nullable();
        assert_eq!(field.name, "name");
        assert_eq!(field.field_type, FieldType::String);
        assert!(field.required);
        assert!(field.nullable);
    }

    #[test]
    fn test_schema_creation() {
        let schema = Schema::new("Customer")
            .add_field(FieldDef::new("id", FieldType::String).required())
            .add_field(FieldDef::new("name", FieldType::String).required())
            .add_field(FieldDef::new("email", FieldType::String).nullable());

        assert!(schema.has_field("id"));
        assert!(schema.has_field("name"));
        assert!(schema.has_field("email"));
        assert!(!schema.has_field("nonexistent"));
    }

    #[test]
    fn test_schema_validation() {
        let valid_schema = Schema::new("Customer")
            .add_field(FieldDef::new("id", FieldType::String).required())
            .add_field(FieldDef::new("order", FieldType::Reference).references("Order"));

        assert!(valid_schema.validate().is_ok());

        let invalid_schema =
            Schema::new("Customer").add_field(FieldDef::new("ref", FieldType::Reference));

        assert!(invalid_schema.validate().is_err());
    }
}
