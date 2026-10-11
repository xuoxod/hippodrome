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
    /// Delay before dispatching the speculative secondary steed (default: 10ms).
    pub hedged_delay: Duration,
    /// Hard deadline ceiling for the entire race before timing out (default: 2000ms).
    pub hard_timeout: Duration,
}

impl Default for RaceConfig {
    fn default() -> Self {
        Self {
            hedged_delay: Duration::from_millis(10),
            hard_timeout: Duration::from_millis(2000),
        }
    }
}

/// Execute a speculative hedged race across two selected steeds (Primary & Secondary).
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
    let hedged_timer = tokio::time::sleep(config.hedged_delay);
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
            let remaining = config.hard_timeout.saturating_sub(start_instant.elapsed());
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
    let total_remaining = config.hard_timeout.saturating_sub(start_instant.elapsed());
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
                    timeout_ms: config.hard_timeout.as_millis() as u64,
                });
            }
        }
    }

    Err(HippodromeError::AllSteedsFailed)
}
