//! # Scoreboard & Circuit Breaker
//!
//! High-performance lock-free latency tracking with Exponentially-Weighted
//! Moving Average (EWMA) and autonomous circuit breaker isolation.

use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::Serialize;

/// Circuit breaker operational state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CircuitState {
    /// Circuit is healthy; steed participates normally.
    Closed,
    /// Circuit is tripped; steed is exiled in the penalty box.
    Open,
    /// Cooldown expired; steed receives a single canary probe to test recovery.
    HalfOpen,
}

/// Statistics and real-time performance telemetry for a single Steed.
#[derive(Debug, Clone, Serialize)]
pub struct SteedTelemetry {
    pub ewma_micros: u64,
    pub total_races: u64,
    pub total_wins: u64,
    pub consecutive_failures: u32,
    pub circuit_state: CircuitState,
    pub last_seen_ms: u64,
}

/// Scoreboard entry tracking latency, wins, and circuit state for a Steed.
pub struct ScoreboardEntry {
    /// EWMA of round-trip time in microseconds (stored as AtomicU64).
    ewma_micros: AtomicU64,
    /// Total races participated in.
    total_races: AtomicU64,
    /// Total races won (crossed the finish line first with valid response).
    total_wins: AtomicU64,
    /// Consecutive failures count.
    consecutive_failures: AtomicU32,
    /// Epoch millisecond when circuit was opened (0 if healthy).
    circuit_opened_at_ms: AtomicU64,
    /// Epoch millisecond of the most recent query completion.
    last_seen_ms: AtomicU64,
    /// Maximum consecutive failures before circuit breaker trips open.
    failure_threshold: u32,
    /// Cooldown duration in milliseconds before moving Open -> HalfOpen.
    cooldown_ms: u64,
    /// EWMA smoothing alpha coefficient (scaled by 100, e.g. 20 = 0.2).
    alpha_percent: u64,
}

impl ScoreboardEntry {
    /// Create a new scoreboard entry with defaults.
    pub fn new(failure_threshold: u32, cooldown_ms: u64, alpha_percent: u64) -> Self {
        Self {
            ewma_micros: AtomicU64::new(10_000), // Initialize with optimistic 10ms baseline
            total_races: AtomicU64::new(0),
            total_wins: AtomicU64::new(0),
            consecutive_failures: AtomicU32::new(0),
            circuit_opened_at_ms: AtomicU64::new(0),
            last_seen_ms: AtomicU64::new(0),
            failure_threshold,
            cooldown_ms,
            alpha_percent,
        }
    }

    /// Record a victorious race outcome with measured RTT in microseconds.
    pub fn record_win(&self, rtt_micros: u64) {
        self.total_wins.fetch_add(1, Ordering::Relaxed);
        self.record_success(rtt_micros);
    }

    /// Record a successful query response and update the EWMA scoreboard.
    pub fn record_success(&self, rtt_micros: u64) {
        self.total_races.fetch_add(1, Ordering::Relaxed);
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.circuit_opened_at_ms.store(0, Ordering::Release);

        let now_ms = current_epoch_ms();
        self.last_seen_ms.store(now_ms, Ordering::Relaxed);

        // Update EWMA: new_ewma = (alpha * rtt) + ((100 - alpha) * old_ewma) / 100
        let alpha = self.alpha_percent;
        let mut old = self.ewma_micros.load(Ordering::Relaxed);
        loop {
            let next = (alpha * rtt_micros + (100 - alpha) * old) / 100;
            match self.ewma_micros.compare_exchange_weak(
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

    /// Record a query failure or timeout and evaluate circuit breaker tripping.
    pub fn record_failure(&self) {
        self.total_races.fetch_add(1, Ordering::Relaxed);
        let fails = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;

        // Apply a latency penalty to the EWMA so failing nodes rank lower
        let mut old = self.ewma_micros.load(Ordering::Relaxed);
        loop {
            let next = old.saturating_add(50_000); // +50ms penalty
            match self.ewma_micros.compare_exchange_weak(
                old,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => old = actual,
            }
        }

        if fails >= self.failure_threshold {
            let now_ms = current_epoch_ms();
            self.circuit_opened_at_ms.store(now_ms, Ordering::Release);
        }
    }

    /// Determine if this steed is eligible to race right now.
    pub fn is_eligible(&self, now_ms: u64) -> bool {
        match self.circuit_state(now_ms) {
            CircuitState::Closed | CircuitState::HalfOpen => true,
            CircuitState::Open => false,
        }
    }

    /// Current circuit state given the current wall-clock timestamp.
    pub fn circuit_state(&self, now_ms: u64) -> CircuitState {
        let opened_at = self.circuit_opened_at_ms.load(Ordering::Acquire);
        if opened_at == 0 {
            return CircuitState::Closed;
        }

        if now_ms.saturating_sub(opened_at) >= self.cooldown_ms {
            CircuitState::HalfOpen
        } else {
            CircuitState::Open
        }
    }

    /// Current EWMA round-trip latency in microseconds.
    pub fn ewma_micros(&self) -> u64 {
        self.ewma_micros.load(Ordering::Relaxed)
    }

    /// Export real-time telemetry snapshot.
    pub fn telemetry(&self) -> SteedTelemetry {
        let now_ms = current_epoch_ms();
        SteedTelemetry {
            ewma_micros: self.ewma_micros.load(Ordering::Relaxed),
            total_races: self.total_races.load(Ordering::Relaxed),
            total_wins: self.total_wins.load(Ordering::Relaxed),
            consecutive_failures: self.consecutive_failures.load(Ordering::Relaxed),
            circuit_state: self.circuit_state(now_ms),
            last_seen_ms: self.last_seen_ms.load(Ordering::Relaxed),
        }
    }
}

/// Helper function to retrieve current wall-clock epoch in milliseconds.
pub fn current_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
