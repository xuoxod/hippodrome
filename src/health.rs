//! # Proactive Upstream Health Sentinel
//!
//! Background daemon actively monitoring tripped and cooling upstreams.
//! Restores recovering steeds back into the active racetrack via lightweight
//! non-blocking TCP connect probes without penalizing real client traffic.

use crate::scoreboard::CircuitState;
use crate::tournament::HippodromeTournament;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tracing::{debug, info};

/// Configuration governing background health probing.
#[derive(Debug, Clone, Copy)]
pub struct ProbeConfig {
    /// Interval between proactive probing sweeps (default: 5s).
    pub interval: Duration,
    /// Hard connect timeout for probe packets (default: 500ms).
    pub connect_timeout: Duration,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            connect_timeout: Duration::from_millis(500),
        }
    }
}

/// RAII Guard managing the lifecycle of the background health prober task.
pub struct HealthProbeGuard {
    shutdown_tx: watch::Sender<bool>,
}

impl HealthProbeGuard {
    /// Launch the proactive health sentinel in a background Tokio task.
    pub fn start(tournament: HippodromeTournament, config: ProbeConfig) -> Self {
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.interval);
            // Skip immediate tick to allow initial runs to settle
            interval.tick().await;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        Self::probe_sweep(&tournament, config.connect_timeout).await;
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("🛑 [HIPPODROME-HEALTH] Probing loop terminated by shutdown signal.");
                            break;
                        }
                    }
                }
            }
        });

        Self { shutdown_tx }
    }

    /// Explicitly terminate the background probe loop.
    pub fn stop(self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Single sweep testing all non-closed (tripped or cooling) contenders.
    pub async fn probe_sweep(tournament: &HippodromeTournament, timeout: Duration) {
        let steeds = tournament.steeds();
        let now_ms = crate::scoreboard::current_epoch_ms();

        for steed in steeds {
            if let Some(entry) = tournament.scoreboard(&steed.id) {
                let state = entry.circuit_state(now_ms);
                // Only probe steeds currently suffering degraded or tripped states
                if state == CircuitState::Open || state == CircuitState::HalfOpen {
                    let endpoint = steed.endpoint.clone();
                    let start = Instant::now();

                    let probe_res =
                        tokio::time::timeout(timeout, TcpStream::connect(&endpoint)).await;

                    match probe_res {
                        Ok(Ok(_stream)) => {
                            let rtt = start.elapsed().as_micros() as u64;
                            entry.reset_circuit();
                            entry.record_success(rtt);
                            info!(
                                steed_id = %steed.id,
                                endpoint = %endpoint,
                                rtt_micros = rtt,
                                "🐎 [HIPPODROME-HEALTH] Steed successfully revived by proactive probe; circuit restored to Closed!"
                            );
                        }
                        _ => {
                            debug!(
                                steed_id = %steed.id,
                                endpoint = %endpoint,
                                "Steed failed proactive probe; remains in penalty box"
                            );
                        }
                    }
                }
            }
        }
    }
}

impl Drop for HealthProbeGuard {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
    }
}
