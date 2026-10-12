//! Behavioral & Safety Tests: L7 HTTP Speculative Racing Adapter
//! Conforms to AGY-RULE-SOVEREIGN-FLAGSHIP-01 & AGY-RULE-SOVEREIGN-SECURITY-01.

use hippodrome::{HippodromeHttpRacer, HippodromeTournament, HttpMethodPolicy, RaceConfig, Steed};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_http_method_classification_and_policy() {
    assert_eq!(
        HttpMethodPolicy::classify("GET"),
        HttpMethodPolicy::IdempotentHedged
    );
    assert_eq!(
        HttpMethodPolicy::classify("get"),
        HttpMethodPolicy::IdempotentHedged
    );
    assert_eq!(
        HttpMethodPolicy::classify("HEAD"),
        HttpMethodPolicy::IdempotentHedged
    );
    assert_eq!(
        HttpMethodPolicy::classify("OPTIONS"),
        HttpMethodPolicy::IdempotentHedged
    );

    assert_eq!(
        HttpMethodPolicy::classify("POST"),
        HttpMethodPolicy::MutatingSingleFlight
    );
    assert_eq!(
        HttpMethodPolicy::classify("PUT"),
        HttpMethodPolicy::MutatingSingleFlight
    );
    assert_eq!(
        HttpMethodPolicy::classify("DELETE"),
        HttpMethodPolicy::MutatingSingleFlight
    );
    assert_eq!(
        HttpMethodPolicy::classify("PATCH"),
        HttpMethodPolicy::MutatingSingleFlight
    );
}

#[tokio::test]
async fn test_idempotent_get_executes_hedged_racing() {
    let tournament = HippodromeTournament::new(RaceConfig {
        hedged_delay: Duration::from_millis(10),
        hard_timeout: Duration::from_millis(500),
        ..RaceConfig::default()
    });

    tournament.register(Steed::new("primary", "127.0.0.1:8001"));
    tournament.register(Steed::new("backup", "127.0.0.1:8002"));

    let racer = HippodromeHttpRacer::new(tournament);

    let res = racer
        .dispatch("GET", |steed| async move {
            if steed.id == "primary" {
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok("PRIMARY_RESPONSE".to_string())
            } else {
                tokio::time::sleep(Duration::from_millis(5)).await;
                Ok("BACKUP_RESPONSE".to_string())
            }
        })
        .await
        .expect("GET dispatch must succeed");

    assert_eq!(res.value, "BACKUP_RESPONSE");
    assert_eq!(res.winner.id, "backup");
    assert!(res.was_hedged);
    assert_eq!(res.policy, HttpMethodPolicy::IdempotentHedged);
}

#[tokio::test]
async fn test_mutating_post_strictly_enforces_single_flight() {
    let tournament = HippodromeTournament::new(RaceConfig::default());
    let steed_a = Steed::new("primary", "127.0.0.1:8001");
    let steed_b = Steed::new("backup", "127.0.0.1:8002");

    tournament.register(steed_a);
    tournament.register(steed_b);

    let racer = HippodromeHttpRacer::new(tournament);

    let execution_counts = Arc::new(AtomicU32::new(0));
    let counts_clone = Arc::clone(&execution_counts);

    let res = racer
        .dispatch("POST", move |_steed| {
            let cnt = Arc::clone(&counts_clone);
            async move {
                cnt.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                Ok("MUTATION_SUCCESS".to_string())
            }
        })
        .await
        .expect("POST dispatch must succeed");

    assert_eq!(res.value, "MUTATION_SUCCESS");
    assert_eq!(res.policy, HttpMethodPolicy::MutatingSingleFlight);
    assert!(!res.was_hedged);
    assert_eq!(
        execution_counts.load(Ordering::SeqCst),
        1,
        "POST must be executed exactly once without speculative duplicate flights"
    );
}

#[tokio::test]
async fn test_mutating_post_fails_over_to_backup_on_primary_error() {
    let tournament = HippodromeTournament::new(RaceConfig::default());
    tournament.register(Steed::new("dead-primary", "127.0.0.1:8001"));
    tournament.register(Steed::new("healthy-backup", "127.0.0.1:8002"));

    let racer = HippodromeHttpRacer::new(tournament);

    let res = racer
        .dispatch("POST", |steed| async move {
            if steed.id == "dead-primary" {
                Err(hippodrome::HippodromeError::Transport(
                    steed.endpoint,
                    "Connection refused".to_string(),
                ))
            } else {
                Ok("BACKUP_SAVED".to_string())
            }
        })
        .await
        .expect("POST should fail over to healthy backup");

    assert_eq!(res.value, "BACKUP_SAVED");
    assert_eq!(res.winner.id, "healthy-backup");
}
