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
    /// Schema version; bump when field meaning or layout changes.
    pub schema_version: u32,
    /// Fields in this schema
    pub fields: HashMap<String, FieldDef>,
    /// Deterministic field ordering for schema-compiled physical layouts.
    pub field_order: Vec<String>,
}

impl Schema {
    /// Create a new schema with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema_version: 1,
            fields: HashMap::new(),
            field_order: Vec::new(),
        }
    }

    /// Add a field to this schema.
    pub fn add_field(mut self, field: FieldDef) -> Self {
        if !self.fields.contains_key(&field.name) {
            self.field_order.push(field.name.clone());
        }
        self.fields.insert(field.name.clone(), field);
        self
    }

    /// Get a stable field identifier for a field name.
    pub fn field_id(&self, name: &str) -> Option<FieldId> {
        self.field_order
            .iter()
            .position(|field_name| field_name == name)
            .map(|idx| FieldId::new((idx as u16) + 1))
    }

    /// Get the field for a stable field ID.
    pub fn field_by_id(&self, field_id: FieldId) -> Option<(&String, &FieldDef)> {
        let idx = field_id.as_u16().saturating_sub(1) as usize;
        let name = self.field_order.get(idx)?;
        let field = self.fields.get(name)?;
        Some((name, field))
    }

    /// Compile a deterministic physical layout for this schema.
    pub fn layout(&self) -> SchemaLayout {
        let mut fixed_size = 0usize;
        let mut fields = Vec::with_capacity(self.field_order.len());
        let mut variable_fields = Vec::new();

        for (idx, field_name) in self.field_order.iter().enumerate() {
            let field_def = self.fields.get(field_name).expect("field present");
            let field_id = FieldId::new((idx as u16) + 1);
            let (is_fixed, width, is_variable) = match field_def.field_type {
                FieldType::String | FieldType::Reference => (false, None, true),
                FieldType::Integer => (true, Some(8), false),
                FieldType::Float => (true, Some(8), false),
                FieldType::Boolean => (true, Some(1), false),
            };

            if is_fixed {
                fixed_size += width.unwrap_or(0);
            }

            let layout = FieldLayout {
                field_id,
                name: field_name.clone(),
                field_type: field_def.field_type,
                offset: if is_fixed {
                    fixed_size.saturating_sub(width.unwrap_or(0))
                } else {
                    0
                },
                width,
                nullable: field_def.nullable,
                is_fixed,
                is_variable,
                overflow_capable: matches!(
                    field_def.field_type,
                    FieldType::String | FieldType::Reference
                ),
            };

            if is_variable {
                variable_fields.push(layout.clone());
            }
            fields.push(layout);
        }

        SchemaLayout {
            schema_name: self.name.clone(),
            schema_version: self.schema_version,
            fields,
            fixed_size,
            variable_fields,
        }
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
        self.field_order.iter()
    }

    /// Validate schema for correctness.
    pub fn validate(&self) -> crate::Result<()> {
        if self.name.is_empty() {
            return Err(crate::Error::SchemaError {
                reason: "Schema name cannot be empty".to_string(),
            });
        }

        if self.schema_version == 0 {
            return Err(crate::Error::SchemaError {
                reason: "Schema version must be greater than zero".to_string(),
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

/// Stable field identifier used in physical storage layouts.
///
/// Field IDs are assigned by schema order and remain stable across a schema version.
/// They are not derived from field names or hash values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FieldId(pub u16);

impl FieldId {
    pub fn new(id: u16) -> Self {
        Self(id)
    }

    pub fn as_u16(self) -> u16 {
        self.0
    }
}

/// Compiled layout metadata for a field.
///
/// This is intentionally schema-driven and can describe both fixed-width and
/// variable-width physical encoding strategies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldLayout {
    pub field_id: FieldId,
    pub name: String,
    pub field_type: FieldType,
    pub offset: usize,
    pub width: Option<usize>,
    pub nullable: bool,
    pub is_fixed: bool,
    pub is_variable: bool,
    pub overflow_capable: bool,
}

/// Compiled storage layout for a schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaLayout {
    pub schema_name: String,
    pub schema_version: u32,
    pub fields: Vec<FieldLayout>,
    pub fixed_size: usize,
    pub variable_fields: Vec<FieldLayout>,
}

impl SchemaLayout {
    pub fn field_for_id(&self, field_id: FieldId) -> Option<&FieldLayout> {
        self.fields.iter().find(|field| field.field_id == field_id)
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
