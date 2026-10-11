//! # Adversarial Racing & Resilience Tests
//!
//! Asserts Hippodrome's speculative concurrency, hedged racing, circuit breaking,
//! and sovereign hygiene demotion under simulated hostile network conditions.

use hippodrome::{
    execute_hedged_race, CircuitState, HippodromeError, HippodromeTournament,
    HygieneInfraction, RaceConfig, Steed,
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_fast_primary_wins_before_hedged_delay() {
    let primary = Steed::new("fast-horse", "10.0.0.1:53");
    let secondary = Some(Steed::new("slow-horse", "10.0.0.2:53"));

    let config = RaceConfig {
        hedged_delay: Duration::from_millis(50),
        hard_timeout: Duration::from_millis(1000),
    };

    let outcome = execute_hedged_race(primary.clone(), secondary, config, |steed| async move {
        if steed.id == "fast-horse" {
            tokio::time::sleep(Duration::from_millis(5)).await;
            Ok("FAST_PACKET".to_string())
        } else {
            tokio::time::sleep(Duration::from_millis(300)).await;
            Ok("SLOW_PACKET".to_string())
        }
    })
    .await
    .expect("Fast primary must win");

    assert_eq!(outcome.value, "FAST_PACKET");
    assert_eq!(outcome.winner.id, "fast-horse");
    assert!(!outcome.was_hedged, "Should not be hedged since primary finished in 5ms < 50ms");
}

#[tokio::test]
async fn test_stalled_primary_beaten_by_speculative_secondary() {
    let primary = Steed::new("stalled-primary", "10.0.0.1:53");
    let secondary = Some(Steed::new("swift-backup", "10.0.0.2:53"));

    let config = RaceConfig {
        hedged_delay: Duration::from_millis(10), // Hedged trigger after 10ms
        hard_timeout: Duration::from_millis(1000),
    };

    let outcome = execute_hedged_race(primary, secondary, config, |steed| async move {
        if steed.id == "stalled-primary" {
            // Simulated fiber cut / 500ms freeze
            tokio::time::sleep(Duration::from_millis(500)).await;
            Ok("STALLED_PACKET".to_string())
        } else {
            // Backup replies in 5ms
            tokio::time::sleep(Duration::from_millis(5)).await;
            Ok("SWIFT_BACKUP_PACKET".to_string())
        }
    })
    .await
    .expect("Secondary steed must rescue query");

    assert_eq!(outcome.value, "SWIFT_BACKUP_PACKET");
    assert_eq!(outcome.winner.id, "swift-backup");
    assert!(outcome.was_hedged, "Race was won via speculative hedging!");
    // Total latency was hedged_delay (10ms) + secondary execution (~5ms) = ~15ms, NOT 500ms!
    assert!(outcome.rtt_micros < 100_000, "Measured RTT must be under 100ms");
}

#[tokio::test]
async fn test_circuit_breaker_trips_after_consecutive_failures() {
    let tournament = HippodromeTournament::new(RaceConfig::default());
    let flaky_steed = Steed::new("flaky-upstream", "10.0.0.99:53");
    tournament.register(flaky_steed.clone());

    let attempt_counter = Arc::new(AtomicU32::new(0));

    // Simulate 3 consecutive failures
    for _ in 0..3 {
        let counter = Arc::clone(&attempt_counter);
        let _: Result<hippodrome::RaceOutcome<String>, _> = tournament
            .race(move |_| {
                let c = Arc::clone(&counter);
                async move {
                    c.fetch_add(1, Ordering::Relaxed);
                    Err(HippodromeError::Timeout {
                        endpoint: "10.0.0.99:53".to_string(),
                        timeout_ms: 100,
                    })
                }
            })
            .await;
    }

    // Verify circuit breaker tripped to Open
    let report = tournament.report();
    let entry = report.iter().find(|r| r.steed.id == "flaky-upstream").unwrap();
    assert_eq!(entry.scoreboard.circuit_state, CircuitState::Open);
    assert_eq!(entry.scoreboard.consecutive_failures, 3);

    // Verify flaky upstream is excluded from subsequent races
    let eligible = tournament.rank_contenders();
    assert!(!eligible.iter().any(|s| s.id == "flaky-upstream"), "Tripped steed must be excluded");
}

#[tokio::test]
async fn test_hygiene_infraction_demotes_untrusted_upstream() {
    let tournament = HippodromeTournament::new(RaceConfig::default());
    let rogue = Steed::new("rogue-dns", "10.0.0.66:53");
    let honest = Steed::new("honest-dns", "10.0.0.77:53");

    tournament.register(rogue.clone());
    tournament.register(honest.clone());

    // Initially both have clean trust (100)
    let pre_report = tournament.report();
    for r in pre_report {
        assert_eq!(r.hygiene.trust_score, 100);
    }

    // Report a DNS Rebinding infraction on rogue
    tournament.report_infraction("rogue-dns", HygieneInfraction::DnsRebindingAttempt);

    let post_report = tournament.report();
    let rogue_post = post_report.iter().find(|r| r.steed.id == "rogue-dns").unwrap();
    assert_eq!(rogue_post.hygiene.trust_score, 50);
    assert_eq!(rogue_post.hygiene.rebinding_violations, 1);

    // Assert honest steed is ranked #1 due to virtual latency penalty on rogue
    let ranked = tournament.rank_contenders();
    assert_eq!(ranked[0].id, "honest-dns", "Honest steed must take pole position");
}

#[tokio::test]
async fn test_all_steeds_failed_returns_clean_error_without_panic() {
    let primary = Steed::new("dead-1", "10.0.0.1:53");
    let secondary = Some(Steed::new("dead-2", "10.0.0.2:53"));

    let config = RaceConfig {
        hedged_delay: Duration::from_millis(5),
        hard_timeout: Duration::from_millis(50),
    };

    let result: Result<hippodrome::RaceOutcome<String>, _> = execute_hedged_race(primary, secondary, config, |steed| async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Err(HippodromeError::Transport(steed.endpoint, "Connection refused".to_string()))
    })
    .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), HippodromeError::AllSteedsFailed);
}
