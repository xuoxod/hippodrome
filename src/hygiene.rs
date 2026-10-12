//! # Sovereign Hygiene & Trust Scoreboard
//!
//! Evaluates upstream veracity, integrity, and hygiene compliance. Upstreams
//! returning poisoned, rebinding, or cloaked records receive trust penalties.

use serde::Serialize;
use std::sync::atomic::{AtomicI32, Ordering};

/// Types of hygiene infractions detected on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HygieneInfraction {
    /// Attempted to return private RFC 1918 / Loopback address for public name.
    DnsRebindingAttempt,
    /// Returned known third-party corporate telemetry tracker or CNAME cloaking.
    TelemetryCloaking,
    /// Wire framing violation or corrupted unparseable packet.
    CorruptedWirePayload,
}

/// Trust and hygiene state for an individual Steed.
pub struct HygieneLedger {
    /// Sovereign Trust Score (0 to 100, initialized to 100).
    trust_score: AtomicI32,
    /// Total rebinding attempts detected.
    rebinding_violations: AtomicI32,
    /// Total cloaking / tracker redirects detected.
    cloaking_violations: AtomicI32,
}

impl Default for HygieneLedger {
    fn default() -> Self {
        Self {
            trust_score: AtomicI32::new(100),
            rebinding_violations: AtomicI32::new(0),
            cloaking_violations: AtomicI32::new(0),
        }
    }
}

impl HygieneLedger {
    /// Construct a pristine hygiene ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a hygiene infraction and apply algorithmic trust deduction.
    pub fn record_infraction(&self, infraction: HygieneInfraction) {
        let penalty = match infraction {
            HygieneInfraction::DnsRebindingAttempt => {
                self.rebinding_violations.fetch_add(1, Ordering::Relaxed);
                50 // Critical infraction
            }
            HygieneInfraction::TelemetryCloaking => {
                self.cloaking_violations.fetch_add(1, Ordering::Relaxed);
                20 // Tracking violation
            }
            HygieneInfraction::CorruptedWirePayload => 10,
        };

        // Decrement trust score with floor at 0
        let mut old = self.trust_score.load(Ordering::Relaxed);
        loop {
            let next = (old - penalty).max(0);
            match self.trust_score.compare_exchange_weak(
                old,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => old = actual,
            }
        }
    }

    /// Compute an effective latency multiplier based on trust.
    /// Perfectly honest steeds (100) have multiplier 1.0.
    /// Untrusted steeds (0) have multiplier 3.0 (penalizing rank in race).
    pub fn effective_latency(&self, raw_ewma_micros: u64) -> u64 {
        let trust = self.trust_score.load(Ordering::Relaxed).clamp(0, 100) as u64;
        let penalty_ratio = 100u64.saturating_sub(trust);
        // Add up to 200% virtual latency penalty for untrusted upstreams
        raw_ewma_micros + (raw_ewma_micros * penalty_ratio * 2 / 100)
    }

    /// Current trust score (0 to 100).
    pub fn trust_score(&self) -> i32 {
        self.trust_score.load(Ordering::Relaxed)
    }

    /// Telemetry snapshot of hygiene statistics.
    pub fn telemetry(&self) -> HygieneTelemetry {
        HygieneTelemetry {
            trust_score: self.trust_score.load(Ordering::Relaxed),
            rebinding_violations: self.rebinding_violations.load(Ordering::Relaxed),
            cloaking_violations: self.cloaking_violations.load(Ordering::Relaxed),
        }
    }
}

/// Exportable telemetry of hygiene violations.
#[derive(Debug, Clone, Serialize)]
pub struct HygieneTelemetry {
    pub trust_score: i32,
    pub rebinding_violations: i32,
    pub cloaking_violations: i32,
}
