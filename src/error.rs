//! # Hippodrome Errors
//!
//! Strongly typed error states for speculative racing, upstream failures,
//! and circuit breaker trips.

use thiserror::Error;

/// Core error states for Hippodrome racing operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HippodromeError {
    /// No upstreams (steeds) were configured or all are currently disabled.
    #[error("Hippodrome has no registered or active upstreams")]
    NoUpstreamsAvailable,

    /// All steeds tripped their circuit breakers or failed the race.
    #[error("All racing upstreams failed or circuit breakers are open")]
    AllSteedsFailed,

    /// Upstream circuit breaker is currently open (in penalty box).
    #[error("Circuit breaker is open for upstream: {0}")]
    CircuitOpen(String),

    /// Upstream network timeout occurred during the race.
    #[error("Upstream {endpoint} timed out after {timeout_ms}ms")]
    Timeout { endpoint: String, timeout_ms: u64 },

    /// Corrupted or invalid payload received from upstream.
    #[error("Invalid or poisoned payload received from {0}: {1}")]
    InvalidPayload(String, String),

    /// Network I/O transport error occurred.
    #[error("Transport I/O failure on {0}: {1}")]
    Transport(String, String),

    /// Generic error string for custom racing closures.
    #[error("Racing task failed: {0}")]
    Custom(String),
}
