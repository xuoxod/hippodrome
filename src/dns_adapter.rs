//! # DNS Protocol Adapter for Hippodrome
//!
//! High-performance UDP wire runner for DNS speculative racing. Matches
//! transaction IDs and executes speculative concurrency directly on port 53.

use crate::error::HippodromeError;
use crate::hedged::{RaceConfig, RaceOutcome};
use crate::steed::Steed;
use crate::tournament::HippodromeTournament;
use std::sync::Arc;
use tokio::net::UdpSocket;

/// DNS speculative racer wrapping the Hippodrome tournament engine.
#[derive(Clone)]
pub struct HippodromeDnsRacer {
    tournament: HippodromeTournament,
}

impl HippodromeDnsRacer {
    /// Construct a new DNS racer with specified configuration and upstream endpoints.
    pub fn new(upstreams: Vec<(String, String)>, config: RaceConfig) -> Self {
        let tournament = HippodromeTournament::new(config);
        for (id, addr) in upstreams {
            tournament.register(Steed::new(id, addr));
        }
        Self { tournament }
    }

    /// Access the underlying tournament engine.
    pub fn tournament(&self) -> &HippodromeTournament {
        &self.tournament
    }

    /// Race a raw DNS query packet across upstreams.
    pub async fn race_query(
        &self,
        query_packet: Vec<u8>,
    ) -> Result<RaceOutcome<Vec<u8>>, HippodromeError> {
        if query_packet.len() < 12 {
            return Err(HippodromeError::InvalidPayload(
                "client".to_string(),
                "Query packet shorter than 12-byte DNS header".to_string(),
            ));
        }

        let expected_tx_id = u16::from_be_bytes([query_packet[0], query_packet[1]]);
        let packet_arc = Arc::new(query_packet);

        self.tournament
            .race(move |steed: Steed| {
                let packet = Arc::clone(&packet_arc);
                async move { send_udp_dns_query(&steed.endpoint, &packet, expected_tx_id).await }
            })
            .await
    }
}

/// Send UDP DNS query to single endpoint and validate response transaction ID.
async fn send_udp_dns_query(
    endpoint: &str,
    packet: &[u8],
    expected_tx_id: u16,
) -> Result<Vec<u8>, HippodromeError> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| HippodromeError::Transport(endpoint.to_string(), e.to_string()))?;

    socket
        .connect(endpoint)
        .await
        .map_err(|e| HippodromeError::Transport(endpoint.to_string(), e.to_string()))?;

    socket
        .send(packet)
        .await
        .map_err(|e| HippodromeError::Transport(endpoint.to_string(), e.to_string()))?;

    let mut buf = [0u8; 4096];
    let len = socket
        .recv(&mut buf)
        .await
        .map_err(|e| HippodromeError::Transport(endpoint.to_string(), e.to_string()))?;

    if len < 12 {
        return Err(HippodromeError::InvalidPayload(
            endpoint.to_string(),
            "Response truncated below 12-byte header".to_string(),
        ));
    }

    let resp_tx_id = u16::from_be_bytes([buf[0], buf[1]]);
    if resp_tx_id != expected_tx_id {
        return Err(HippodromeError::InvalidPayload(
            endpoint.to_string(),
            format!("Transaction ID mismatch: expected {expected_tx_id:#x}, got {resp_tx_id:#x}"),
        ));
    }

    Ok(buf[..len].to_vec())
}
