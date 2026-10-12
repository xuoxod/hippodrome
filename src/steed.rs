//! # Steed: Racing Upstream Target
//!
//! Represents an individual upstream participant in the Hippodrome tournament.

use serde::{Deserialize, Serialize};

/// An upstream target registered for tournament racing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Steed {
    /// Unique identifier for this upstream (e.g. "quad9", "google", "local-gw").
    pub id: String,
    /// Network address and port (e.g. "1.1.1.1:53" or "http://upstream-1:8080").
    pub endpoint: String,
    /// Priority weight (higher means favored when latencies are equal).
    pub weight: u32,
    /// Descriptive tag or role label (e.g. "primary", "defensive", "fallback").
    pub tag: String,
}

impl Steed {
    /// Construct a new Steed with default weight and tag.
    pub fn new(id: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            endpoint: endpoint.into(),
            weight: 100,
            tag: "standard".to_string(),
        }
    }

    /// Builder method to assign custom weight.
    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    /// Builder method to assign custom tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }
}
