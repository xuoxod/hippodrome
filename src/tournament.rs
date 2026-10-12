//! # Hippodrome Tournament Manager
//!
//! Manages registered steeds, ranks contenders according to microsecond
//! EWMA and sovereign hygiene scoring, and drives speculative tournament races.

use crate::error::HippodromeError;
use crate::hedged::{RaceConfig, RaceOutcome};
use crate::hygiene::{HygieneInfraction, HygieneLedger, HygieneTelemetry};
use crate::scoreboard::{current_epoch_ms, ScoreboardEntry, SteedTelemetry};
use crate::steed::Steed;
use parking_lot::RwLock;
use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

/// Comprehensive telemetry report for an upstream participant.
#[derive(Debug, Clone, Serialize)]
pub struct ContenderTelemetry {
    pub steed: Steed,
    pub scoreboard: SteedTelemetry,
    pub hygiene: HygieneTelemetry,
    pub effective_latency_micros: u64,
}

/// The Tournament coordinator managing all participating upstreams.
#[derive(Clone)]
pub struct HippodromeTournament {
    steeds: Arc<RwLock<Vec<Steed>>>,
    scoreboards: Arc<RwLock<HashMap<String, Arc<ScoreboardEntry>>>>,
    hygiene_ledgers: Arc<RwLock<HashMap<String, Arc<HygieneLedger>>>>,
    config: RaceConfig,
}

impl std::fmt::Debug for HippodromeTournament {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HippodromeTournament")
            .field("config", &self.config)
            .field("steed_count", &self.steeds.read().len())
            .finish()
    }
}

impl HippodromeTournament {
    /// Construct a new empty tournament with specified race configuration.
    pub fn new(config: RaceConfig) -> Self {
        Self {
            steeds: Arc::new(RwLock::new(Vec::new())),
            scoreboards: Arc::new(RwLock::new(HashMap::new())),
            hygiene_ledgers: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Register a new Steed into the tournament.
    pub fn register(&self, steed: Steed) {
        let id = steed.id.clone();
        let mut steeds = self.steeds.write();
        if !steeds.iter().any(|s| s.id == id) {
            steeds.push(steed);
        }

        let mut sc = self.scoreboards.write();
        sc.entry(id.clone())
            .or_insert_with(|| Arc::new(ScoreboardEntry::new(3, 30_000, 20)));

        let mut hy = self.hygiene_ledgers.write();
        hy.entry(id)
            .or_insert_with(|| Arc::new(HygieneLedger::new()));
    }

    /// Retrieve eligible contenders ranked by lowest effective latency.
    pub fn rank_contenders(&self) -> Vec<Steed> {
        let now_ms = current_epoch_ms();
        let steeds = self.steeds.read().clone();
        let sc = self.scoreboards.read();
        let hy = self.hygiene_ledgers.read();

        let mut ranked: Vec<(Steed, u64)> = steeds
            .into_iter()
            .filter_map(|s| {
                let entry = sc.get(&s.id)?;
                if !entry.is_eligible(now_ms) {
                    return None;
                }
                let raw_ewma = entry.ewma_micros();
                let effective = match hy.get(&s.id) {
                    Some(ledger) => ledger.effective_latency(raw_ewma),
                    None => raw_ewma,
                };
                Some((s, effective))
            })
            .collect();

        // Sort ascending by effective latency (lowest micros first)
        ranked.sort_by_key(|(_, eff)| *eff);
        ranked.into_iter().map(|(s, _)| s).collect()
    }

    /// Dispatch a speculative hedged tournament race.
    pub async fn race<T, F, Fut>(&self, runner: F) -> Result<RaceOutcome<T>, HippodromeError>
    where
        F: Fn(Steed) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, HippodromeError>> + Send + 'static,
        T: Send + 'static,
    {
        let contenders = self.rank_contenders();
        if contenders.is_empty() {
            return Err(HippodromeError::NoUpstreamsAvailable);
        }

        let primary = contenders[0].clone();
        let secondary = contenders.get(1).cloned();

        let primary_ewma = self
            .scoreboards
            .read()
            .get(&primary.id)
            .map(|e| e.ewma_micros())
            .unwrap_or(10_000);
        let dynamic_delay = self.config.compute_hedged_delay(primary_ewma);

        let outcome = crate::hedged::execute_hedged_race_with_delay(
            primary.clone(),
            secondary.clone(),
            dynamic_delay,
            self.config.hard_timeout,
            runner,
        )
        .await;

        match outcome {
            Ok(res) => {
                // Update winner's scoreboard
                if let Some(entry) = self.scoreboards.read().get(&res.winner.id) {
                    entry.record_win(res.rtt_micros);
                }
                Ok(res)
            }
            Err(e) => {
                if let Some(entry) = self.scoreboards.read().get(&primary.id) {
                    entry.record_failure();
                }
                if let Some(sec) = secondary {
                    if let Some(entry) = self.scoreboards.read().get(&sec.id) {
                        entry.record_failure();
                    }
                }
                Err(e)
            }
        }
    }

    /// Report a detected hygiene infraction against an upstream.
    pub fn report_infraction(&self, steed_id: &str, infraction: HygieneInfraction) {
        if let Some(ledger) = self.hygiene_ledgers.read().get(steed_id) {
            ledger.record_infraction(infraction);
        }
    }

    /// Export tournament status report across all contenders.
    pub fn report(&self) -> Vec<ContenderTelemetry> {
        let steeds = self.steeds.read().clone();
        let sc = self.scoreboards.read();
        let hy = self.hygiene_ledgers.read();

        steeds
            .into_iter()
            .filter_map(|s| {
                let s_entry = sc.get(&s.id)?;
                let h_entry = hy.get(&s.id)?;
                let raw_ewma = s_entry.ewma_micros();
                let effective = h_entry.effective_latency(raw_ewma);
                Some(ContenderTelemetry {
                    steed: s,
                    scoreboard: s_entry.telemetry(),
                    hygiene: h_entry.telemetry(),
                    effective_latency_micros: effective,
                })
            })
            .collect()
    }

    /// Retrieve the tournament's race configuration.
    pub fn config(&self) -> RaceConfig {
        self.config
    }

    /// Retrieve a snapshot of all registered steeds.
    pub fn steeds(&self) -> Vec<Steed> {
        self.steeds.read().clone()
    }

    /// Retrieve the scoreboard entry for a specific steed.
    pub fn scoreboard(&self, steed_id: &str) -> Option<Arc<ScoreboardEntry>> {
        self.scoreboards.read().get(steed_id).cloned()
    }

    /// Retrieve a snapshot mapping of all scoreboard entries.
    pub fn scoreboards(&self) -> HashMap<String, Arc<ScoreboardEntry>> {
        self.scoreboards.read().clone()
    }
}
