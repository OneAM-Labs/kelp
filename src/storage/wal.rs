use crate::Result;
/// Write-Ahead Log (WAL) for crash recovery.
///
/// The WAL ensures durability:
/// 1. All mutations are written to the WAL first
/// 2. WAL is flushed/synced
/// 3. Then the transaction is acknowledged
/// 4. Data pages can be written asynchronously
/// 5. On restart, uncommitted log records are rolled back
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

/// WAL record types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WalRecordType {
    /// Begin transaction
    Begin = 1,
    /// Insert/create record
    Insert = 2,
    /// Update record
    Update = 3,
    /// Delete record
    Delete = 4,
    /// Index update
    IndexUpdate = 5,
    /// Commit transaction
    Commit = 6,
    /// Rollback transaction
    Rollback = 7,
}

impl WalRecordType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(WalRecordType::Begin),
            2 => Some(WalRecordType::Insert),
            3 => Some(WalRecordType::Update),
            4 => Some(WalRecordType::Delete),
            5 => Some(WalRecordType::IndexUpdate),
            6 => Some(WalRecordType::Commit),
            7 => Some(WalRecordType::Rollback),
            _ => None,
        }
    }

    fn as_u8(self) -> u8 {
        self as u8
    }
}

/// A single WAL entry.
#[derive(Debug, Clone)]
pub struct WalRecord {
    /// Transaction ID
    pub txn_id: u64,
    /// Log sequence number
    pub lsn: u64,
    /// Record type
    pub record_type: WalRecordType,
    /// Payload (varies by type)
    pub payload: Vec<u8>,
}

impl WalRecord {
    pub fn new(txn_id: u64, lsn: u64, record_type: WalRecordType) -> Self {
        Self {
            txn_id,
            lsn,
            record_type,
            payload: Vec::new(),
        }
    }

    pub fn with_payload(mut self, payload: Vec<u8>) -> Self {
        self.payload = payload;
        self
    }

    /// Serialize to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(256);

        bytes.extend_from_slice(&self.txn_id.to_le_bytes());
        bytes.extend_from_slice(&self.lsn.to_le_bytes());
        bytes.push(self.record_type.as_u8());
        bytes.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.payload);

        // Checksum (simple XOR for now)
        let checksum = bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        bytes.push(checksum);

        bytes
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 21 {
            return Err(crate::Error::StorageError {
                reason: "WAL record too small".to_string(),
            });
        }

        let txn_id = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        let lsn = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);
        let record_type =
            WalRecordType::from_u8(bytes[16]).ok_or_else(|| crate::Error::StorageError {
                reason: format!("Invalid WAL record type: {}", bytes[16]),
            })?;

        let payload_len = u32::from_le_bytes([bytes[17], bytes[18], bytes[19], bytes[20]]) as usize;

        if 21 + payload_len + 1 > bytes.len() {
            return Err(crate::Error::StorageError {
                reason: "WAL record truncated".to_string(),
            });
        }

        let payload = bytes[21..21 + payload_len].to_vec();
        let checksum_stored = bytes[21 + payload_len];

        // Verify checksum
        let checksum_calc = bytes[0..21 + payload_len]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_add(b));
        if checksum_calc != checksum_stored {
            return Err(crate::Error::StorageError {
                reason: "WAL record checksum mismatch".to_string(),
            });
        }

        Ok(Self {
            txn_id,
            lsn,
            record_type,
            payload,
        })
    }
}

/// Write-Ahead Log manager.
pub struct Wal {
    log_file: Option<File>,
    file_path: PathBuf,
    next_lsn: u64,
    next_txn_id: u64,
    buffer: Vec<u8>,
}

impl Wal {
    /// Create or open WAL at the given path.
    pub fn new(file_path: impl Into<PathBuf>) -> Result<Self> {
        let file_path = file_path.into();

        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to open WAL: {}", e),
            })?;

        Ok(Self {
            log_file: Some(log_file),
            file_path,
            next_lsn: 1,
            next_txn_id: 1,
            buffer: Vec::with_capacity(4096),
        })
    }

    /// Begin a new transaction.
    pub fn begin_transaction(&mut self) -> Result<u64> {
        let txn_id = self.next_txn_id;
        self.next_txn_id += 1;

        let record = WalRecord::new(txn_id, self.next_lsn, WalRecordType::Begin);
        self.next_lsn += 1;

        self.append_record(&record)?;
        Ok(txn_id)
    }

    /// Append an insert record to the log.
    pub fn log_insert(&mut self, txn_id: u64, payload: Vec<u8>) -> Result<()> {
        let record =
            WalRecord::new(txn_id, self.next_lsn, WalRecordType::Insert).with_payload(payload);
        self.next_lsn += 1;
        self.append_record(&record)
    }

    /// Append an update record to the log.
    pub fn log_update(&mut self, txn_id: u64, payload: Vec<u8>) -> Result<()> {
        let record =
            WalRecord::new(txn_id, self.next_lsn, WalRecordType::Update).with_payload(payload);
        self.next_lsn += 1;
        self.append_record(&record)
    }

    /// Append a delete record to the log.
    pub fn log_delete(&mut self, txn_id: u64, payload: Vec<u8>) -> Result<()> {
        let record =
            WalRecord::new(txn_id, self.next_lsn, WalRecordType::Delete).with_payload(payload);
        self.next_lsn += 1;
        self.append_record(&record)
    }

    /// Commit a transaction.
    pub fn commit(&mut self, txn_id: u64) -> Result<()> {
        let record = WalRecord::new(txn_id, self.next_lsn, WalRecordType::Commit);
        self.next_lsn += 1;
        self.append_record(&record)?;
        self.sync()
    }

    /// Rollback a transaction.
    pub fn rollback(&mut self, txn_id: u64) -> Result<()> {
        let record = WalRecord::new(txn_id, self.next_lsn, WalRecordType::Rollback);
        self.next_lsn += 1;
        self.append_record(&record)?;
        self.sync()
    }

    /// Append a record to the log.
    fn append_record(&mut self, record: &WalRecord) -> Result<()> {
        self.buffer.extend_from_slice(&record.to_bytes());
        Ok(())
    }

    /// Flush buffered records to storage.
    pub fn flush(&mut self) -> Result<()> {
        if let Some(ref mut file) = self.log_file {
            file.write_all(&self.buffer)
                .map_err(|e| crate::Error::StorageError {
                    reason: format!("Failed to write WAL: {}", e),
                })?;
            self.buffer.clear();
        }
        Ok(())
    }

    /// Sync WAL to durable storage.
    pub fn sync(&mut self) -> Result<()> {
        self.flush()?;
        if let Some(ref mut file) = self.log_file {
            file.sync_all().map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to sync WAL: {}", e),
            })?;
        }
        Ok(())
    }

    /// Get the current LSN.
    pub fn current_lsn(&self) -> u64 {
        self.next_lsn
    }

    /// Read all records from the WAL.
    pub fn read_all_records(&self) -> Result<Vec<WalRecord>> {
        let mut file = File::open(&self.file_path).map_err(|e| crate::Error::StorageError {
            reason: format!("Failed to read WAL: {}", e),
        })?;

        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|e| crate::Error::StorageError {
                reason: format!("Failed to read WAL: {}", e),
            })?;

        let mut records = Vec::new();
        let mut pos = 0;

        while pos < buffer.len() {
            // Try to find a complete record
            if pos + 21 > buffer.len() {
                break;
            }

            let payload_len = u32::from_le_bytes([
                buffer[pos + 17],
                buffer[pos + 18],
                buffer[pos + 19],
                buffer[pos + 20],
            ]) as usize;

            if pos + 21 + payload_len + 1 > buffer.len() {
                break;
            }

            match WalRecord::from_bytes(&buffer[pos..pos + 21 + payload_len + 1]) {
                Ok(record) => {
                    records.push(record);
                    pos += 21 + payload_len + 1;
                }
                Err(_) => {
                    break;
                }
            }
        }

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_record_roundtrip() {
        let record =
            WalRecord::new(1, 1, WalRecordType::Begin).with_payload(b"test_payload".to_vec());

        let bytes = record.to_bytes();
        let restored = WalRecord::from_bytes(&bytes).unwrap();

        assert_eq!(restored.txn_id, 1);
        assert_eq!(restored.lsn, 1);
        assert_eq!(restored.record_type, WalRecordType::Begin);
        assert_eq!(restored.payload, b"test_payload");
    }

    // Test requires tempfile dependency; skipped for now
    // Can be enabled when tempfile is added to Cargo.toml
    // #[test]
    // fn test_wal_manager() { ... }
}
