// Copyright (c) 2025-present Cesar Saguier Antebi
// All Rights Reserved.
//
// This file is part of the AIGEN Blockchain project.
// Licensed under the Business Source License 1.1 (BUSL-1.1).
// See LICENSE file in the project root for full license information.
//
// Commercial use requires express written consent and royalty agreements.
// Contact: Cesar Saguier Antebi

use std::cmp::Ordering;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use libp2p::PeerId;

#[derive(Clone, Debug)]
pub enum FailureReason {
    InvalidSignature,
    InvalidPoIProof,
    Timeout,
    MalformedMessage,
    InvalidModelShard,
    ModelTransferTimeout,
    ModelHashMismatch,
}

#[derive(Clone, Debug)]
pub struct PeerReputation {
    pub peer_id: PeerId,
    pub score: f64,
    pub successful_msgs: u64,
    pub failed_msgs: u64,
    pub last_seen: Instant,
    pub is_banned: bool,
}

#[derive(Debug)]
pub struct PeerReputationManager {
    pub peers: DashMap<PeerId, PeerReputation>,
    pub ban_threshold: f64,
    pub stale_ttl: Duration,
}

impl Default for PeerReputationManager {
    fn default() -> Self {
        Self {
            peers: DashMap::new(),
            ban_threshold: 0.2,
            stale_ttl: Duration::from_secs(60 * 60),
        }
    }
}

impl PeerReputationManager {
    pub fn new(ban_threshold: f64, stale_ttl: Duration) -> Self {
        Self {
            peers: DashMap::new(),
            ban_threshold,
            stale_ttl,
        }
    }

    pub fn record_success(&self, peer: PeerId) {
        let now = Instant::now();
        let mut entry = self.peers.entry(peer).or_insert_with(|| PeerReputation {
            peer_id: peer,
            score: 0.5,
            successful_msgs: 0,
            failed_msgs: 0,
            last_seen: now,
            is_banned: false,
        });

        entry.successful_msgs = entry.successful_msgs.saturating_add(1);
        entry.last_seen = now;
        entry.score = (entry.score + 0.01).min(1.0);
        if entry.score < self.ban_threshold {
            entry.is_banned = true;
        }
    }

    pub fn record_failure(&self, peer: PeerId, reason: FailureReason) {
        let penalty = match reason {
            FailureReason::InvalidSignature => 0.5,
            FailureReason::InvalidPoIProof => 0.3,
            FailureReason::Timeout => 0.05,
            FailureReason::MalformedMessage => 0.2,
            FailureReason::InvalidModelShard => 0.3,
            FailureReason::ModelTransferTimeout => 0.05,
            FailureReason::ModelHashMismatch => 0.4,
        };

        let now = Instant::now();
        let mut entry = self.peers.entry(peer).or_insert_with(|| PeerReputation {
            peer_id: peer,
            score: 0.5,
            successful_msgs: 0,
            failed_msgs: 0,
            last_seen: now,
            is_banned: false,
        });

        entry.failed_msgs = entry.failed_msgs.saturating_add(1);
        entry.last_seen = now;
        entry.score = (entry.score - penalty).max(0.0);
        if entry.score < self.ban_threshold {
            entry.is_banned = true;
        }
    }

    pub fn is_banned(&self, peer: &PeerId) -> bool {
        self.peers.get(peer).map(|p| p.is_banned).unwrap_or(false)
    }

    pub fn get_top_peers(&self, n: usize) -> Vec<PeerReputation> {
        let mut peers: Vec<_> = self.peers.iter().map(|e| e.value().clone()).collect();
        peers.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
        peers.truncate(n);
        peers
    }

    pub fn cleanup_stale(&self) {
        let cutoff = Instant::now().checked_sub(self.stale_ttl);
        if cutoff.is_none() {
            return;
        }
        let cutoff = cutoff.unwrap();

        let stale: Vec<PeerId> = self
            .peers
            .iter()
            .filter(|p| p.last_seen < cutoff)
            .map(|p| *p.key())
            .collect();

        for peer in stale {
            self.peers.remove(&peer);
        }
    }
}
