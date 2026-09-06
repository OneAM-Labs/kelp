use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::object::Object;
use crate::value::Value;
use crate::Result;

/// Represents a projection/view of objects.
///
/// A model defines which fields to include in a result, potentially
/// including nested fields from references.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// Name of the model
    pub name: String,
    /// The object type this model projects from
    pub base_type: String,
    /// Field projections: field_name -> projection spec
    pub fields: HashMap<String, FieldProjection>,
}

/// Defines how to project a single field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FieldProjection {
    /// Include a direct field
    Direct { field: String },
    /// Include a nested field from a reference (e.g., order.customer.name)
    Nested {
        reference_field: String,
        nested_field: String,
    },
}

impl Model {
    /// Create a new model.
    pub fn new(name: impl Into<String>, base_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            base_type: base_type.into(),
            fields: HashMap::new(),
        }
    }

    /// Add a direct field projection.
    pub fn add_field(
        mut self,
        projection_name: impl Into<String>,
        field: impl Into<String>,
    ) -> Self {
        self.fields.insert(
            projection_name.into(),
            FieldProjection::Direct {
                field: field.into(),
            },
        );
        self
    }

    /// Add a nested field projection (e.g., for order.customer.name).
    pub fn add_nested_field(
        mut self,
        projection_name: impl Into<String>,
        reference_field: impl Into<String>,
        nested_field: impl Into<String>,
    ) -> Self {
        self.fields.insert(
            projection_name.into(),
            FieldProjection::Nested {
                reference_field: reference_field.into(),
                nested_field: nested_field.into(),
            },
        );
        self
    }

    /// Validate the model definition.
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            return Err(crate::Error::ModelError {
                reason: "Model name cannot be empty".to_string(),
            });
        }

        if self.base_type.is_empty() {
            return Err(crate::Error::ModelError {
                reason: "Base type cannot be empty".to_string(),
            });
        }

        if self.fields.is_empty() {
            return Err(crate::Error::ModelError {
                reason: "Model must have at least one field".to_string(),
            });
        }

        Ok(())
    }
}

/// A projected view of an object data.
pub type ProjectedData = serde_json::Value;

/// Projects an object according to a model.
///
/// This is a simple read-time projection - doesn't materialize views.
pub struct ModelProjector;

impl ModelProjector {
    /// Project an object using a model.
    ///
    /// For nested projections, the referenced object must be looked up from the database.
    /// This function returns the projected data structure, and caller must resolve references.
    pub fn project(
        obj: &Object,
        model: &Model,
        resolve_reference: impl Fn(&str) -> Result<Option<Object>>,
    ) -> Result<serde_json::Value> {
        let mut result = serde_json::json!({});

        for (proj_name, proj) in &model.fields {
            match proj {
                FieldProjection::Direct { field } => {
                    if let Some(value) = obj.get_field(field) {
                        result[proj_name] = value.to_json_value();
                    } else {
                        result[proj_name] = serde_json::Value::Null;
                    }
                }
                FieldProjection::Nested {
                    reference_field,
                    nested_field,
                } => {
                    // Get the reference
                    if let Some(Value::Reference(ref_str)) = obj.get_field(reference_field) {
                        // Resolve the reference
                        if let Ok(Some(ref_obj)) = resolve_reference(ref_str) {
                            // Get the nested field from the resolved object
                            if let Some(value) = ref_obj.get_field(nested_field) {
                                result[proj_name] = value.to_json_value();
                            } else {
                                result[proj_name] = serde_json::Value::Null;
                            }
                        } else {
                            result[proj_name] = serde_json::Value::Null;
                        }
                    } else {
                        result[proj_name] = serde_json::Value::Null;
                    }
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_creation() {
        let model = Model::new("OrderCard", "Order")
            .add_field("id", "id")
            .add_field("total", "total")
            .add_field("status", "status");

        assert_eq!(model.name, "OrderCard");
        assert_eq!(model.base_type, "Order");
        assert_eq!(model.fields.len(), 3);
    }

    #[test]
    fn test_model_validation() {
        let valid_model = Model::new("OrderCard", "Order").add_field("id", "id");
        assert!(valid_model.validate().is_ok());

        let invalid_model = Model::new("", "Order");
        assert!(invalid_model.validate().is_err());
    }

    #[test]
    fn test_direct_projection() {
        let mut obj = Object::new("Order", "o1");
        obj.set_field("id", Value::String("o1".to_string()));
        obj.set_field("total", Value::Float(500.0));

        let model = Model::new("OrderCard", "Order")
            .add_field("id", "id")
            .add_field("total", "total");

        let projected = ModelProjector::project(&obj, &model, |_| Ok(None)).unwrap();
        assert_eq!(projected["id"], "o1");
        assert_eq!(projected["total"], 500.0);
    }
}
