//! # Speculative Hedged Racing Engine
//!
//! Executes concurrent speculative races across ranked upstreams.
//! Fired with a hedged delay ($T_{hedged}$), eliminating tail latency
//! without flooding network bandwidth.

use crate::error::HippodromeError;
use crate::steed::Steed;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Result of a completed speculative tournament race.
#[derive(Debug)]
pub struct RaceOutcome<T> {
    /// The payload returned by the winning upstream.
    pub value: T,
    /// The victorious steed that crossed the finish line first.
    pub winner: Steed,
    /// Round-trip time measured for the winner in microseconds.
    pub rtt_micros: u64,
    /// Whether this race was decided via speculative hedging.
    pub was_hedged: bool,
}

/// Configuration options governing hedged racing execution.
#[derive(Debug, Clone, Copy)]
pub struct RaceConfig {
    /// Base delay before dispatching the speculative secondary steed (default: 10ms).
    pub hedged_delay: Duration,
    /// Hard deadline ceiling for the entire race before timing out (default: 2000ms).
    pub hard_timeout: Duration,
    /// Whether to dynamically calculate hedged delay from primary steed EWMA.
    pub adaptive: bool,
    /// EWMA multiplier when adaptive is active (default: 1.25).
    pub ewma_multiplier: f64,
    /// Minimum floor for hedged delay (default: 2ms).
    pub min_hedged_delay: Duration,
    /// Maximum ceiling for hedged delay (default: 50ms).
    pub max_hedged_delay: Duration,
}

impl Default for RaceConfig {
    fn default() -> Self {
        Self {
            hedged_delay: Duration::from_millis(10),
            hard_timeout: Duration::from_millis(2000),
            adaptive: true,
            ewma_multiplier: 1.25,
            min_hedged_delay: Duration::from_millis(2),
            max_hedged_delay: Duration::from_millis(50),
        }
    }
}

impl RaceConfig {
    /// Calculate effective hedged delay given the primary contender's EWMA in microseconds.
    pub fn compute_hedged_delay(&self, ewma_micros: u64) -> Duration {
        if !self.adaptive || ewma_micros == 0 {
            return self.hedged_delay;
        }
        let dynamic_micros = (ewma_micros as f64 * self.ewma_multiplier) as u64;
        let dynamic_dur = Duration::from_micros(dynamic_micros);
        dynamic_dur.clamp(self.min_hedged_delay, self.max_hedged_delay)
    }
}

/// Execute a speculative hedged race across two selected steeds using default config delay.
pub async fn execute_hedged_race<T, F, Fut>(
    primary: Steed,
    secondary: Option<Steed>,
    config: RaceConfig,
    runner: F,
) -> Result<RaceOutcome<T>, HippodromeError>
where
    F: Fn(Steed) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, HippodromeError>> + Send + 'static,
    T: Send + 'static,
{
    execute_hedged_race_with_delay(
        primary,
        secondary,
        config.hedged_delay,
        config.hard_timeout,
        runner,
    )
    .await
}

/// Execute a speculative hedged race with explicitly computed hedged delay.
pub async fn execute_hedged_race_with_delay<T, F, Fut>(
    primary: Steed,
    secondary: Option<Steed>,
    hedged_delay: Duration,
    hard_timeout: Duration,
    runner: F,
) -> Result<RaceOutcome<T>, HippodromeError>
where
    F: Fn(Steed) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, HippodromeError>> + Send + 'static,
    T: Send + 'static,
{
    let runner = Arc::new(runner);
    let start_instant = Instant::now();

    // 1. Dispatch Primary Steed
    let primary_runner = Arc::clone(&runner);
    let primary_steed = primary.clone();
    let mut primary_handle = tokio::spawn(async move {
        let p_start = Instant::now();
        let res = primary_runner(primary_steed).await;
        (res, p_start.elapsed())
    });

    let mut primary_failed = false;

    // 2. Wait up to hedged_delay for primary to finish
    let hedged_timer = tokio::time::sleep(hedged_delay);
    tokio::pin!(hedged_timer);

    tokio::select! {
        primary_res = &mut primary_handle => {
            match primary_res {
                Ok((Ok(val), elapsed)) => {
                    return Ok(RaceOutcome {
                        value: val,
                        winner: primary,
                        rtt_micros: elapsed.as_micros() as u64,
                        was_hedged: false,
                    });
                }
                _ => {
                    primary_failed = true;
                }
            }
        }
        _ = &mut hedged_timer => {
            // Primary hasn't crossed line in hedged_delay; speculative race begins!
        }
    }

    // 3. If no secondary steed available, simply wait for primary with hard timeout
    let secondary_steed = match secondary {
        Some(s) => s,
        None => {
            if primary_failed {
                return Err(HippodromeError::AllSteedsFailed);
            }
            let remaining = hard_timeout.saturating_sub(start_instant.elapsed());
            return match tokio::time::timeout(remaining, primary_handle).await {
                Ok(Ok((Ok(val), elapsed))) => Ok(RaceOutcome {
                    value: val,
                    winner: primary,
                    rtt_micros: elapsed.as_micros() as u64,
                    was_hedged: false,
                }),
                _ => Err(HippodromeError::AllSteedsFailed),
            };
        }
    };

    // 4. Dispatch Secondary Steed Speculatively in parallel
    let secondary_runner = Arc::clone(&runner);
    let sec_steed_clone = secondary_steed.clone();
    let mut secondary_handle = tokio::spawn(async move {
        let s_start = Instant::now();
        let res = secondary_runner(sec_steed_clone).await;
        (res, s_start.elapsed())
    });

    // 5. Speculative Race: Whichever steed returns valid response first wins!
    let total_remaining = hard_timeout.saturating_sub(start_instant.elapsed());
    let race_deadline = tokio::time::sleep(total_remaining);
    tokio::pin!(race_deadline);

    let mut secondary_failed = false;

    while !primary_failed || !secondary_failed {
        tokio::select! {
            res = &mut primary_handle, if !primary_failed => {
                match res {
                    Ok((Ok(val), elapsed)) => {
                        secondary_handle.abort(); // Cancel loser
                        return Ok(RaceOutcome {
                            value: val,
                            winner: primary,
                            rtt_micros: elapsed.as_micros() as u64,
                            was_hedged: true,
                        });
                    }
                    _ => {
                        primary_failed = true;
                    }
                }
            }
            res = &mut secondary_handle, if !secondary_failed => {
                match res {
                    Ok((Ok(val), elapsed)) => {
                        primary_handle.abort(); // Cancel loser
                        return Ok(RaceOutcome {
                            value: val,
                            winner: secondary_steed,
                            rtt_micros: elapsed.as_micros() as u64,
                            was_hedged: true,
                        });
                    }
                    _ => {
                        secondary_failed = true;
                    }
                }
            }
            _ = &mut race_deadline => {
                primary_handle.abort();
                secondary_handle.abort();
                return Err(HippodromeError::Timeout {
                    endpoint: format!("{} / {}", primary.endpoint, secondary_steed.endpoint),
                    timeout_ms: hard_timeout.as_millis() as u64,
                });
            }
        }
    }

    Err(HippodromeError::AllSteedsFailed)
}
