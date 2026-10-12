//! # Hippodrome CLI
//!
//! Sovereign Speculative Hedged Racing & Upstream Tournament Command-Line Bridge.

use clap::{Parser, Subcommand};
use hippodrome::{HippodromeDnsRacer, RaceConfig};
use std::time::{Duration, Instant};

#[derive(Parser)]
#[command(
    name = "hippodrome",
    version,
    author = "Rick Walker <rick@rmediatech.com>"
)]
#[command(about = "Sovereign Speculative Hedged Racing & Upstream Tournament Engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute a live speculative race across upstreams for a domain query
    Race {
        /// Target domain name to query (e.g. "google.com")
        domain: String,
        /// Comma-separated list of upstream addresses (e.g. "1.1.1.1:53,8.8.8.8:53")
        #[arg(short, long, default_value = "1.1.1.1:53,8.8.8.8:53,9.9.9.9:53")]
        upstreams: String,
        /// Speculative hedged delay in milliseconds (default: 10ms)
        #[arg(long, default_value = "10")]
        hedged_ms: u64,
    },
    /// Benchmark Hippodrome speculative racing vs naive round-robin
    Bench {
        /// Target domain name to query
        domain: String,
        /// Comma-separated list of upstream addresses
        #[arg(short, long, default_value = "1.1.1.1:53,8.8.8.8:53")]
        upstreams: String,
        /// Total query iterations to run
        #[arg(short, long, default_value = "20")]
        count: usize,
    },
    /// Display tournament standings and telemetry
    Status {
        /// Comma-separated list of upstream addresses
        #[arg(short, long, default_value = "1.1.1.1:53,8.8.8.8:53,9.9.9.9:53")]
        upstreams: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Race {
            domain,
            upstreams,
            hedged_ms,
        } => {
            println!("🐎 HIPPODROME SPECULATIVE TOURNAMENT RACE");
            println!("🎯 Target Domain : {}", domain);
            println!("⚡ Hedged Delay  : {}ms", hedged_ms);

            let upstream_list = parse_upstreams(&upstreams);
            let config = RaceConfig {
                hedged_delay: Duration::from_millis(hedged_ms),
                hard_timeout: Duration::from_millis(2000),
                ..RaceConfig::default()
            };

            let racer = HippodromeDnsRacer::new(upstream_list, config);
            let packet = build_dns_query(&domain, 0x1337);

            let t0 = Instant::now();
            match racer.race_query(packet).await {
                Ok(outcome) => {
                    let total_elapsed = t0.elapsed();
                    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    println!(
                        "🏆 RACE WINNER     : {} ({})",
                        outcome.winner.id, outcome.winner.endpoint
                    );
                    println!(
                        "⏱️  Measured RTT    : {} µs ({:.2} ms)",
                        outcome.rtt_micros,
                        outcome.rtt_micros as f64 / 1000.0
                    );
                    println!(
                        "⚡ Total Execution : {:.2} ms",
                        total_elapsed.as_secs_f64() * 1000.0
                    );
                    println!(
                        "🏇 Speculative Win : {}",
                        if outcome.was_hedged {
                            "YES (Hedged secondary won)"
                        } else {
                            "NO (Primary won before hedged)"
                        }
                    );
                    println!("📦 Response Bytes  : {} bytes", outcome.value.len());
                    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                }
                Err(e) => {
                    eprintln!("❌ Race failed: {e}");
                }
            }
        }

        Commands::Bench {
            domain,
            upstreams,
            count,
        } => {
            println!("🐎 HIPPODROME VS NAIVE ROUND-ROBIN BENCHMARK");
            println!("🎯 Target Domain : {} ({} iterations)", domain, count);

            let upstream_list = parse_upstreams(&upstreams);
            let config = RaceConfig::default();
            let racer = HippodromeDnsRacer::new(upstream_list.clone(), config);

            let mut hippodrome_times = Vec::with_capacity(count);
            for i in 0..count {
                let packet = build_dns_query(&domain, 0x2000 + i as u16);
                let _start = Instant::now();
                if let Ok(outcome) = racer.race_query(packet).await {
                    hippodrome_times.push(outcome.rtt_micros);
                }
                tokio::time::sleep(Duration::from_millis(15)).await;
            }

            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            if !hippodrome_times.is_empty() {
                hippodrome_times.sort_unstable();
                let p50 = hippodrome_times[hippodrome_times.len() * 50 / 100];
                let p90 = hippodrome_times[hippodrome_times.len() * 90 / 100];
                let p99 = hippodrome_times[hippodrome_times.len() * 99 / 100];
                let min = hippodrome_times[0];
                let max = hippodrome_times[hippodrome_times.len() - 1];

                println!("📊 HIPPODROME SPECULATIVE RACING LATENCY:");
                println!("   Min : {:.2} ms", min as f64 / 1000.0);
                println!("   P50 : {:.2} ms", p50 as f64 / 1000.0);
                println!("   P90 : {:.2} ms", p90 as f64 / 1000.0);
                println!("   P99 : {:.2} ms", p99 as f64 / 1000.0);
                println!("   Max : {:.2} ms", max as f64 / 1000.0);
            }
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        }

        Commands::Status { upstreams } => {
            println!("🐎 HIPPODROME TOURNAMENT SCOREBOARD");
            let upstream_list = parse_upstreams(&upstreams);
            let racer = HippodromeDnsRacer::new(upstream_list, RaceConfig::default());
            let reports = racer.tournament().report();

            println!(
                "{:<15} {:<20} {:<12} {:<10} {:<10}",
                "ID", "ENDPOINT", "EWMA (ms)", "WINS", "CIRCUIT"
            );
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            for r in reports {
                println!(
                    "{:<15} {:<20} {:<12.2} {:<10} {:?}",
                    r.steed.id,
                    r.steed.endpoint,
                    r.effective_latency_micros as f64 / 1000.0,
                    r.scoreboard.total_wins,
                    r.scoreboard.circuit_state
                );
            }
        }
    }

    Ok(())
}

fn parse_upstreams(input: &str) -> Vec<(String, String)> {
    input
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .enumerate()
        .map(|(i, addr)| {
            let id = format!("steed-{}", i + 1);
            (id, addr.trim().to_string())
        })
        .collect()
}

fn build_dns_query(domain: &str, tx_id: u16) -> Vec<u8> {
    let mut packet = Vec::with_capacity(64);
    packet.extend_from_slice(&tx_id.to_be_bytes());
    packet.extend_from_slice(&[0x01, 0x00]); // Standard query, RD=1
    packet.extend_from_slice(&[0x00, 0x01]); // QDCOUNT = 1
    packet.extend_from_slice(&[0x00, 0x00]); // ANCOUNT = 0
    packet.extend_from_slice(&[0x00, 0x00]); // NSCOUNT = 0
    packet.extend_from_slice(&[0x00, 0x00]); // ARCOUNT = 0

    for part in domain.split('.') {
        if !part.is_empty() {
            packet.push(part.len() as u8);
            packet.extend_from_slice(part.as_bytes());
        }
    }
    packet.push(0x00); // Root label
    packet.extend_from_slice(&[0x00, 0x01]); // QTYPE = A (1)
    packet.extend_from_slice(&[0x00, 0x01]); // QCLASS = IN (1)

    packet
}
