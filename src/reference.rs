use crate::object::ObjectId;
use crate::Result;

/// Represents a reference from one object to another.
///
/// References are first-class in Kelp, allowing objects to maintain relationships
/// without duplicating data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub target: ObjectId,
}

impl Reference {
    /// Create a new reference to a target object.
    pub fn new(type_name: impl Into<String>, object_id: impl Into<String>) -> Self {
        Self {
            target: ObjectId::new(type_name, object_id),
        }
    }

    /// Parse a reference from a string like "Type/id".
    pub fn parse(s: &str) -> Result<Self> {
        let target = ObjectId::parse(s)?;
        Ok(Self { target })
    }

    /// Get the target object ID.
    pub fn target_id(&self) -> &ObjectId {
        &self.target
    }
}

/// Tracks visited objects during reference traversal to detect cycles.
pub struct ReferenceTracker {
    visited: std::collections::HashSet<String>,
    max_depth: usize,
    current_depth: usize,
}

impl ReferenceTracker {
    /// Create a new reference tracker with a maximum depth.
    pub fn new(max_depth: usize) -> Self {
        Self {
            visited: std::collections::HashSet::new(),
            max_depth,
            current_depth: 0,
        }
    }

    /// Mark an object as visited.
    pub fn visit(&mut self, object_id: &str) -> Result<()> {
        if self.current_depth >= self.max_depth {
            return Err(crate::Error::CircularReference {
                path: "Maximum reference depth exceeded".to_string(),
            });
        }

        if self.visited.contains(object_id) {
            return Err(crate::Error::CircularReference {
                path: format!("Circular reference detected at: {}", object_id),
            });
        }

        self.visited.insert(object_id.to_string());
        self.current_depth += 1;
        Ok(())
    }

    /// Create a child tracker for nested traversal.
    pub fn child(&self) -> Self {
        Self {
            visited: self.visited.clone(),
            max_depth: self.max_depth,
            current_depth: self.current_depth + 1,
        }
    }

    /// Pop back from a visit (for depth-first traversal).
    pub fn pop_visit(&mut self) {
        if self.current_depth > 0 {
            self.current_depth -= 1;
        }
    }

    /// Get the current depth.
    pub fn depth(&self) -> usize {
        self.current_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reference_creation() {
        let r = Reference::new("Customer", "c1");
        assert_eq!(r.target.type_name, "Customer");
        assert_eq!(r.target.object_id, "c1");
    }

    #[test]
    fn test_reference_parse() {
        let r = Reference::parse("Order/o1").unwrap();
        assert_eq!(r.target.type_name, "Order");
        assert_eq!(r.target.object_id, "o1");
    }

    #[test]
    fn test_circular_reference_detection() {
        let mut tracker = ReferenceTracker::new(5);

        assert!(tracker.visit("obj1").is_ok());
        assert!(tracker.visit("obj2").is_ok());
        // Revisiting should fail
        assert!(tracker.visit("obj1").is_err());
    }

    #[test]
    fn test_max_depth_limit() {
        let mut tracker = ReferenceTracker::new(2);

        assert!(tracker.visit("obj1").is_ok());
        assert!(tracker.visit("obj2").is_ok());
        // Third visit should fail due to max depth
        assert!(tracker.visit("obj3").is_err());
    }
}
