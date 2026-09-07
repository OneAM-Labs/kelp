//! Kelp: an embedded, object-oriented database.
//!
//! Kelp provides a clean object model with first-class support for:
//! - Strongly-typed objects with schemas
//! - References between objects
//! - Validation and constraints
//! - Models/projections
//! - Simple queries
//! - Local persistence

pub mod database;
pub mod error;
pub mod model;
pub mod object;
pub mod query;
pub mod reference;
pub mod schema;
pub mod shell;
pub mod storage;
pub mod transaction;
pub mod validation;
pub mod value;

pub use database::Database;
pub use error::{Error, Result};
pub use model::Model;
pub use object::Object;
pub use query::{Predicate, QueryResult};
pub use schema::{FieldDef, FieldType, ObjectType, Schema};
pub use shell::{Shell, ShellConfig};
pub use value::Value;
pub mod server;

pub use server::serve as start_server;
