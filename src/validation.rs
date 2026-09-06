use crate::object::Object;
use crate::schema::FieldType;
use crate::schema::Schema;
use crate::value::Value;
use crate::{Error, Result};

/// Validates an object against a schema.
pub struct Validator;

impl Validator {
    /// Validate an object against a schema.
    pub fn validate(obj: &Object, schema: &Schema) -> Result<()> {
        for (field_name, field_def) in &schema.fields {
            let value = obj.get_field(field_name);

            // Check required fields
            if field_def.required && (value.is_none() || value.is_some_and(|v| v.is_null())) {
                return Err(Error::RequiredFieldMissing {
                    field: field_name.to_string(),
                });
            }

            // Check nullable constraint
            if let Some(Value::Null) = value {
                if !field_def.nullable {
                    return Err(Error::FieldNotNullable {
                        field: field_name.to_string(),
                    });
                }
                // If null and nullable, it's valid
                continue;
            }

            // Validate type for non-null values
            if let Some(val) = value {
                Self::validate_field_type(field_name, val, field_def)?;
            }
        }

        Ok(())
    }

    /// Validate a single field value against its field definition.
    fn validate_field_type(
        field_name: &str,
        value: &Value,
        field_def: &crate::schema::FieldDef,
    ) -> Result<()> {
        let type_mismatch = |expected: &str, actual: &str| Error::TypeMismatch {
            field: field_name.to_string(),
            expected: expected.to_string(),
            actual: actual.to_string(),
        };

        match field_def.field_type {
            FieldType::String => {
                if !matches!(value, Value::String(_)) {
                    return Err(type_mismatch("String", value.type_name()));
                }
            }
            FieldType::Integer => {
                if !matches!(value, Value::Integer(_)) {
                    return Err(type_mismatch("Integer", value.type_name()));
                }
            }
            FieldType::Float => {
                // Allow integers to be treated as floats
                if !matches!(value, Value::Float(_) | Value::Integer(_)) {
                    return Err(type_mismatch("Float", value.type_name()));
                }
            }
            FieldType::Boolean => {
                if !matches!(value, Value::Boolean(_)) {
                    return Err(type_mismatch("Boolean", value.type_name()));
                }
            }
            FieldType::Reference => {
                if !matches!(value, Value::Reference(_)) {
                    return Err(type_mismatch("Reference", value.type_name()));
                }
            }
        }

        Ok(())
    }

    /// Check if a value matches a field type.
    pub fn value_matches_type(value: &Value, field_type: FieldType) -> bool {
        match field_type {
            FieldType::String => matches!(value, Value::String(_)),
            FieldType::Integer => matches!(value, Value::Integer(_)),
            FieldType::Float => matches!(value, Value::Float(_) | Value::Integer(_)),
            FieldType::Boolean => matches!(value, Value::Boolean(_)),
            FieldType::Reference => matches!(value, Value::Reference(_)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::FieldDef;

    #[test]
    fn test_validate_required_field() {
        let schema = Schema::new("Customer")
            .add_field(FieldDef::new("id", FieldType::String).required())
            .add_field(FieldDef::new("name", FieldType::String).required());

        let mut obj = Object::new("Customer", "c1");
        obj.set_field("id", Value::String("c1".to_string()));
        // missing name

        assert!(Validator::validate(&obj, &schema).is_err());

        obj.set_field("name", Value::String("Alice".to_string()));
        assert!(Validator::validate(&obj, &schema).is_ok());
    }

    #[test]
    fn test_validate_nullable_field() {
        let schema = Schema::new("Customer")
            .add_field(FieldDef::new("id", FieldType::String).required())
            .add_field(FieldDef::new("email", FieldType::String).nullable());

        let mut obj = Object::new("Customer", "c1");
        obj.set_field("id", Value::String("c1".to_string()));
        obj.set_field("email", Value::Null);

        assert!(Validator::validate(&obj, &schema).is_ok());
    }

    #[test]
    fn test_validate_type_mismatch() {
        let schema = Schema::new("Customer")
            .add_field(FieldDef::new("id", FieldType::String).required())
            .add_field(FieldDef::new("age", FieldType::Integer).required());

        let mut obj = Object::new("Customer", "c1");
        obj.set_field("id", Value::String("c1".to_string()));
        obj.set_field("age", Value::String("not_an_age".to_string()));

        let result = Validator::validate(&obj, &schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Type mismatch"));
    }

    #[test]
    fn test_validate_nullable_constraint() {
        let schema = Schema::new("Customer")
            .add_field(FieldDef::new("id", FieldType::String).required())
            .add_field(FieldDef::new("name", FieldType::String)); // not nullable

        let mut obj = Object::new("Customer", "c1");
        obj.set_field("id", Value::String("c1".to_string()));
        obj.set_field("name", Value::Null);

        assert!(Validator::validate(&obj, &schema).is_err());
    }
}
