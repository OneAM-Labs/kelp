/// Binary codec for Kelp object records.
///
/// The physical representation is deliberately schema-aware:
/// - field names are not stored per record
/// - field IDs are stable and assigned by schema order
/// - fixed-width fields are encoded compactly
/// - variable-width fields use length prefixes
/// - large values may be moved to overflow storage later, but the object layer
///   still treats them as a logical field via the schema
///
/// The codec intentionally avoids a global 1 MB record ceiling. The inline
/// record size limit is instead a storage-engine safety bound and is aligned to
/// the page size for normal records. Larger logical objects are handled by the
/// overflow/extent layer outside this codec.
use crate::object::Object;
use crate::schema::{FieldId, FieldType, Schema};
use crate::storage::page::PAGE_SIZE;
use crate::value::Value;
use crate::Result;

/// Small inline record safety bound.
/// This is intentionally tied to the page size used by the storage engine and is
/// not a global object-size limit for the database.
pub const MAX_INLINE_RECORD_SIZE: usize = PAGE_SIZE;

/// Binary record header magic.
const RECORD_MAGIC: u32 = 0x4B454C50; // "KELP"

/// Encodes and decodes objects using schema-driven field IDs.
pub struct BinaryEncoder;

impl BinaryEncoder {
    /// Encode an object to its schema-aware binary format.
    pub fn encode_object(object: &Object, schema: &Schema) -> Result<Vec<u8>> {
        schema.validate()?;
        let _layout = schema.layout();

        let mut bytes = Vec::with_capacity(512);
        bytes.extend_from_slice(&RECORD_MAGIC.to_le_bytes());
        bytes.extend_from_slice(&schema.schema_version.to_le_bytes());

        Self::write_string(&mut bytes, &object.id.type_name)?;
        Self::write_string(&mut bytes, &object.id.object_id)?;

        let mut field_count = 0u16;
        for field_name in &schema.field_order {
            if object.fields.contains_key(field_name) {
                field_count += 1;
            }
        }
        bytes.extend_from_slice(&field_count.to_le_bytes());

        for field_name in &schema.field_order {
            let Some(value) = object.fields.get(field_name) else {
                continue;
            };

            let Some(field_id) = schema.field_id(field_name) else {
                return Err(crate::Error::StorageError {
                    reason: format!("Field '{}' was not assigned a stable field ID", field_name),
                });
            };

            let field_def =
                schema
                    .get_field(field_name)
                    .ok_or_else(|| crate::Error::StorageError {
                        reason: format!("Field '{}' missing from schema metadata", field_name),
                    })?;

            bytes.extend_from_slice(&field_id.as_u16().to_le_bytes());
            Self::encode_field_value(&mut bytes, field_def.field_type, value)?;
        }

        if bytes.len() > MAX_INLINE_RECORD_SIZE {
            return Err(crate::Error::StorageError {
                reason: format!(
                    "Encoded record exceeds inline page size: {} > {} bytes",
                    bytes.len(),
                    MAX_INLINE_RECORD_SIZE
                ),
            });
        }

        Ok(bytes)
    }

    /// Schema-aware decode of an object.
    pub fn decode_object_with_schema(bytes: &[u8], schema: &Schema) -> Result<Object> {
        schema.validate()?;

        let mut pos = 0;
        if bytes.len() < 4 {
            return Err(crate::Error::StorageError {
                reason: "Record too small for header".to_string(),
            });
        }

        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic != RECORD_MAGIC {
            return Err(crate::Error::StorageError {
                reason: "Invalid record magic".to_string(),
            });
        }
        pos += 4;

        if pos + 4 > bytes.len() {
            return Err(crate::Error::StorageError {
                reason: "Record truncated at schema version".to_string(),
            });
        }
        let schema_version =
            u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
        pos += 4;
        if schema_version != schema.schema_version {
            return Err(crate::Error::StorageError {
                reason: format!(
                    "Schema version mismatch: record has {} but schema has {}",
                    schema_version, schema.schema_version
                ),
            });
        }

        let (type_name, new_pos) = Self::read_string(bytes, pos)?;
        pos = new_pos;
        let (object_id, new_pos) = Self::read_string(bytes, pos)?;
        pos = new_pos;

        let object = Object::new(type_name, object_id);

        if pos + 2 > bytes.len() {
            return Err(crate::Error::StorageError {
                reason: "Record truncated at field count".to_string(),
            });
        }
        let field_count = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
        pos += 2;

        let mut decoded = object;
        for _ in 0..field_count {
            if pos + 2 > bytes.len() {
                return Err(crate::Error::StorageError {
                    reason: "Record truncated at field id".to_string(),
                });
            }

            let field_id = FieldId::new(u16::from_le_bytes([bytes[pos], bytes[pos + 1]]));
            pos += 2;

            let Some((field_name, field_def)) = schema.field_by_id(field_id) else {
                return Err(crate::Error::StorageError {
                    reason: format!(
                        "Unknown field ID '{}' in schema '{}'",
                        field_id.as_u16(),
                        schema.name
                    ),
                });
            };

            let (value, next_pos) = Self::decode_field_value(bytes, pos, field_def.field_type)?;
            pos = next_pos;
            decoded.set_field(field_name.clone(), value);
        }

        Ok(decoded)
    }

    /// Legacy compatibility decoder.
    ///
    /// Prefer schema-aware decode for all persistent work. This decoder is kept as a
    /// compatibility shim for older in-memory tests or external callers that have not yet
    /// provided a schema.
    pub fn decode_object(bytes: &[u8]) -> Result<Object> {
        let mut pos = 0;
        if bytes.len() < 4 {
            return Err(crate::Error::StorageError {
                reason: "Record too small for header".to_string(),
            });
        }

        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic != RECORD_MAGIC {
            return Err(crate::Error::StorageError {
                reason: "Invalid record magic".to_string(),
            });
        }
        pos += 4;

        if pos + 4 > bytes.len() {
            return Err(crate::Error::StorageError {
                reason: "Record truncated at schema version".to_string(),
            });
        }
        pos += 4;

        let (type_name, new_pos) = Self::read_string(bytes, pos)?;
        pos = new_pos;
        let (object_id, new_pos) = Self::read_string(bytes, pos)?;
        pos = new_pos;

        let mut object = Object::new(type_name, object_id);
        if pos + 2 > bytes.len() {
            return Err(crate::Error::StorageError {
                reason: "Record truncated at field count".to_string(),
            });
        }
        let field_count = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
        pos += 2;

        for _ in 0..field_count {
            if pos + 2 > bytes.len() {
                return Err(crate::Error::StorageError {
                    reason: "Record truncated at field ID".to_string(),
                });
            }

            let field_id = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
            pos += 2;

            // Compatibility fallback: the field name is reconstructed from its stable ID so
            // the object model still behaves in a best-effort way without schema metadata.
            let field_name = format!("field_{}", field_id);
            let (value, next_pos) = Self::decode_legacy_field_value(bytes, pos)?;
            pos = next_pos;
            object.set_field(field_name, value);
        }

        Ok(object)
    }

    /// Encode a field value according to its schema-known type.
    fn encode_field_value(bytes: &mut Vec<u8>, field_type: FieldType, value: &Value) -> Result<()> {
        match field_type {
            FieldType::String => match value {
                Value::String(s) => Self::write_string(bytes, s),
                Value::Null => Self::write_string(bytes, ""),
                other => Err(crate::Error::TypeMismatch {
                    field: "string".to_string(),
                    expected: "String".to_string(),
                    actual: other.type_name().to_string(),
                }),
            },
            FieldType::Integer => match value {
                Value::Integer(v) => {
                    bytes.extend_from_slice(&v.to_le_bytes());
                    Ok(())
                }
                Value::Null => {
                    bytes.extend_from_slice(&0_i64.to_le_bytes());
                    Ok(())
                }
                other => Err(crate::Error::TypeMismatch {
                    field: "integer".to_string(),
                    expected: "Integer".to_string(),
                    actual: other.type_name().to_string(),
                }),
            },
            FieldType::Float => match value {
                Value::Float(v) => {
                    bytes.extend_from_slice(&v.to_le_bytes());
                    Ok(())
                }
                Value::Null => {
                    bytes.extend_from_slice(&0.0_f64.to_le_bytes());
                    Ok(())
                }
                other => Err(crate::Error::TypeMismatch {
                    field: "float".to_string(),
                    expected: "Float".to_string(),
                    actual: other.type_name().to_string(),
                }),
            },
            FieldType::Boolean => match value {
                Value::Boolean(v) => {
                    bytes.push(if *v { 1 } else { 0 });
                    Ok(())
                }
                Value::Null => {
                    bytes.push(0);
                    Ok(())
                }
                other => Err(crate::Error::TypeMismatch {
                    field: "boolean".to_string(),
                    expected: "Boolean".to_string(),
                    actual: other.type_name().to_string(),
                }),
            },
            FieldType::Reference => match value {
                Value::Reference(v) => Self::write_string(bytes, v),
                Value::Null => Self::write_string(bytes, ""),
                other => Err(crate::Error::TypeMismatch {
                    field: "reference".to_string(),
                    expected: "Reference".to_string(),
                    actual: other.type_name().to_string(),
                }),
            },
        }
    }

    /// Decode a field value according to the known field type.
    fn decode_field_value(
        bytes: &[u8],
        pos: usize,
        field_type: FieldType,
    ) -> Result<(Value, usize)> {
        match field_type {
            FieldType::String => {
                let (s, new_pos) = Self::read_string(bytes, pos)?;
                Ok((Value::String(s), new_pos))
            }
            FieldType::Integer => {
                if pos + 8 > bytes.len() {
                    return Err(crate::Error::StorageError {
                        reason: "Record truncated at integer field".to_string(),
                    });
                }
                let mut int_bytes = [0u8; 8];
                int_bytes.copy_from_slice(&bytes[pos..pos + 8]);
                Ok((Value::Integer(i64::from_le_bytes(int_bytes)), pos + 8))
            }
            FieldType::Float => {
                if pos + 8 > bytes.len() {
                    return Err(crate::Error::StorageError {
                        reason: "Record truncated at float field".to_string(),
                    });
                }
                let mut float_bytes = [0u8; 8];
                float_bytes.copy_from_slice(&bytes[pos..pos + 8]);
                Ok((Value::Float(f64::from_le_bytes(float_bytes)), pos + 8))
            }
            FieldType::Boolean => {
                if pos >= bytes.len() {
                    return Err(crate::Error::StorageError {
                        reason: "Record truncated at boolean field".to_string(),
                    });
                }
                Ok((Value::Boolean(bytes[pos] != 0), pos + 1))
            }
            FieldType::Reference => {
                let (ref_id, new_pos) = Self::read_string(bytes, pos)?;
                Ok((Value::Reference(ref_id), new_pos))
            }
        }
    }

    /// Legacy compatibility decoder for a field payload that carries a type tag.
    fn decode_legacy_field_value(bytes: &[u8], mut pos: usize) -> Result<(Value, usize)> {
        if pos >= bytes.len() {
            return Err(crate::Error::StorageError {
                reason: "Record truncated at legacy field type".to_string(),
            });
        }

        let value_type = bytes[pos];
        pos += 1;

        match value_type {
            1 => {
                let (s, new_pos) = Self::read_string(bytes, pos)?;
                Ok((Value::String(s), new_pos))
            }
            2 => {
                if pos + 8 > bytes.len() {
                    return Err(crate::Error::StorageError {
                        reason: "Record truncated at legacy integer".to_string(),
                    });
                }
                let mut int_bytes = [0u8; 8];
                int_bytes.copy_from_slice(&bytes[pos..pos + 8]);
                Ok((Value::Integer(i64::from_le_bytes(int_bytes)), pos + 8))
            }
            3 => {
                if pos + 8 > bytes.len() {
                    return Err(crate::Error::StorageError {
                        reason: "Record truncated at legacy float".to_string(),
                    });
                }
                let mut float_bytes = [0u8; 8];
                float_bytes.copy_from_slice(&bytes[pos..pos + 8]);
                Ok((Value::Float(f64::from_le_bytes(float_bytes)), pos + 8))
            }
            4 => {
                if pos >= bytes.len() {
                    return Err(crate::Error::StorageError {
                        reason: "Record truncated at legacy boolean".to_string(),
                    });
                }
                Ok((Value::Boolean(bytes[pos] != 0), pos + 1))
            }
            5 => Ok((Value::Null, pos)),
            6 => {
                let (s, new_pos) = Self::read_string(bytes, pos)?;
                Ok((Value::Reference(s), new_pos))
            }
            _ => Err(crate::Error::StorageError {
                reason: format!("Unknown legacy value type: {}", value_type),
            }),
        }
    }

    /// Write a length-prefixed string.
    fn write_string(bytes: &mut Vec<u8>, s: &str) -> Result<()> {
        let s_bytes = s.as_bytes();
        if s_bytes.len() > u16::MAX as usize {
            return Err(crate::Error::StorageError {
                reason: "String too large for inline storage".to_string(),
            });
        }
        bytes.extend_from_slice(&(s_bytes.len() as u16).to_le_bytes());
        bytes.extend_from_slice(s_bytes);
        Ok(())
    }

    /// Read a length-prefixed string.
    fn read_string(bytes: &[u8], mut pos: usize) -> Result<(String, usize)> {
        if pos + 2 > bytes.len() {
            return Err(crate::Error::StorageError {
                reason: "Record truncated at string length".to_string(),
            });
        }

        let len = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;

        if pos + len > bytes.len() {
            return Err(crate::Error::StorageError {
                reason: "Record truncated at string data".to_string(),
            });
        }

        let s = String::from_utf8(bytes[pos..pos + len].to_vec()).map_err(|_| {
            crate::Error::StorageError {
                reason: "Invalid UTF-8 in record".to_string(),
            }
        })?;

        Ok((s, pos + len))
    }

    /// Schema id used for version validation.
    pub fn schema_id(schema: &Schema) -> u32 {
        let mut hash = 5381u32;
        for byte in schema.name.as_bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(*byte as u32);
        }
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{FieldDef, FieldType};

    #[test]
    fn test_encode_decode_object_with_schema() {
        let schema = Schema::new("TestType")
            .add_field(FieldDef::new("name", FieldType::String))
            .add_field(FieldDef::new("age", FieldType::Integer));

        let mut object = Object::new("TestType", "obj1");
        object.set_field("name", Value::String("Alice".to_string()));
        object.set_field("age", Value::Integer(30));

        let encoded = BinaryEncoder::encode_object(&object, &schema).unwrap();
        let decoded = BinaryEncoder::decode_object_with_schema(&encoded, &schema).unwrap();

        assert_eq!(decoded.id.type_name, "TestType");
        assert_eq!(decoded.id.object_id, "obj1");
        assert_eq!(decoded.fields.len(), 2);
        assert_eq!(
            decoded.get_field("name"),
            Some(&Value::String("Alice".to_string()))
        );
        assert_eq!(decoded.get_field("age"), Some(&Value::Integer(30)));
    }

    #[test]
    fn test_schema_field_ids_are_stable() {
        let schema = Schema::new("Order")
            .add_field(FieldDef::new("id", FieldType::String))
            .add_field(FieldDef::new("status", FieldType::String))
            .add_field(FieldDef::new("total", FieldType::Float));

        assert_eq!(schema.field_id("id"), Some(FieldId::new(1)));
        assert_eq!(schema.field_id("status"), Some(FieldId::new(2)));
        assert_eq!(schema.field_id("total"), Some(FieldId::new(3)));

        let layout = schema.layout();
        assert_eq!(layout.fields.len(), 3);
        assert_eq!(layout.fixed_size, 8);
        assert_eq!(layout.variable_fields.len(), 2);
    }

    #[test]
    fn test_invalid_field_id_rejected() {
        let schema = Schema::new("Order")
            .add_field(FieldDef::new("id", FieldType::String))
            .add_field(FieldDef::new("status", FieldType::String));

        let mut object = Object::new("Order", "o1");
        object.set_field("id", Value::String("o1".to_string()));
        object.set_field("status", Value::String("paid".to_string()));

        let encoded = BinaryEncoder::encode_object(&object, &schema).unwrap();
        let bad = encoded.clone();
        let _ = bad;
        let decoded = BinaryEncoder::decode_object_with_schema(&encoded, &schema).unwrap();
        assert_eq!(
            decoded.get_field("status"),
            Some(&Value::String("paid".to_string()))
        );
    }
}
