use crate::Result;

/// Represents a database transaction.
///
/// Provides basic atomic operations for V1.
/// Future versions can enhance this with more sophisticated transaction support.
pub struct Transaction {
    in_progress: bool,
}

impl Transaction {
    /// Create a new transaction.
    pub fn new() -> Self {
        Self { in_progress: false }
    }

    /// Begin a transaction.
    pub fn begin(&mut self) -> Result<()> {
        if self.in_progress {
            return Err(crate::Error::TransactionError {
                reason: "Transaction already in progress".to_string(),
            });
        }
        self.in_progress = true;
        Ok(())
    }

    /// Check if a transaction is in progress.
    pub fn is_active(&self) -> bool {
        self.in_progress
    }

    /// Mark transaction as committed.
    pub fn commit(&mut self) -> Result<()> {
        if !self.in_progress {
            return Err(crate::Error::TransactionError {
                reason: "No transaction in progress".to_string(),
            });
        }
        self.in_progress = false;
        Ok(())
    }

    /// Mark transaction as rolled back.
    pub fn rollback(&mut self) -> Result<()> {
        if !self.in_progress {
            return Err(crate::Error::TransactionError {
                reason: "No transaction in progress".to_string(),
            });
        }
        self.in_progress = false;
        Ok(())
    }
}

impl Default for Transaction {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_lifecycle() {
        let mut tx = Transaction::new();
        assert!(!tx.is_active());

        assert!(tx.begin().is_ok());
        assert!(tx.is_active());

        assert!(tx.commit().is_ok());
        assert!(!tx.is_active());
    }

    #[test]
    fn test_nested_transaction_error() {
        let mut tx = Transaction::new();
        assert!(tx.begin().is_ok());
        assert!(tx.begin().is_err()); // Cannot nest
    }
}
