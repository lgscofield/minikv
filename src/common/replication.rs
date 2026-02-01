//! Cross-datacenter replication with async replication and conflict resolution.

use crate::common::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;

/// Datacenter identifier
pub type DatacenterId = String;

/// Replication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    /// This datacenter's ID
    pub local_dc: DatacenterId,

    /// Remote datacenters to replicate to
    pub remote_dcs: Vec<RemoteDatacenter>,

    /// Conflict resolution strategy
    #[serde(default)]
    pub conflict_resolution: ConflictResolution,

    /// Enable async replication
    #[serde(default = "default_true")]
    pub async_replication: bool,

    /// Replication batch size
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Replication interval (milliseconds)
    #[serde(default = "default_replication_interval")]
    pub replication_interval_ms: u64,

    /// Maximum replication lag before alerting (seconds)
    #[serde(default = "default_max_lag")]
    pub max_lag_secs: u64,
}

fn default_true() -> bool {
    true
}

fn default_batch_size() -> usize {
    100
}

fn default_replication_interval() -> u64 {
    1000
}

fn default_max_lag() -> u64 {
    60
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            local_dc: "dc1".to_string(),
            remote_dcs: vec![],
            conflict_resolution: ConflictResolution::default(),
            async_replication: true,
            batch_size: 100,
            replication_interval_ms: 1000,
            max_lag_secs: 60,
        }
    }
}

/// Remote datacenter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteDatacenter {
    /// Datacenter ID
    pub id: DatacenterId,

    /// Datacenter name (human-readable)
    pub name: String,

    /// Coordinator endpoints in this DC
    pub endpoints: Vec<String>,

    /// Priority for failover (lower is higher priority)
    #[serde(default)]
    pub priority: u32,

    /// Region/location
    #[serde(default)]
    pub region: String,

    /// Whether this DC is read-only
    #[serde(default)]
    pub read_only: bool,
}

/// Conflict resolution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Last write wins based on timestamp
    #[default]
    LastWriteWins,

    /// Use vector clocks for causality tracking
    VectorClock,

    /// Always prefer local datacenter writes
    LocalFirst,

    /// Always prefer primary datacenter writes
    PrimaryFirst,
}

/// Vector clock for causality tracking
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VectorClock {
    /// Map of datacenter ID to logical timestamp
    pub clocks: HashMap<DatacenterId, u64>,
}

impl VectorClock {
    /// Create a new vector clock
    pub fn new() -> Self {
        Self {
            clocks: HashMap::new(),
        }
    }

    /// Increment the clock for a datacenter
    pub fn increment(&mut self, dc: &DatacenterId) {
        *self.clocks.entry(dc.clone()).or_insert(0) += 1;
    }

    /// Merge with another vector clock (take max of each component)
    pub fn merge(&mut self, other: &VectorClock) {
        for (dc, &ts) in &other.clocks {
            let entry = self.clocks.entry(dc.clone()).or_insert(0);
            *entry = (*entry).max(ts);
        }
    }

    /// Check if this clock is concurrent with another (neither dominates)
    pub fn is_concurrent(&self, other: &VectorClock) -> bool {
        let self_dominates = self.dominates(other);
        let other_dominates = other.dominates(self);
        !self_dominates && !other_dominates
    }

    /// Check if this clock dominates another (happened after)
    pub fn dominates(&self, other: &VectorClock) -> bool {
        let mut dominated = false;
        for (dc, &ts) in &other.clocks {
            let self_ts = self.clocks.get(dc).copied().unwrap_or(0);
            if self_ts < ts {
                return false;
            }
            if self_ts > ts {
                dominated = true;
            }
        }
        dominated
    }

    /// Get the timestamp for a datacenter
    pub fn get(&self, dc: &DatacenterId) -> u64 {
        self.clocks.get(dc).copied().unwrap_or(0)
    }
}

/// Replication event to be sent to remote datacenters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationEvent {
    /// Event ID (UUID)
    pub id: String,

    /// Source datacenter
    pub source_dc: DatacenterId,

    /// Event type
    pub event_type: ReplicationEventType,

    /// Key affected
    pub key: String,

    /// Value (for put operations)
    pub value: Option<Vec<u8>>,

    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// Vector clock (if using vector clocks)
    pub vector_clock: Option<VectorClock>,

    /// Tenant
    pub tenant: Option<String>,
}

/// Type of replication event
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationEventType {
    Put,
    Delete,
    Snapshot,
}

/// Replication status for a remote datacenter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationStatus {
    /// Datacenter ID
    pub dc_id: DatacenterId,

    /// Last replicated event ID
    pub last_replicated_id: Option<String>,

    /// Last replicated timestamp
    pub last_replicated_at: Option<DateTime<Utc>>,

    /// Current replication lag (seconds)
    pub lag_secs: u64,

    /// Events pending replication
    pub pending_events: usize,

    /// Is replication healthy
    pub healthy: bool,

    /// Last error (if any)
    pub last_error: Option<String>,
}

/// Cross-datacenter replication manager
pub struct ReplicationManager {
    /// Configuration
    config: ReplicationConfig,

    /// Pending events queue
    pending_events: Arc<RwLock<Vec<ReplicationEvent>>>,

    /// Replication status per datacenter
    status: Arc<RwLock<HashMap<DatacenterId, ReplicationStatus>>>,

    /// Event sender channel
    event_tx: mpsc::Sender<ReplicationEvent>,

    /// Shutdown signal
    shutdown: Arc<RwLock<bool>>,
}

impl ReplicationManager {
    /// Create a new replication manager
    pub fn new(config: ReplicationConfig) -> (Self, mpsc::Receiver<ReplicationEvent>) {
        let (event_tx, event_rx) = mpsc::channel(10000);

        let mut status = HashMap::new();
        for dc in &config.remote_dcs {
            status.insert(
                dc.id.clone(),
                ReplicationStatus {
                    dc_id: dc.id.clone(),
                    last_replicated_id: None,
                    last_replicated_at: None,
                    lag_secs: 0,
                    pending_events: 0,
                    healthy: true,
                    last_error: None,
                },
            );
        }

        let manager = Self {
            config,
            pending_events: Arc::new(RwLock::new(Vec::new())),
            status: Arc::new(RwLock::new(status)),
            event_tx,
            shutdown: Arc::new(RwLock::new(false)),
        };

        (manager, event_rx)
    }

    /// Queue an event for replication
    pub async fn queue_event(&self, event: ReplicationEvent) -> Result<()> {
        self.event_tx
            .send(event.clone())
            .await
            .map_err(|e| Error::Other(format!("Failed to queue replication event: {}", e)))?;

        let mut pending = self.pending_events.write().unwrap();
        pending.push(event);

        Ok(())
    }

    /// Create a replication event for a PUT operation
    pub fn create_put_event(
        &self,
        key: &str,
        value: Vec<u8>,
        tenant: Option<String>,
    ) -> ReplicationEvent {
        ReplicationEvent {
            id: uuid::Uuid::new_v4().to_string(),
            source_dc: self.config.local_dc.clone(),
            event_type: ReplicationEventType::Put,
            key: key.to_string(),
            value: Some(value),
            timestamp: Utc::now(),
            vector_clock: None,
            tenant,
        }
    }

    /// Create a replication event for a DELETE operation
    pub fn create_delete_event(&self, key: &str, tenant: Option<String>) -> ReplicationEvent {
        ReplicationEvent {
            id: uuid::Uuid::new_v4().to_string(),
            source_dc: self.config.local_dc.clone(),
            event_type: ReplicationEventType::Delete,
            key: key.to_string(),
            value: None,
            timestamp: Utc::now(),
            vector_clock: None,
            tenant,
        }
    }

    /// Get replication status for all datacenters
    pub fn get_status(&self) -> Vec<ReplicationStatus> {
        self.status.read().unwrap().values().cloned().collect()
    }

    /// Get replication status for a specific datacenter
    pub fn get_dc_status(&self, dc_id: &str) -> Option<ReplicationStatus> {
        self.status.read().unwrap().get(dc_id).cloned()
    }

    /// Update replication status for a datacenter
    pub fn update_status(&self, dc_id: &str, update: impl FnOnce(&mut ReplicationStatus)) {
        if let Some(status) = self.status.write().unwrap().get_mut(dc_id) {
            update(status);
        }
    }

    /// Resolve conflicts between two events
    pub fn resolve_conflict<'a>(
        &self,
        local: &'a ReplicationEvent,
        remote: &'a ReplicationEvent,
    ) -> &'a ReplicationEvent {
        match self.config.conflict_resolution {
            ConflictResolution::LastWriteWins => {
                if local.timestamp >= remote.timestamp {
                    local
                } else {
                    remote
                }
            }
            ConflictResolution::VectorClock => {
                if let (Some(local_vc), Some(remote_vc)) =
                    (&local.vector_clock, &remote.vector_clock)
                {
                    if local_vc.dominates(remote_vc) {
                        local
                    } else if remote_vc.dominates(local_vc) {
                        remote
                    } else {
                        // Concurrent - fall back to timestamp
                        if local.timestamp >= remote.timestamp {
                            local
                        } else {
                            remote
                        }
                    }
                } else {
                    // No vector clocks - fall back to timestamp
                    if local.timestamp >= remote.timestamp {
                        local
                    } else {
                        remote
                    }
                }
            }
            ConflictResolution::LocalFirst => local,
            ConflictResolution::PrimaryFirst => {
                // Primary is assumed to be "dc1" by convention
                if local.source_dc == "dc1" {
                    local
                } else if remote.source_dc == "dc1" {
                    remote
                } else {
                    // Neither is primary - fall back to timestamp
                    if local.timestamp >= remote.timestamp {
                        local
                    } else {
                        remote
                    }
                }
            }
        }
    }

    /// Get configuration
    pub fn config(&self) -> &ReplicationConfig {
        &self.config
    }

    /// Check if replication is healthy (all DCs within max lag)
    pub fn is_healthy(&self) -> bool {
        self.status
            .read()
            .unwrap()
            .values()
            .all(|s| s.healthy && s.lag_secs <= self.config.max_lag_secs)
    }

    /// Shutdown the replication manager
    pub fn shutdown(&self) {
        *self.shutdown.write().unwrap() = true;
    }
}

/// Global replication manager instance
pub static REPLICATION_MANAGER: once_cell::sync::Lazy<RwLock<Option<ReplicationManager>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(None));

/// Initialize the global replication manager
pub fn init_replication(config: ReplicationConfig) -> mpsc::Receiver<ReplicationEvent> {
    let (manager, rx) = ReplicationManager::new(config);
    *REPLICATION_MANAGER.write().unwrap() = Some(manager);
    rx
}

/// Get the global replication manager
pub fn get_replication_manager(
) -> Option<std::sync::RwLockReadGuard<'static, Option<ReplicationManager>>> {
    let guard = REPLICATION_MANAGER.read().unwrap();
    if guard.is_some() {
        Some(guard)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_clock_increment() {
        let mut vc = VectorClock::new();
        vc.increment(&"dc1".to_string());
        vc.increment(&"dc1".to_string());
        vc.increment(&"dc2".to_string());

        assert_eq!(vc.get(&"dc1".to_string()), 2);
        assert_eq!(vc.get(&"dc2".to_string()), 1);
        assert_eq!(vc.get(&"dc3".to_string()), 0);
    }

    #[test]
    fn test_vector_clock_merge() {
        let mut vc1 = VectorClock::new();
        vc1.clocks.insert("dc1".to_string(), 3);
        vc1.clocks.insert("dc2".to_string(), 1);

        let mut vc2 = VectorClock::new();
        vc2.clocks.insert("dc1".to_string(), 2);
        vc2.clocks.insert("dc2".to_string(), 4);
        vc2.clocks.insert("dc3".to_string(), 1);

        vc1.merge(&vc2);

        assert_eq!(vc1.get(&"dc1".to_string()), 3);
        assert_eq!(vc1.get(&"dc2".to_string()), 4);
        assert_eq!(vc1.get(&"dc3".to_string()), 1);
    }

    #[test]
    fn test_vector_clock_dominates() {
        let mut vc1 = VectorClock::new();
        vc1.clocks.insert("dc1".to_string(), 3);
        vc1.clocks.insert("dc2".to_string(), 2);

        let mut vc2 = VectorClock::new();
        vc2.clocks.insert("dc1".to_string(), 2);
        vc2.clocks.insert("dc2".to_string(), 1);

        assert!(vc1.dominates(&vc2));
        assert!(!vc2.dominates(&vc1));
    }

    #[test]
    fn test_vector_clock_concurrent() {
        let mut vc1 = VectorClock::new();
        vc1.clocks.insert("dc1".to_string(), 3);
        vc1.clocks.insert("dc2".to_string(), 1);

        let mut vc2 = VectorClock::new();
        vc2.clocks.insert("dc1".to_string(), 2);
        vc2.clocks.insert("dc2".to_string(), 4);

        assert!(vc1.is_concurrent(&vc2));
        assert!(vc2.is_concurrent(&vc1));
    }
}
