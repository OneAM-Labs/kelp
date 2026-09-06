use crate::object::Object;
use crate::value::Value;
use crate::Result;

/// Represents a query predicate.
///
/// Predicates are used to filter objects based on field values.
/// Multiple predicates can be combined with AND logic using `Predicate::and()`.
///
/// # Supported Operations
/// - Equals: `field == value`
/// - NotEquals: `field != value`
/// - GreaterThan: `field > value`
/// - GreaterOrEqual: `field >= value`
/// - LessThan: `field < value`
/// - LessOrEqual: `field <= value`
/// - And: combine multiple predicates (all must match)
///
/// # Examples
///
/// ```ignore
/// use kelp_db::{Predicate, Value};
///
/// // Simple equality check
/// let p1 = Predicate::equals("status", Value::String("active".to_string()));
///
/// // Comparison
/// let p2 = Predicate::gt("age", Value::Integer(18));
///
/// // Combine multiple predicates
/// let combined = Predicate::and(vec![p1, p2]);
/// let results = db.query("User", &combined)?;
/// ```
#[derive(Debug, Clone)]
pub enum Predicate {
    /// field == value
    Equals { field: String, value: Value },
    /// field != value
    NotEquals { field: String, value: Value },
    /// field > value
    GreaterThan { field: String, value: Value },
    /// field >= value
    GreaterOrEqual { field: String, value: Value },
    /// field < value
    LessThan { field: String, value: Value },
    /// field <= value
    LessOrEqual { field: String, value: Value },
    /// All predicates must be true (AND)
    And(Vec<Predicate>),
}

impl Predicate {
    /// Create an equals predicate.
    pub fn equals(field: impl Into<String>, value: Value) -> Self {
        Self::Equals {
            field: field.into(),
            value,
        }
    }

    /// Create a not-equals predicate.
    pub fn not_equals(field: impl Into<String>, value: Value) -> Self {
        Self::NotEquals {
            field: field.into(),
            value,
        }
    }

    /// Create a greater-than predicate.
    pub fn gt(field: impl Into<String>, value: Value) -> Self {
        Self::GreaterThan {
            field: field.into(),
            value,
        }
    }

    /// Create a less-than predicate.
    pub fn lt(field: impl Into<String>, value: Value) -> Self {
        Self::LessThan {
            field: field.into(),
            value,
        }
    }

    /// Combine predicates with AND.
    pub fn and(predicates: Vec<Predicate>) -> Self {
        Self::And(predicates)
    }

    /// Execute this predicate against an object.
    pub fn matches(&self, obj: &Object) -> Result<bool> {
        match self {
            Predicate::Equals { field, value } => {
                Ok(obj.get_field(field).is_some_and(|v| v == value))
            }
            Predicate::NotEquals { field, value } => Ok(obj.get_field(field) != Some(value)),
            Predicate::GreaterThan { field, value } => {
                Ok(self.compare_fields(obj, field, value)? > 0)
            }
            Predicate::GreaterOrEqual { field, value } => {
                Ok(self.compare_fields(obj, field, value)? >= 0)
            }
            Predicate::LessThan { field, value } => Ok(self.compare_fields(obj, field, value)? < 0),
            Predicate::LessOrEqual { field, value } => {
                Ok(self.compare_fields(obj, field, value)? <= 0)
            }
            Predicate::And(predicates) => {
                for pred in predicates {
                    if !pred.matches(obj)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
        }
    }

    fn compare_fields(&self, obj: &Object, field: &str, value: &Value) -> Result<i64> {
        match (obj.get_field(field), value) {
            (Some(Value::Integer(a)), Value::Integer(b)) => Ok(a.cmp(b) as i64),
            (Some(Value::Float(a)), Value::Float(b)) => Ok(if a > b {
                1
            } else if a < b {
                -1
            } else {
                0
            }),
            (Some(Value::Integer(a)), Value::Float(b)) => {
                let a_float = *a as f64;
                Ok(if a_float > *b {
                    1
                } else if a_float < *b {
                    -1
                } else {
                    0
                })
            }
            (Some(Value::Float(a)), Value::Integer(b)) => {
                let b_float = *b as f64;
                Ok(if a > &b_float {
                    1
                } else if a < &b_float {
                    -1
                } else {
                    0
                })
            }
            (Some(Value::String(a)), Value::String(b)) => Ok(a.cmp(b) as i64),
            _ => Err(crate::Error::QueryError {
                reason: format!("Cannot compare field '{}': type mismatch", field),
            }),
        }
    }
}

/// Represents a query result set.
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub objects: Vec<Object>,
}

impl QueryResult {
    /// Create an empty query result.
    pub fn empty() -> Self {
        Self {
            objects: Vec::new(),
        }
    }

    /// Create a query result with objects.
    pub fn new(objects: Vec<Object>) -> Self {
        Self { objects }
    }

    /// Get the number of results.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Get a result by index.
    pub fn get(&self, index: usize) -> Option<&Object> {
        self.objects.get(index)
    }

    /// Iterate over results.
    pub fn iter(&self) -> impl Iterator<Item = &Object> {
        self.objects.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equals_predicate() {
        let mut obj = Object::new("Customer", "c1");
        obj.set_field("status", Value::String("active".to_string()));

        let pred = Predicate::equals("status", Value::String("active".to_string()));
        assert!(pred.matches(&obj).unwrap());

        let pred2 = Predicate::equals("status", Value::String("inactive".to_string()));
        assert!(!pred2.matches(&obj).unwrap());
    }

    #[test]
    fn test_numeric_comparison() {
        let mut obj = Object::new("Order", "o1");
        obj.set_field("total", Value::Float(500.0));

        let pred = Predicate::gt("total", Value::Float(100.0));
        assert!(pred.matches(&obj).unwrap());

        let pred2 = Predicate::lt("total", Value::Float(1000.0));
        assert!(pred2.matches(&obj).unwrap());

        let pred3 = Predicate::gt("total", Value::Float(600.0));
        assert!(!pred3.matches(&obj).unwrap());
    }

    #[test]
    fn test_and_predicate() {
        let mut obj = Object::new("Order", "o1");
        obj.set_field("status", Value::String("pending".to_string()));
        obj.set_field("total", Value::Float(500.0));

        let pred = Predicate::and(vec![
            Predicate::equals("status", Value::String("pending".to_string())),
            Predicate::gt("total", Value::Float(100.0)),
        ]);

        assert!(pred.matches(&obj).unwrap());
    }
}
