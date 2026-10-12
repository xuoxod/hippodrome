//! # 🐎 Hippodrome (`hippodrome`)
//!
//! Sovereign Speculative Hedged Racing & Upstream Tournament Engine in pure Rust.
//! Replaces naive round-robin with microsecond fastest-horse speculative concurrency.
//!
//! ```text
//!                          🐎 THE HIPPODROME
//!                   ┌─────────────────────────────┐
//!                   │   Client Query / Request    │
//!                   └──────────────┬──────────────┘
//!                                  │
//!                 ┌────────────────┴────────────────┐
//!                 ▼                                 ▼
//!      ┌──────────────────────┐          ┌──────────────────────┐
//!      │  Rank #1 Steed (0ms) │          │ Rank #2 Steed (10ms) │
//!      │  [Primary Dispatch]  │          │ [Speculative Hedged] │
//!      └──────────┬───────────┘          └──────────┬───────────┘
//!                 │                                 │
//!                 └────────────────┬────────────────┘
//!                                  ▼
//!                     🏆 FIRST TO CROSS FINISH LINE
//!                  (Loser canceled; Zero tail latency)
//! ```

pub mod dns_adapter;
pub mod error;
pub mod health;
pub mod hedged;
pub mod http_adapter;
pub mod hygiene;
pub mod scoreboard;
pub mod steed;
pub mod tournament;

pub use dns_adapter::HippodromeDnsRacer;
pub use error::HippodromeError;
pub use health::{HealthProbeGuard, ProbeConfig};
pub use hedged::{execute_hedged_race, execute_hedged_race_with_delay, RaceConfig, RaceOutcome};
pub use http_adapter::{HippodromeHttpRacer, HttpMethodPolicy, HttpRaceResult};
pub use hygiene::{HygieneInfraction, HygieneLedger, HygieneTelemetry};
pub use scoreboard::{CircuitState, ScoreboardEntry, SteedTelemetry};
pub use steed::Steed;
pub use tournament::{ContenderTelemetry, HippodromeTournament};
