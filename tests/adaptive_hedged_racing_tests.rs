//! Adversarial & Behavioral Tests: Adaptive Hedged Racing Engine
//! Conforms to AGY-RULE-SOVEREIGN-FLAGSHIP-01 & AGY-RULE-SOVEREIGN-SECURITY-01.

use hippodrome::{HippodromeTournament, RaceConfig, Steed};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_adaptive_delay_calculation_and_bounds() {
    let config = RaceConfig {
        hedged_delay: Duration::from_millis(10),
        hard_timeout: Duration::from_millis(1000),
        adaptive: true,
        ewma_multiplier: 1.25,
        min_hedged_delay: Duration::from_millis(2),
        max_hedged_delay: Duration::from_millis(50),
    };

    // Sub-millisecond EWMA (500µs) -> clamped up to min 2ms floor
    assert_eq!(config.compute_hedged_delay(500), Duration::from_millis(2));

    // Fast EWMA (4000µs = 4ms) -> 4ms * 1.25 = 5ms (5000µs)
    assert_eq!(
        config.compute_hedged_delay(4000),
        Duration::from_micros(5000)
    );

    // Slow EWMA (60,000µs = 60ms) -> 60ms * 1.25 = 75ms -> clamped down to 50ms ceiling
    assert_eq!(
        config.compute_hedged_delay(60_000),
        Duration::from_millis(50)
    );

    // Adaptive disabled -> always returns base hedged_delay
    let static_config = RaceConfig {
        adaptive: false,
        ..config
    };
    assert_eq!(
        static_config.compute_hedged_delay(4000),
        Duration::from_millis(10)
    );
}

#[tokio::test]
async fn test_adaptive_hedging_beats_tail_latency_spike() {
    let config = RaceConfig {
        hedged_delay: Duration::from_millis(100), // High static delay
        hard_timeout: Duration::from_millis(1000),
        adaptive: true,
        ewma_multiplier: 1.25,
        min_hedged_delay: Duration::from_millis(3),
        max_hedged_delay: Duration::from_millis(50),
    };

    let tournament = HippodromeTournament::new(config);
    let steed_a = Steed::new("primary-steed", "127.0.0.1:8001");
    let steed_b = Steed::new("backup-steed", "127.0.0.1:8002");

    tournament.register(steed_a.clone());
    tournament.register(steed_b.clone());

    // Train primary steed to a fast 4ms EWMA baseline
    if let Some(entry) = tournament.scoreboard(&steed_a.id) {
        for _ in 0..5 {
            entry.record_win(4000); // 4ms
        }
    }

    let backup_dispatches = Arc::new(AtomicU32::new(0));
    let backup_dispatches_clone = Arc::clone(&backup_dispatches);

    // Primary experiences sudden 200ms tail-latency spike; backup responds in 4ms
    let outcome = tournament
        .race(move |steed| {
            let backup_cnt = Arc::clone(&backup_dispatches_clone);
            async move {
                if steed.id == "primary-steed" {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok("PRIMARY_SLOW".to_string())
                } else {
                    backup_cnt.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(4)).await;
                    Ok("BACKUP_FAST".to_string())
                }
            }
        })
        .await
        .expect("Race must succeed via speculative hedge");

    assert_eq!(outcome.value, "BACKUP_FAST");
    assert_eq!(outcome.winner.id, "backup-steed");
    assert!(outcome.was_hedged, "Outcome must be marked as hedged");
    assert_eq!(
        backup_dispatches.load(Ordering::SeqCst),
        1,
        "Backup must have been speculatively dispatched"
    );
}
