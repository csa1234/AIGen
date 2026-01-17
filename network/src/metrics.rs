use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct NetworkMetrics {
    pub peers_connected: AtomicU64,
    pub messages_sent: AtomicU64,
    pub messages_received: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub reputation_bans: AtomicU64,
    pub model_shards_sent: AtomicU64,
    pub model_shards_received: AtomicU64,
    pub model_bytes_sent: AtomicU64,
    pub model_bytes_received: AtomicU64,
    pub model_transfer_failures: AtomicU64,
    pub model_announcements_sent: AtomicU64,
    pub model_announcements_received: AtomicU64,
}

impl NetworkMetrics {
    pub fn inc_peers_connected(&self) {
        self.peers_connected.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_peers_connected(&self) {
        self.peers_connected.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn inc_messages_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_messages_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_bytes_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_bytes_received(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn inc_reputation_bans(&self) {
        self.reputation_bans.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_model_shards_sent(&self) {
        self.model_shards_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_model_shards_received(&self) {
        self.model_shards_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_model_bytes_sent(&self, bytes: u64) {
        self.model_bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_model_bytes_received(&self, bytes: u64) {
        self.model_bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn inc_model_transfer_failures(&self) {
        self.model_transfer_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_model_announcements_sent(&self) {
        self.model_announcements_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_model_announcements_received(&self) {
        self.model_announcements_received.fetch_add(1, Ordering::Relaxed);
    }
}

pub type SharedNetworkMetrics = Arc<NetworkMetrics>;
