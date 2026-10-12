# 📋 Hippodrome Changelog & Architectural Ledger

All notable changes, architectural milestones, and security invariants for **Hippodrome** (`hippodrome`) are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.1] - 2026-10-12 (Hardened Mutating Failover & Bounded Timeouts)

### 🌟 Added & Fortified
* **Bounded Mutating Request Failover (`src/http_adapter.rs`)**:
  - In `HippodromeHttpRacer::dispatch_single_flight`, wrapped upstream flights with `hard_timeout` so hung upstreams or Slowloris conditions fail over to backup within the deadline rather than blocking worker threads indefinitely.
* **Adversarial Security Tests (`tests/http_speculative_racing_tests.rs`)**:
  - Added `test_mutating_post_times_out_and_fails_over_when_primary_hangs` asserting bounded failover under hanging upstreams.
  - Added `test_mutating_post_fails_cleanly_when_all_upstreams_hang` asserting clean error returns without thread starvation.

---

## [0.1.0] - 2026-10-12 (Initial Sovereign Public Release)

### 🌟 Initial Capabilities
* **Fastest-Horse Upstream Tournament Racing (`src/tournament.rs`)**:
  - Speculative concurrent execution across ranked upstreams with microsecond EWMA latency tracking.
* **Adaptive Hedged Concurrency (`src/hedged.rs`)**:
  - Eliminates tail-latency spikes with dynamic delay calculation ($T_{hedged}$).
* **Sovereign Hygiene Ledger & Circuit Breakers (`src/hygiene.rs`, `src/scoreboard.rs`)**:
  - Dynamic infraction penalization, exponential backoff, and automatic circuit-breaker tripping.
* **L7 HTTP Reverse Proxy Adapter (`src/http_adapter.rs`)**:
  - Method-aware dispatch: idempotent hedged racing vs. safe single-flight mutating dispatches.
* **Proactive Microsecond Health Probing (`src/health.rs`)**:
  - Background probe guard automatically testing and reviving tripped steeds.
