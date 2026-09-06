use thiserror::Error;

/// Result type for Kelp operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for Kelp operations.
#[derive(Error, Debug)]
pub enum Error {
    #[error("Object type '{type_name}' not found")]
    TypeNotFound { type_name: String },

    #[error("Object '{object_id}' of type '{type_name}' not found")]
    ObjectNotFound {
        type_name: String,
        object_id: String,
    },

    #[error("Validation failed for field '{field}': {reason}")]
    ValidationError { field: String, reason: String },

    #[error("Field '{field}' is required")]
    RequiredFieldMissing { field: String },

    #[error("Field '{field}' does not allow null")]
    FieldNotNullable { field: String },

    #[error("Type mismatch for field '{field}': expected {expected}, got {actual}")]
    TypeMismatch {
        field: String,
        expected: String,
        actual: String,
    },

    #[error("Reference to '{type_name}/{object_id}' is invalid")]
    InvalidReference {
        type_name: String,
        object_id: String,
    },

    #[error("Circular reference detected: {path}")]
    CircularReference { path: String },

    #[error("Schema error: {reason}")]
    SchemaError { reason: String },

    #[error("Storage error: {reason}")]
    StorageError { reason: String },

    #[error("Query error: {reason}")]
    QueryError { reason: String },

    #[error("Transaction error: {reason}")]
    TransactionError { reason: String },

    #[error("Model error: {reason}")]
    ModelError { reason: String },

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}
