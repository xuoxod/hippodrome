# 🐎 Hippodrome (`hippodrome`)

> **Classification:** SOVEREIGN CONFIDENTIAL / OPEN ARCHITECTURE  
> **Sovereign Fleet Role:** Speculative Hedged Racing & Upstream Tournament Engine  
> **Standard:** `AGY-RULE-SOVEREIGN-FLAGSHIP-01` & `AGY-RULE-SOVEREIGN-SECURITY-01`  
> **Master Release Key:** `4E428019A109578B` (`Rick Walker <xuoxod@gmail.com>`)  
> **Operational Anthem:** *"From the wire to the RAM / 'Til the kernel drops that spam! / All them botnets crash and slam / Got that sovereign shield, GOD DAMN!"*  
> **Architects' Credo:** *"Real recognizes real, brother. No thanks needed between architects in the trenches."*  

[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![Security Standard](https://img.shields.io/badge/security-AGY--RULE--SOVEREIGN-green.svg)](GEMINI.md)
[![License: Dual MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

---

## 🏛️ What is Hippodrome?

In ancient Greece and the Byzantine Empire, the **Hippodrome** (*Ἱππόδρομος*, from *ἵππος* "horse" and *δρόμος* "course") was the grand arena where the fastest steeds competed in high-stakes chariot races before emperors and citizens.

In our sovereign systems fleet, **`hippodrome`** is our pure safe Rust **speculative hedged racing and tournament engine**. It permanently replaces naive, slow round-robin and sequential failover patterns across network forwarders and upstream proxies with microsecond "fastest-horse" speculative concurrency.

Traditional DNS resolvers and reverse proxies suffer from severe **tail latency** (P99/P99.9 spikes) whenever an upstream resolver experiences network congestion, packet drop, or route flaps. `hippodrome` defeats tail latency by executing hedged speculative races: dispatching requests to the historically fastest upstream, hedged by a secondary speculative dispatch if the primary fails to answer within a configurable microsecond window. The first valid upstream to cross the finish line wins, updating the dynamic scoreboard, while losing races are instantly aborted.

---

## ⚡ Ground-Floor Architecture

```text
                         🐎 THE HIPPODROME RACETRACK
                  ┌─────────────────────────────────────┐
                  │    Inbound Query / Forward Task     │
                  └──────────────────┬──────────────────┘
                                     │
                    ┌────────────────┴────────────────┐
                    ▼                                 ▼
         ┌──────────────────────┐          ┌──────────────────────┐
         │  Rank #1 Steed (0ms) │          │ Rank #2 Steed (10ms) │
         │  [Primary Dispatch]  │          │ [Speculative Hedged] │
         └──────────┬───────────┘          └──────────┬───────────┘
                    │                                 │
                    └────────────────┬────────────────┘
                                     ▼
                        🏆 FIRST TO CROSS FINISH LINE
                     (Loser canceled; Zero tail latency)
```

| Subsystem | Role & Implementation | Invariant |
| :--- | :--- | :--- |
| **`src/main.rs`** | Thin CLI Lifecycle Shell | Strict **$\le 80$ lines** invariant; zero inline business logic. |
| **`hippodrome::steed`** | Contender Entity | Encapsulates endpoint identities, operational state, and dynamic weights. |
| **`hippodrome::tournament`** | Tournament Arena & Standings | Manages dynamic rank sorting, EWMA latency tracking, and selection. |
| **`hippodrome::hedged`** | Hedged Request Executor | Asynchronous Tokio speculative racing engine with microsecond timers. |
| **`hippodrome::scoreboard`** | Adaptive Circuit Breaker | Trips failing upstreams, auto-heals upon recovery, tracks telemetry. |
| **`hippodrome::hygiene`** | Wire & Protocol Sanitizer | Verifies transaction IDs, sanitizes malformed responses. |
| **`hippodrome::dns_adapter`** | `HippodromeDnsRacer` | High-performance UDP wire runner natively embedded into `aegis`. |

---

## 🚀 Performance & Benchmark

Empirical verification executed across the sovereign fleet (Linux Node, Windows Server 2022 Node, Portable Edge Node, and Primary Workstation):

```text
🐎 HIPPODROME VS NAIVE ROUND-ROBIN BENCHMARK
🎯 Target Domain : google.com
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
📊 HIPPODROME SPECULATIVE RACING LATENCY:
   Min : 2.17 ms
   P50 : 2.46 ms
   P90 : 4.90 ms
   P99 : 4.90 ms
   Max : 4.90 ms
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

---

## 📜 Sovereign Cryptographic Provenance (`SOVEREIGN-SIGN-01`)

Every compiled binary is built as a static Musl binary and verified against Rick Walker's Sovereign Master Key:
- **Signer:** `Rick Walker (Sovereign Release Signing Key) <xuoxod@gmail.com>`
- **Key ID:** `4E428019A109578B`
- **Fingerprint:** `6BC8 DA5E 7F1B 8B49 BE5C D70A 4E42 8019 A109 578B`
