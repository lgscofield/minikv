//! Data tiering (hot/warm/cold/archive).

use crate::common::{Error, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

// ============================================================================
// Configuration
// ============================================================================

/// Data tiering configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieringConfig {
    /// Enable data tiering
    #[serde(default)]
    pub enabled: bool,

    /// Hot tier configuration
    #[serde(default)]
    pub hot: TierConfig,

    /// Warm tier configuration
    #[serde(default)]
    pub warm: TierConfig,

    /// Cold tier configuration
    #[serde(default)]
    pub cold: TierConfig,

    /// Archive tier configuration
    pub archive: Option<ArchiveConfig>,

    /// Tiering policies
    #[serde(default)]
    pub policies: Vec<TieringPolicy>,

    /// Background tiering interval in seconds
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
}

fn default_interval() -> u64 {
    3600 // 1 hour
}

impl Default for TieringConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            hot: TierConfig::default_hot(),
            warm: TierConfig::default_warm(),
            cold: TierConfig::default_cold(),
            archive: None,
            policies: vec![],
            interval_secs: default_interval(),
        }
    }
}

/// Tier configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    /// Tier name
    pub name: String,

    /// Storage path
    pub path: Option<PathBuf>,

    /// Maximum size in bytes (0 = unlimited)
    #[serde(default)]
    pub max_size_bytes: u64,

    /// Maximum number of items (0 = unlimited)
    #[serde(default)]
    pub max_items: u64,

    /// Compression enabled
    #[serde(default)]
    pub compression: bool,

    /// Compression algorithm
    #[serde(default)]
    pub compression_algorithm: CompressionAlgorithm,

    /// Enable in-memory cache
    #[serde(default)]
    pub cache_enabled: bool,

    /// Cache size in bytes
    #[serde(default)]
    pub cache_size_bytes: u64,
}

impl TierConfig {
    fn default_hot() -> Self {
        Self {
            name: "hot".to_string(),
            path: None,
            max_size_bytes: 1024 * 1024 * 1024, // 1 GB
            max_items: 0,
            compression: false,
            compression_algorithm: CompressionAlgorithm::None,
            cache_enabled: true,
            cache_size_bytes: 256 * 1024 * 1024, // 256 MB
        }
    }

    fn default_warm() -> Self {
        Self {
            name: "warm".to_string(),
            path: None,
            max_size_bytes: 10 * 1024 * 1024 * 1024, // 10 GB
            max_items: 0,
            compression: true,
            compression_algorithm: CompressionAlgorithm::Lz4,
            cache_enabled: false,
            cache_size_bytes: 0,
        }
    }

    fn default_cold() -> Self {
        Self {
            name: "cold".to_string(),
            path: None,
            max_size_bytes: 100 * 1024 * 1024 * 1024, // 100 GB
            max_items: 0,
            compression: true,
            compression_algorithm: CompressionAlgorithm::Zstd,
            cache_enabled: false,
            cache_size_bytes: 0,
        }
    }
}

impl Default for TierConfig {
    fn default() -> Self {
        Self::default_hot()
    }
}

/// Archive tier configuration (S3-compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveConfig {
    /// S3-compatible endpoint
    pub endpoint: String,

    /// Bucket name
    pub bucket: String,

    /// Access key
    pub access_key: Option<String>,

    /// Secret key
    pub secret_key: Option<String>,

    /// Region
    pub region: Option<String>,

    /// Compression
    #[serde(default = "default_true")]
    pub compression: bool,

    /// Storage class (STANDARD, GLACIER, DEEP_ARCHIVE)
    #[serde(default = "default_storage_class")]
    pub storage_class: String,
}

fn default_true() -> bool {
    true
}

fn default_storage_class() -> String {
    "STANDARD".to_string()
}

/// Compression algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CompressionAlgorithm {
    #[default]
    None,
    Lz4,
    Zstd,
    Snappy,
    Gzip,
}

// ============================================================================
// Tiering Policy
// ============================================================================

/// Tiering policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieringPolicy {
    /// Policy name
    pub name: String,

    /// Key prefix this policy applies to
    pub prefix: Option<String>,

    /// Rules for this policy
    pub rules: Vec<TieringRule>,

    /// Priority (higher = evaluated first)
    #[serde(default)]
    pub priority: i32,

    /// Is policy enabled?
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Tiering rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieringRule {
    /// Condition for this rule
    pub condition: TieringCondition,

    /// Target tier
    pub target_tier: Tier,
}

/// Tier type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Hot,
    Warm,
    Cold,
    Archive,
}

impl Tier {
    pub fn temperature(&self) -> i32 {
        match self {
            Tier::Hot => 3,
            Tier::Warm => 2,
            Tier::Cold => 1,
            Tier::Archive => 0,
        }
    }
}

/// Tiering condition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TieringCondition {
    /// Access count in time window
    AccessCount {
        /// Minimum access count
        min_count: Option<u64>,
        /// Maximum access count
        max_count: Option<u64>,
        /// Time window in seconds
        window_secs: u64,
    },

    /// Age of data
    Age {
        /// Minimum age in seconds
        min_age_secs: Option<u64>,
        /// Maximum age in seconds
        max_age_secs: Option<u64>,
    },

    /// Size of value
    Size {
        /// Minimum size in bytes
        min_bytes: Option<u64>,
        /// Maximum size in bytes
        max_bytes: Option<u64>,
    },

    /// Last access time
    LastAccess {
        /// Minimum time since last access in seconds
        min_since_access_secs: Option<u64>,
        /// Maximum time since last access in seconds
        max_since_access_secs: Option<u64>,
    },

    /// Combination of conditions (AND)
    And { conditions: Vec<TieringCondition> },

    /// Combination of conditions (OR)
    Or { conditions: Vec<TieringCondition> },
}

// ============================================================================
// Data Tracking
// ============================================================================

/// Metadata for a stored item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemMetadata {
    /// Key
    pub key: String,

    /// Current tier
    pub tier: Tier,

    /// Size in bytes
    pub size_bytes: u64,

    /// Creation time
    pub created_at: DateTime<Utc>,

    /// Last modified time
    pub modified_at: DateTime<Utc>,

    /// Last access time
    pub last_accessed_at: DateTime<Utc>,

    /// Access count
    pub access_count: u64,

    /// Access history (timestamps)
    #[serde(default)]
    pub access_history: Vec<DateTime<Utc>>,

    /// Is compressed?
    pub compressed: bool,

    /// Original size (before compression)
    pub original_size_bytes: u64,
}

impl ItemMetadata {
    pub fn new(key: String, size_bytes: u64, tier: Tier) -> Self {
        let now = Utc::now();
        Self {
            key,
            tier,
            size_bytes,
            created_at: now,
            modified_at: now,
            last_accessed_at: now,
            access_count: 0,
            access_history: vec![],
            compressed: false,
            original_size_bytes: size_bytes,
        }
    }

    /// Record an access
    pub fn record_access(&mut self) {
        let now = Utc::now();
        self.last_accessed_at = now;
        self.access_count += 1;

        // Keep last 100 accesses
        self.access_history.push(now);
        if self.access_history.len() > 100 {
            self.access_history.remove(0);
        }
    }

    /// Get access count in a time window
    pub fn access_count_in_window(&self, window: Duration) -> u64 {
        let cutoff = Utc::now() - window;
        self.access_history.iter().filter(|&t| *t >= cutoff).count() as u64
    }

    /// Get age in seconds
    pub fn age_secs(&self) -> u64 {
        (Utc::now() - self.created_at).num_seconds().max(0) as u64
    }

    /// Get time since last access in seconds
    pub fn since_last_access_secs(&self) -> u64 {
        (Utc::now() - self.last_accessed_at).num_seconds().max(0) as u64
    }
}

// ============================================================================
// Tiering Manager
// ============================================================================

/// Tiering manager
pub struct TieringManager {
    config: TieringConfig,

    /// Item metadata
    metadata: Arc<RwLock<HashMap<String, ItemMetadata>>>,

    /// Tier statistics
    stats: Arc<RwLock<TierStats>>,

    /// Pending tier changes
    pending_changes: Arc<RwLock<Vec<TierChange>>>,
}

/// Tier change request
#[derive(Debug, Clone)]
pub struct TierChange {
    pub key: String,
    pub from_tier: Tier,
    pub to_tier: Tier,
    pub reason: String,
}

/// Tier statistics
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TierStats {
    pub hot: TierStat,
    pub warm: TierStat,
    pub cold: TierStat,
    pub archive: TierStat,

    /// Total promotions
    pub total_promotions: u64,

    /// Total demotions
    pub total_demotions: u64,

    /// Bytes moved
    pub bytes_moved: u64,
}

/// Statistics for a single tier
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TierStat {
    /// Number of items
    pub item_count: u64,

    /// Total size in bytes
    pub size_bytes: u64,

    /// Read operations
    pub reads: u64,

    /// Write operations
    pub writes: u64,

    /// Cache hits (for hot tier)
    pub cache_hits: u64,

    /// Cache misses (for hot tier)
    pub cache_misses: u64,
}

impl TieringManager {
    /// Create a new tiering manager
    pub fn new(config: TieringConfig) -> Self {
        Self {
            config,
            metadata: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(TierStats::default())),
            pending_changes: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Track a new item
    pub fn track_item(&self, key: &str, size_bytes: u64, tier: Tier) {
        let metadata = ItemMetadata::new(key.to_string(), size_bytes, tier);

        let mut items = self.metadata.write().unwrap();
        items.insert(key.to_string(), metadata);

        let mut stats = self.stats.write().unwrap();
        let stat = self.get_tier_stat_mut(&mut stats, tier);
        stat.item_count += 1;
        stat.size_bytes += size_bytes;
        stat.writes += 1;
    }

    /// Record an access
    pub fn record_access(&self, key: &str) -> Option<Tier> {
        let mut items = self.metadata.write().unwrap();
        if let Some(metadata) = items.get_mut(key) {
            metadata.record_access();
            let tier = metadata.tier;

            let mut stats = self.stats.write().unwrap();
            self.get_tier_stat_mut(&mut stats, tier).reads += 1;

            return Some(tier);
        }
        None
    }

    /// Remove tracking for an item
    pub fn untrack_item(&self, key: &str) {
        let mut items = self.metadata.write().unwrap();
        if let Some(metadata) = items.remove(key) {
            let mut stats = self.stats.write().unwrap();
            let stat = self.get_tier_stat_mut(&mut stats, metadata.tier);
            stat.item_count = stat.item_count.saturating_sub(1);
            stat.size_bytes = stat.size_bytes.saturating_sub(metadata.size_bytes);
        }
    }

    /// Get item metadata
    pub fn get_metadata(&self, key: &str) -> Option<ItemMetadata> {
        let items = self.metadata.read().unwrap();
        items.get(key).cloned()
    }

    /// Get current tier for an item
    pub fn get_tier(&self, key: &str) -> Option<Tier> {
        let items = self.metadata.read().unwrap();
        items.get(key).map(|m| m.tier)
    }

    /// Evaluate tiering policies and generate tier change recommendations
    pub fn evaluate_policies(&self) -> Vec<TierChange> {
        let items = self.metadata.read().unwrap();
        let mut changes = Vec::new();

        // Sort policies by priority
        let mut policies = self.config.policies.clone();
        policies.sort_by(|a, b| b.priority.cmp(&a.priority));

        for (key, metadata) in items.iter() {
            // Find first matching policy
            for policy in &policies {
                if !policy.enabled {
                    continue;
                }

                // Check prefix
                if let Some(ref prefix) = policy.prefix {
                    if !key.starts_with(prefix) {
                        continue;
                    }
                }

                // Evaluate rules
                for rule in &policy.rules {
                    if self.evaluate_condition(&rule.condition, metadata) {
                        if rule.target_tier != metadata.tier {
                            changes.push(TierChange {
                                key: key.clone(),
                                from_tier: metadata.tier,
                                to_tier: rule.target_tier,
                                reason: format!("Policy: {}", policy.name),
                            });
                        }
                        break;
                    }
                }
            }
        }

        // Store pending changes
        let mut pending = self.pending_changes.write().unwrap();
        *pending = changes.clone();

        changes
    }

    /// Evaluate a tiering condition
    #[allow(clippy::only_used_in_recursion)]
    fn evaluate_condition(&self, condition: &TieringCondition, metadata: &ItemMetadata) -> bool {
        match condition {
            TieringCondition::AccessCount {
                min_count,
                max_count,
                window_secs,
            } => {
                let count = metadata.access_count_in_window(Duration::seconds(*window_secs as i64));

                if let Some(min) = min_count {
                    if count < *min {
                        return false;
                    }
                }
                if let Some(max) = max_count {
                    if count > *max {
                        return false;
                    }
                }
                true
            }

            TieringCondition::Age {
                min_age_secs,
                max_age_secs,
            } => {
                let age = metadata.age_secs();

                if let Some(min) = min_age_secs {
                    if age < *min {
                        return false;
                    }
                }
                if let Some(max) = max_age_secs {
                    if age > *max {
                        return false;
                    }
                }
                true
            }

            TieringCondition::Size {
                min_bytes,
                max_bytes,
            } => {
                if let Some(min) = min_bytes {
                    if metadata.size_bytes < *min {
                        return false;
                    }
                }
                if let Some(max) = max_bytes {
                    if metadata.size_bytes > *max {
                        return false;
                    }
                }
                true
            }

            TieringCondition::LastAccess {
                min_since_access_secs,
                max_since_access_secs,
            } => {
                let since = metadata.since_last_access_secs();

                if let Some(min) = min_since_access_secs {
                    if since < *min {
                        return false;
                    }
                }
                if let Some(max) = max_since_access_secs {
                    if since > *max {
                        return false;
                    }
                }
                true
            }

            TieringCondition::And { conditions } => conditions
                .iter()
                .all(|c| self.evaluate_condition(c, metadata)),

            TieringCondition::Or { conditions } => conditions
                .iter()
                .any(|c| self.evaluate_condition(c, metadata)),
        }
    }

    /// Apply a tier change
    pub fn apply_change(&self, change: &TierChange) -> Result<()> {
        let mut items = self.metadata.write().unwrap();
        if let Some(metadata) = items.get_mut(&change.key) {
            let size = metadata.size_bytes;

            // Update stats
            let mut stats = self.stats.write().unwrap();

            let from_stat = self.get_tier_stat_mut(&mut stats, change.from_tier);
            from_stat.item_count = from_stat.item_count.saturating_sub(1);
            from_stat.size_bytes = from_stat.size_bytes.saturating_sub(size);

            let to_stat = self.get_tier_stat_mut(&mut stats, change.to_tier);
            to_stat.item_count += 1;
            to_stat.size_bytes += size;

            stats.bytes_moved += size;

            if change.to_tier.temperature() > change.from_tier.temperature() {
                stats.total_promotions += 1;
            } else {
                stats.total_demotions += 1;
            }

            // Update metadata
            metadata.tier = change.to_tier;

            Ok(())
        } else {
            Err(Error::Internal(format!("Key not found: {}", change.key)))
        }
    }

    /// Get tier statistics
    pub fn get_stats(&self) -> TierStats {
        self.stats.read().unwrap().clone()
    }

    /// Get pending tier changes
    pub fn get_pending_changes(&self) -> Vec<TierChange> {
        self.pending_changes.read().unwrap().clone()
    }

    /// Get items in a specific tier
    pub fn items_in_tier(&self, tier: Tier) -> Vec<ItemMetadata> {
        let items = self.metadata.read().unwrap();
        items.values().filter(|m| m.tier == tier).cloned().collect()
    }

    /// Get hottest items (most frequently accessed)
    pub fn hottest_items(&self, limit: usize) -> Vec<ItemMetadata> {
        let items = self.metadata.read().unwrap();
        let mut sorted: Vec<_> = items.values().cloned().collect();
        sorted.sort_by(|a, b| b.access_count.cmp(&a.access_count));
        sorted.truncate(limit);
        sorted
    }

    /// Get coldest items (least recently accessed)
    pub fn coldest_items(&self, limit: usize) -> Vec<ItemMetadata> {
        let items = self.metadata.read().unwrap();
        let mut sorted: Vec<_> = items.values().cloned().collect();
        sorted.sort_by(|a, b| a.last_accessed_at.cmp(&b.last_accessed_at));
        sorted.truncate(limit);
        sorted
    }

    fn get_tier_stat_mut<'a>(&self, stats: &'a mut TierStats, tier: Tier) -> &'a mut TierStat {
        match tier {
            Tier::Hot => &mut stats.hot,
            Tier::Warm => &mut stats.warm,
            Tier::Cold => &mut stats.cold,
            Tier::Archive => &mut stats.archive,
        }
    }
}

// ============================================================================
// Compression Helpers
// ============================================================================

/// Compress data
pub fn compress(data: &[u8], algorithm: CompressionAlgorithm) -> Result<Vec<u8>> {
    match algorithm {
        CompressionAlgorithm::None => Ok(data.to_vec()),
        CompressionAlgorithm::Lz4 => {
            // Placeholder - would use lz4 crate
            Ok(data.to_vec())
        }
        CompressionAlgorithm::Zstd => {
            // Placeholder - would use zstd crate
            Ok(data.to_vec())
        }
        CompressionAlgorithm::Snappy => {
            // Placeholder - would use snap crate
            Ok(data.to_vec())
        }
        CompressionAlgorithm::Gzip => {
            // Placeholder - would use flate2 crate
            Ok(data.to_vec())
        }
    }
}

/// Decompress data
pub fn decompress(data: &[u8], algorithm: CompressionAlgorithm) -> Result<Vec<u8>> {
    match algorithm {
        CompressionAlgorithm::None => Ok(data.to_vec()),
        CompressionAlgorithm::Lz4 => {
            // Placeholder
            Ok(data.to_vec())
        }
        CompressionAlgorithm::Zstd => {
            // Placeholder
            Ok(data.to_vec())
        }
        CompressionAlgorithm::Snappy => {
            // Placeholder
            Ok(data.to_vec())
        }
        CompressionAlgorithm::Gzip => {
            // Placeholder
            Ok(data.to_vec())
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> TieringConfig {
        TieringConfig {
            enabled: true,
            policies: vec![
                TieringPolicy {
                    name: "hot-access".to_string(),
                    prefix: None,
                    priority: 10,
                    enabled: true,
                    rules: vec![TieringRule {
                        condition: TieringCondition::AccessCount {
                            min_count: Some(100),
                            max_count: None,
                            window_secs: 3600,
                        },
                        target_tier: Tier::Hot,
                    }],
                },
                TieringPolicy {
                    name: "cold-age".to_string(),
                    prefix: None,
                    priority: 5,
                    enabled: true,
                    rules: vec![TieringRule {
                        condition: TieringCondition::Age {
                            min_age_secs: Some(86400 * 30), // 30 days
                            max_age_secs: None,
                        },
                        target_tier: Tier::Cold,
                    }],
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn test_track_item() {
        let manager = TieringManager::new(sample_config());

        manager.track_item("key1", 1000, Tier::Hot);
        manager.track_item("key2", 2000, Tier::Warm);

        let stats = manager.get_stats();
        assert_eq!(stats.hot.item_count, 1);
        assert_eq!(stats.hot.size_bytes, 1000);
        assert_eq!(stats.warm.item_count, 1);
        assert_eq!(stats.warm.size_bytes, 2000);
    }

    #[test]
    fn test_record_access() {
        let manager = TieringManager::new(sample_config());

        manager.track_item("key1", 1000, Tier::Hot);

        for _ in 0..10 {
            manager.record_access("key1");
        }

        let metadata = manager.get_metadata("key1").unwrap();
        assert_eq!(metadata.access_count, 10);
    }

    #[test]
    fn test_tier_change() {
        let manager = TieringManager::new(sample_config());

        manager.track_item("key1", 1000, Tier::Hot);

        let change = TierChange {
            key: "key1".to_string(),
            from_tier: Tier::Hot,
            to_tier: Tier::Warm,
            reason: "Test".to_string(),
        };

        manager.apply_change(&change).unwrap();

        let metadata = manager.get_metadata("key1").unwrap();
        assert_eq!(metadata.tier, Tier::Warm);

        let stats = manager.get_stats();
        assert_eq!(stats.hot.item_count, 0);
        assert_eq!(stats.warm.item_count, 1);
        assert_eq!(stats.total_demotions, 1);
    }

    #[test]
    fn test_hottest_items() {
        let manager = TieringManager::new(sample_config());

        manager.track_item("key1", 100, Tier::Hot);
        manager.track_item("key2", 100, Tier::Hot);
        manager.track_item("key3", 100, Tier::Hot);

        for _ in 0..50 {
            manager.record_access("key1");
        }
        for _ in 0..100 {
            manager.record_access("key2");
        }
        for _ in 0..10 {
            manager.record_access("key3");
        }

        let hottest = manager.hottest_items(2);
        assert_eq!(hottest.len(), 2);
        assert_eq!(hottest[0].key, "key2");
        assert_eq!(hottest[1].key, "key1");
    }
}
