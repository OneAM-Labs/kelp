use serde::{Deserialize, Serialize};
use std::fmt;

/// A value in the Kelp database.
///
/// Represents the atomic data types: String, Integer, Float, Boolean, Null, and Reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    /// String value
    String(String),
    /// Integer value (64-bit signed)
    Integer(i64),
    /// Float value (64-bit)
    Float(f64),
    /// Boolean value
    Boolean(bool),
    /// Null value
    Null,
    /// Reference to another object: "TypeName/object_id"
    Reference(String),
}

impl Value {
    /// Get the type name of this value.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::String(_) => "String",
            Value::Integer(_) => "Integer",
            Value::Float(_) => "Float",
            Value::Boolean(_) => "Boolean",
            Value::Null => "Null",
            Value::Reference(_) => "Reference",
        }
    }

    /// Check if this value is null.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Try to get as a string.
    pub fn as_string(&self) -> Option<&str> {
        if let Value::String(s) = self {
            Some(s)
        } else {
            None
        }
    }

    /// Try to get as an integer.
    pub fn as_integer(&self) -> Option<i64> {
        if let Value::Integer(i) = self {
            Some(*i)
        } else {
            None
        }
    }

    /// Try to get as a float.
    pub fn as_float(&self) -> Option<f64> {
        if let Value::Float(f) = self {
            Some(*f)
        } else {
            None
        }
    }

    /// Try to get as a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        if let Value::Boolean(b) = self {
            Some(*b)
        } else {
            None
        }
    }

    /// Try to get a reference.
    pub fn as_reference(&self) -> Option<&str> {
        if let Value::Reference(r) = self {
            Some(r)
        } else {
            None
        }
    }

    /// Convert this value to a plain JSON value.
    pub fn to_json_value(&self) -> serde_json::Value {
        match self {
            Value::String(s) => serde_json::Value::String(s.clone()),
            Value::Integer(i) => serde_json::Value::Number(serde_json::Number::from(*i)),
            Value::Float(f) => {
                let n = serde_json::Number::from_f64(*f).unwrap_or(serde_json::Number::from(0));
                serde_json::Value::Number(n)
            }
            Value::Boolean(b) => serde_json::Value::Bool(*b),
            Value::Null => serde_json::Value::Null,
            Value::Reference(r) => serde_json::Value::String(r.clone()),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "\"{}\"", s),
            Value::Integer(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Null => write!(f, "null"),
            Value::Reference(r) => write!(f, "{}", r),
        }
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Integer(i)
    }
}

impl From<i32> for Value {
    fn from(i: i32) -> Self {
        Value::Integer(i as i64)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Boolean(b)
    }
}
