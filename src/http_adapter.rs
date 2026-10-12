//! # Sovereign L7 HTTP Speculative Racing Adapter
//!
//! Provides intelligent speculative hedged routing for HTTP/1.1 and HTTP/2
//! reverse proxies and microservice gateways.
//!
//! - **Idempotent requests (`GET`, `HEAD`, `OPTIONS`, `TRACE`):**
//!   Dispatched via speculative hedged racing across the top-ranked steeds.
//!   Eliminates tail latency spikes with microsecond concurrency.
//! - **Mutating requests (`POST`, `PUT`, `DELETE`, `PATCH`):**
//!   Dispatched with safe single-flight policy to prevent duplicate side-effects,
//!   backed by instantaneous circuit-breaker failover.

use crate::error::HippodromeError;
use crate::steed::Steed;
use crate::tournament::HippodromeTournament;
use std::future::Future;

/// Operational dispatch policy evaluated per HTTP request method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethodPolicy {
    /// Safe for concurrent speculative hedged racing (GET, HEAD, OPTIONS, TRACE).
    IdempotentHedged,
    /// Must only be dispatched to a single upstream (POST, PUT, DELETE, PATCH).
    MutatingSingleFlight,
}

impl HttpMethodPolicy {
    /// Classify an HTTP method string into its dispatch safety category.
    pub fn classify(method: &str) -> Self {
        let m = method.trim().to_ascii_uppercase();
        match m.as_str() {
            "GET" | "HEAD" | "OPTIONS" | "TRACE" => Self::IdempotentHedged,
            _ => Self::MutatingSingleFlight,
        }
    }

    /// Whether this policy allows concurrent hedged speculative execution.
    pub fn is_idempotent(&self) -> bool {
        matches!(self, Self::IdempotentHedged)
    }
}

/// The result of an HTTP dispatch operation through Hippodrome.
#[derive(Debug)]
pub struct HttpRaceResult<T> {
    pub value: T,
    pub winner: Steed,
    pub rtt_micros: u64,
    pub was_hedged: bool,
    pub policy: HttpMethodPolicy,
}

/// High-level HTTP reverse proxy speculative racer.
#[derive(Debug, Clone)]
pub struct HippodromeHttpRacer {
    tournament: HippodromeTournament,
}

impl HippodromeHttpRacer {
    /// Wrap a configured tournament in an HTTP speculative racing adapter.
    pub fn new(tournament: HippodromeTournament) -> Self {
        Self { tournament }
    }

    /// Retrieve reference to the underlying tournament.
    pub fn tournament(&self) -> &HippodromeTournament {
        &self.tournament
    }

    /// Dispatch an HTTP request with automatic method-aware policy evaluation.
    pub async fn dispatch<T, F, Fut>(
        &self,
        method: &str,
        runner: F,
    ) -> Result<HttpRaceResult<T>, HippodromeError>
    where
        F: Fn(Steed) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, HippodromeError>> + Send + 'static,
        T: Send + 'static,
    {
        let policy = HttpMethodPolicy::classify(method);

        match policy {
            HttpMethodPolicy::IdempotentHedged => {
                // Idempotent: Full speculative hedged tournament race!
                let outcome = self.tournament.race(runner).await?;
                Ok(HttpRaceResult {
                    value: outcome.value,
                    winner: outcome.winner,
                    rtt_micros: outcome.rtt_micros,
                    was_hedged: outcome.was_hedged,
                    policy,
                })
            }
            HttpMethodPolicy::MutatingSingleFlight => {
                // Mutating: Single flight to top-ranked contender with failover
                self.dispatch_single_flight(runner, policy).await
            }
        }
    }

    /// Dispatches a non-idempotent request to the top contender with safe failover.
    async fn dispatch_single_flight<T, F, Fut>(
        &self,
        runner: F,
        policy: HttpMethodPolicy,
    ) -> Result<HttpRaceResult<T>, HippodromeError>
    where
        F: Fn(Steed) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, HippodromeError>> + Send + 'static,
        T: Send + 'static,
    {
        let contenders = self.tournament.rank_contenders();
        if contenders.is_empty() {
            return Err(HippodromeError::NoUpstreamsAvailable);
        }

        let primary = contenders[0].clone();
        let secondary = contenders.get(1).cloned();
        let hard_timeout = self.tournament.config().hard_timeout;

        let start = std::time::Instant::now();
        let primary_res = match tokio::time::timeout(hard_timeout, runner(primary.clone())).await {
            Ok(res) => res,
            Err(_) => Err(HippodromeError::Timeout {
                endpoint: primary.endpoint.clone(),
                timeout_ms: hard_timeout.as_millis() as u64,
            }),
        };

        match primary_res {
            Ok(val) => {
                let elapsed_us = start.elapsed().as_micros() as u64;
                if let Some(entry) = self.tournament.scoreboard(&primary.id) {
                    entry.record_win(elapsed_us);
                }
                Ok(HttpRaceResult {
                    value: val,
                    winner: primary,
                    rtt_micros: elapsed_us,
                    was_hedged: false,
                    policy,
                })
            }
            Err(e) => {
                if let Some(entry) = self.tournament.scoreboard(&primary.id) {
                    entry.record_failure();
                }

                // If primary failed and we have a secondary, attempt fallback
                if let Some(sec) = secondary {
                    let sec_start = std::time::Instant::now();
                    let sec_res = match tokio::time::timeout(hard_timeout, runner(sec.clone())).await {
                        Ok(res) => res,
                        Err(_) => Err(HippodromeError::Timeout {
                            endpoint: sec.endpoint.clone(),
                            timeout_ms: hard_timeout.as_millis() as u64,
                        }),
                    };

                    match sec_res {
                        Ok(sec_val) => {
                            let sec_elapsed_us = sec_start.elapsed().as_micros() as u64;
                            if let Some(entry) = self.tournament.scoreboard(&sec.id) {
                                entry.record_win(sec_elapsed_us);
                            }
                            Ok(HttpRaceResult {
                                value: sec_val,
                                winner: sec,
                                rtt_micros: sec_elapsed_us,
                                was_hedged: false,
                                policy,
                            })
                        }
                        Err(sec_err) => {
                            if let Some(entry) = self.tournament.scoreboard(&sec.id) {
                                entry.record_failure();
                            }
                            Err(sec_err)
                        }
                    }
                } else {
                    Err(e)
                }
            }
        }
    }
}
