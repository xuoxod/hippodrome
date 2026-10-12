//! Adversarial & Behavioral Tests: Proactive Health Probing Sentinel
//! Conforms to AGY-RULE-SOVEREIGN-FLAGSHIP-01 & AGY-RULE-SOVEREIGN-SECURITY-01.

use hippodrome::{
    CircuitState, HealthProbeGuard, HippodromeTournament, ProbeConfig, RaceConfig, Steed,
};
use std::time::Duration;
use tokio::net::TcpListener;

#[tokio::test]
async fn test_proactive_probe_auto_heals_tripped_steed() {
    // Bind an ephemeral local TCP port to simulate a live backend service
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Must bind ephemeral test port");
    let addr = listener.local_addr().unwrap();

    let config = RaceConfig::default();
    let tournament = HippodromeTournament::new(config);
    let steed = Steed::new("flapping-backend", addr.to_string());
    tournament.register(steed.clone());

    let entry = tournament
        .scoreboard(&steed.id)
        .expect("Scoreboard entry must exist");

    // 1. Manually trip circuit open with consecutive failures
    for _ in 0..5 {
        entry.record_failure();
    }

    let now_ms = hippodrome::scoreboard::current_epoch_ms();
    assert_eq!(
        entry.circuit_state(now_ms),
        CircuitState::Open,
        "Circuit must be initially Tripped Open"
    );

    // Spawn dummy listener loop accepting probe connections
    let listener_handle = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            drop(stream);
        }
    });

    // 2. Execute proactive health probe sweep
    HealthProbeGuard::probe_sweep(&tournament, Duration::from_millis(500)).await;

    // 3. Verify circuit is restored to Closed (healthy)
    let after_ms = hippodrome::scoreboard::current_epoch_ms();
    assert_eq!(
        entry.circuit_state(after_ms),
        CircuitState::Closed,
        "Circuit must be autonomously reinstated to Closed by proactive probe"
    );

    listener_handle.abort();
}

#[tokio::test]
async fn test_health_probe_guard_lifecycle_and_shutdown() {
    let tournament = HippodromeTournament::new(RaceConfig::default());
    let probe_config = ProbeConfig {
        interval: Duration::from_millis(50),
        connect_timeout: Duration::from_millis(50),
    };

    let guard = HealthProbeGuard::start(tournament, probe_config);
    tokio::time::sleep(Duration::from_millis(120)).await;
    guard.stop();
}
