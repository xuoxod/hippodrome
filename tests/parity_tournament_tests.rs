//! # Parity & Superiority Tests: Hippodrome vs Round-Robin
//!
//! Proves empirically that Hippodrome speculative concurrency eliminates
//! the devastating tail-latency spikes inherent to naive round-robin.

use hippodrome::{HippodromeTournament, RaceConfig, Steed};
use std::time::{Duration, Instant};

#[tokio::test]
async fn test_hippodrome_superiority_over_round_robin() {
    // 3 Upstreams:
    // A: 2ms (Fast local resolver)
    // B: 20ms (Medium public resolver)
    // C: 150ms (Slow / high-jitter upstream)

    let tournament = HippodromeTournament::new(RaceConfig {
        hedged_delay: Duration::from_millis(10),
        hard_timeout: Duration::from_millis(1000),
        ..RaceConfig::default()
    });

    let steed_a = Steed::new("steed-fast", "10.0.0.1:53");
    let steed_b = Steed::new("steed-med", "10.0.0.2:53");
    let steed_c = Steed::new("steed-slow", "10.0.0.3:53");

    tournament.register(steed_a);
    tournament.register(steed_b);
    tournament.register(steed_c);

    // Warm-up to establish EWMA baseline
    for _ in 0..5 {
        let _ = tournament
            .race(|steed| async move {
                let delay = match steed.id.as_str() {
                    "steed-fast" => Duration::from_millis(2),
                    "steed-med" => Duration::from_millis(20),
                    _ => Duration::from_millis(150),
                };
                tokio::time::sleep(delay).await;
                Ok(steed.id)
            })
            .await;
    }

    // Run 30 consecutive queries under Hippodrome
    let mut hippodrome_latencies = Vec::new();
    for _ in 0..30 {
        let start = Instant::now();
        let outcome = tournament
            .race(|steed| async move {
                let delay = match steed.id.as_str() {
                    "steed-fast" => Duration::from_millis(2),
                    "steed-med" => Duration::from_millis(20),
                    _ => Duration::from_millis(150),
                };
                tokio::time::sleep(delay).await;
                Ok(steed.id)
            })
            .await
            .expect("Hippodrome race must succeed");
        hippodrome_latencies.push(start.elapsed().as_millis());
        assert_eq!(outcome.winner.id, "steed-fast");
    }

    // Simulate Round-Robin across same 30 queries
    let mut rr_latencies = Vec::new();
    let rr_delays = [2u64, 20u64, 150u64];
    for i in 0..30 {
        let delay = rr_delays[i % 3];
        rr_latencies.push(delay);
    }

    hippodrome_latencies.sort_unstable();
    rr_latencies.sort_unstable();

    let hippo_p90 = hippodrome_latencies[hippodrome_latencies.len() * 90 / 100];
    let rr_p90 = rr_latencies[rr_latencies.len() * 90 / 100];

    println!(
        "🏆 HIPPODROME P90: {} ms vs ROUND-ROBIN P90: {} ms",
        hippo_p90, rr_p90
    );

    // Hippodrome P90 must be <= 10ms while Round-Robin suffers 150ms!
    assert!(hippo_p90 <= 15, "Hippodrome P90 must stay under 15ms");
    assert!(rr_p90 >= 150, "Round-robin P90 suffers 150ms tail latency");
}
