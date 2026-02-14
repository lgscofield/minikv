//! Time-series storage engine.

use crate::common::{Error, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

// ============================================================================
// Configuration
// ============================================================================

/// Time-series configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeseriesConfig {
    /// Enable time-series optimizations
    #[serde(default)]
    pub enabled: bool,

    /// Data retention in days
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,

    /// Downsampling rules
    #[serde(default)]
    pub downsample_rules: Vec<DownsampleRule>,

    /// Compression settings
    #[serde(default)]
    pub compression: CompressionConfig,

    /// Maximum points per query
    #[serde(default = "default_max_points")]
    pub max_points_per_query: usize,

    /// Background job interval (seconds)
    #[serde(default = "default_job_interval")]
    pub job_interval_secs: u64,
}

fn default_retention_days() -> u32 {
    30
}

fn default_max_points() -> usize {
    10000
}

fn default_job_interval() -> u64 {
    3600
}

impl Default for TimeseriesConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            retention_days: 30,
            downsample_rules: vec![
                DownsampleRule {
                    after: Duration::days(7),
                    resolution: Resolution::Hour,
                    aggregation: Aggregation::Average,
                },
                DownsampleRule {
                    after: Duration::days(30),
                    resolution: Resolution::Day,
                    aggregation: Aggregation::Average,
                },
            ],
            compression: CompressionConfig::default(),
            max_points_per_query: 10000,
            job_interval_secs: 3600,
        }
    }
}

/// Downsampling rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownsampleRule {
    /// Apply after this duration
    #[serde(with = "duration_serde")]
    pub after: Duration,

    /// Target resolution
    pub resolution: Resolution,

    /// Aggregation function
    pub aggregation: Aggregation,
}

mod duration_serde {
    use chrono::Duration;
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(duration.num_seconds())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = i64::deserialize(deserializer)?;
        Ok(Duration::seconds(secs))
    }
}

/// Time resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Resolution {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
}

impl Resolution {
    /// Get duration in milliseconds
    pub fn as_millis(&self) -> i64 {
        match self {
            Resolution::Second => 1_000,
            Resolution::Minute => 60_000,
            Resolution::Hour => 3_600_000,
            Resolution::Day => 86_400_000,
            Resolution::Week => 604_800_000,
            Resolution::Month => 2_592_000_000, // ~30 days
        }
    }

    /// Align timestamp to resolution boundary
    pub fn align(&self, ts: i64) -> i64 {
        let millis = self.as_millis();
        (ts / millis) * millis
    }
}

/// Aggregation function
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Aggregation {
    Average,
    Sum,
    Min,
    Max,
    Count,
    First,
    Last,
}

/// Compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Enable delta encoding for timestamps
    #[serde(default = "default_true")]
    pub delta_encoding: bool,

    /// Enable run-length encoding for repeated values
    #[serde(default = "default_true")]
    pub run_length_encoding: bool,

    /// Enable Gorilla compression for floats
    #[serde(default = "default_true")]
    pub gorilla_compression: bool,
}

fn default_true() -> bool {
    true
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            delta_encoding: true,
            run_length_encoding: true,
            gorilla_compression: true,
        }
    }
}

// ============================================================================
// Data Types
// ============================================================================

/// A single data point
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DataPoint {
    /// Timestamp in milliseconds since epoch
    pub timestamp: i64,
    /// Value
    pub value: f64,
}

impl DataPoint {
    pub fn new(timestamp: i64, value: f64) -> Self {
        Self { timestamp, value }
    }

    pub fn now(value: f64) -> Self {
        Self {
            timestamp: Utc::now().timestamp_millis(),
            value,
        }
    }
}

/// A time series with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeries {
    /// Metric name
    pub metric: String,

    /// Tags for filtering
    #[serde(default)]
    pub tags: HashMap<String, String>,

    /// Data points
    pub points: Vec<DataPoint>,
}

impl TimeSeries {
    pub fn new(metric: &str) -> Self {
        Self {
            metric: metric.to_string(),
            tags: HashMap::new(),
            points: Vec::new(),
        }
    }

    pub fn with_tag(mut self, key: &str, value: &str) -> Self {
        self.tags.insert(key.to_string(), value.to_string());
        self
    }

    pub fn add_point(&mut self, point: DataPoint) {
        self.points.push(point);
    }
}

/// Query for time-series data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeseriesQuery {
    /// Metric name pattern (supports wildcards)
    pub metric: String,

    /// Start time (inclusive)
    pub start: DateTime<Utc>,

    /// End time (exclusive)
    pub end: DateTime<Utc>,

    /// Tag filters
    #[serde(default)]
    pub tags: HashMap<String, String>,

    /// Aggregation function
    #[serde(default)]
    pub aggregation: Option<Aggregation>,

    /// Group by resolution
    #[serde(default)]
    pub resolution: Option<Resolution>,

    /// Maximum number of points
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeseriesResult {
    /// Matching series
    pub series: Vec<TimeSeries>,

    /// Query execution time in milliseconds
    pub execution_time_ms: u64,

    /// Number of points scanned
    pub points_scanned: u64,

    /// Number of points returned
    pub points_returned: u64,
}

// ============================================================================
// Storage Engine
// ============================================================================

/// Time-series storage engine
pub struct TimeseriesEngine {
    config: TimeseriesConfig,
    /// Storage: metric -> timestamp_bucket -> compressed data
    data: Arc<RwLock<BTreeMap<String, BTreeMap<i64, CompressedBlock>>>>,
    /// Index: metric+tags -> series ID
    index: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

/// Compressed data block
#[derive(Debug, Clone)]
pub struct CompressedBlock {
    /// Start timestamp
    pub start_ts: i64,
    /// End timestamp
    pub end_ts: i64,
    /// Number of points
    pub count: usize,
    /// Compressed data
    pub data: Vec<u8>,
    /// Min value in block
    pub min: f64,
    /// Max value in block
    pub max: f64,
    /// Sum of values in block
    pub sum: f64,
}

impl TimeseriesEngine {
    /// Create a new time-series engine
    pub fn new(config: TimeseriesConfig) -> Self {
        Self {
            config,
            data: Arc::new(RwLock::new(BTreeMap::new())),
            index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Write data points
    pub fn write(&self, series: &TimeSeries) -> Result<()> {
        if series.points.is_empty() {
            return Ok(());
        }

        let series_key = self.series_key(&series.metric, &series.tags);

        // Update index
        {
            let mut index = self.index.write().unwrap();
            index
                .entry(series.metric.clone())
                .or_default()
                .push(series_key.clone());
        }

        // Compress and store points
        let compressed = self.compress_points(&series.points)?;

        let mut data = self.data.write().unwrap();
        let metric_data = data.entry(series_key).or_default();

        // Store in time bucket (hourly buckets)
        let bucket_ts = Resolution::Hour.align(series.points[0].timestamp);
        metric_data.insert(bucket_ts, compressed);

        Ok(())
    }

    /// Query data points
    pub fn query(&self, query: &TimeseriesQuery) -> Result<TimeseriesResult> {
        let start = std::time::Instant::now();
        let start_ts = query.start.timestamp_millis();
        let end_ts = query.end.timestamp_millis();

        let data = self.data.read().unwrap();
        let mut results = Vec::new();
        let mut points_scanned = 0u64;
        let mut points_returned = 0u64;

        // Find matching series
        for (series_key, buckets) in data.iter() {
            if !self.matches_metric(series_key, &query.metric) {
                continue;
            }

            // Check tags
            if !self.matches_tags(series_key, &query.tags) {
                continue;
            }

            let mut points = Vec::new();

            // Scan relevant buckets
            for (_bucket_ts, block) in buckets.range(start_ts..=end_ts) {
                points_scanned += block.count as u64;

                // Decompress and filter points
                let block_points = self.decompress_block(block)?;
                for point in block_points {
                    if point.timestamp >= start_ts && point.timestamp < end_ts {
                        points.push(point);
                    }
                }
            }

            // Apply aggregation if requested
            if let (Some(agg), Some(res)) = (&query.aggregation, &query.resolution) {
                points = self.aggregate_points(&points, *agg, *res);
            }

            // Apply limit
            if let Some(limit) = query.limit {
                points.truncate(limit);
            }

            points_returned += points.len() as u64;

            if !points.is_empty() {
                let metric = series_key.split('|').next().unwrap_or(series_key);
                results.push(TimeSeries {
                    metric: metric.to_string(),
                    tags: self.extract_tags(series_key),
                    points,
                });
            }
        }

        let execution_time_ms = start.elapsed().as_millis() as u64;

        Ok(TimeseriesResult {
            series: results,
            execution_time_ms,
            points_scanned,
            points_returned,
        })
    }

    /// Compress data points
    fn compress_points(&self, points: &[DataPoint]) -> Result<CompressedBlock> {
        if points.is_empty() {
            return Err(Error::Internal("No points to compress".to_string()));
        }

        let start_ts = points.first().unwrap().timestamp;
        let end_ts = points.last().unwrap().timestamp;
        let count = points.len();

        // Calculate stats
        let mut min = f64::MAX;
        let mut max = f64::MIN;
        let mut sum = 0.0;

        for p in points {
            min = min.min(p.value);
            max = max.max(p.value);
            sum += p.value;
        }

        // Compress using delta encoding + simple compression
        let mut data = Vec::new();

        if self.config.compression.delta_encoding {
            // Delta encode timestamps
            let mut prev_ts = start_ts;
            for point in points {
                let delta = point.timestamp - prev_ts;
                prev_ts = point.timestamp;

                // Varint encoding for delta
                data.extend_from_slice(&(delta as i32).to_le_bytes());
                data.extend_from_slice(&point.value.to_le_bytes());
            }
        } else {
            // Raw encoding
            for point in points {
                data.extend_from_slice(&point.timestamp.to_le_bytes());
                data.extend_from_slice(&point.value.to_le_bytes());
            }
        }

        Ok(CompressedBlock {
            start_ts,
            end_ts,
            count,
            data,
            min,
            max,
            sum,
        })
    }

    /// Decompress a data block
    fn decompress_block(&self, block: &CompressedBlock) -> Result<Vec<DataPoint>> {
        let mut points = Vec::with_capacity(block.count);

        if self.config.compression.delta_encoding {
            let mut pos = 0;
            let mut ts = block.start_ts;

            while pos + 12 <= block.data.len() {
                let delta = i32::from_le_bytes([
                    block.data[pos],
                    block.data[pos + 1],
                    block.data[pos + 2],
                    block.data[pos + 3],
                ]) as i64;

                ts += delta;

                let value = f64::from_le_bytes([
                    block.data[pos + 4],
                    block.data[pos + 5],
                    block.data[pos + 6],
                    block.data[pos + 7],
                    block.data[pos + 8],
                    block.data[pos + 9],
                    block.data[pos + 10],
                    block.data[pos + 11],
                ]);

                points.push(DataPoint {
                    timestamp: ts,
                    value,
                });

                pos += 12;
            }
        } else {
            let mut pos = 0;
            while pos + 16 <= block.data.len() {
                let ts = i64::from_le_bytes([
                    block.data[pos],
                    block.data[pos + 1],
                    block.data[pos + 2],
                    block.data[pos + 3],
                    block.data[pos + 4],
                    block.data[pos + 5],
                    block.data[pos + 6],
                    block.data[pos + 7],
                ]);

                let value = f64::from_le_bytes([
                    block.data[pos + 8],
                    block.data[pos + 9],
                    block.data[pos + 10],
                    block.data[pos + 11],
                    block.data[pos + 12],
                    block.data[pos + 13],
                    block.data[pos + 14],
                    block.data[pos + 15],
                ]);

                points.push(DataPoint {
                    timestamp: ts,
                    value,
                });

                pos += 16;
            }
        }

        Ok(points)
    }

    /// Aggregate points by resolution
    fn aggregate_points(
        &self,
        points: &[DataPoint],
        agg: Aggregation,
        resolution: Resolution,
    ) -> Vec<DataPoint> {
        if points.is_empty() {
            return Vec::new();
        }

        // Group by resolution bucket
        let mut buckets: BTreeMap<i64, Vec<f64>> = BTreeMap::new();

        for point in points {
            let bucket = resolution.align(point.timestamp);
            buckets.entry(bucket).or_default().push(point.value);
        }

        // Aggregate each bucket
        buckets
            .into_iter()
            .map(|(ts, values)| {
                let value = match agg {
                    Aggregation::Average => values.iter().sum::<f64>() / values.len() as f64,
                    Aggregation::Sum => values.iter().sum(),
                    Aggregation::Min => values.iter().cloned().fold(f64::MAX, f64::min),
                    Aggregation::Max => values.iter().cloned().fold(f64::MIN, f64::max),
                    Aggregation::Count => values.len() as f64,
                    Aggregation::First => values.first().copied().unwrap_or(0.0),
                    Aggregation::Last => values.last().copied().unwrap_or(0.0),
                };
                DataPoint {
                    timestamp: ts,
                    value,
                }
            })
            .collect()
    }

    /// Generate series key from metric and tags
    fn series_key(&self, metric: &str, tags: &HashMap<String, String>) -> String {
        if tags.is_empty() {
            return metric.to_string();
        }

        let mut sorted_tags: Vec<_> = tags.iter().collect();
        sorted_tags.sort_by_key(|(k, _)| *k);

        let tags_str: Vec<String> = sorted_tags
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        format!("{}|{}", metric, tags_str.join(","))
    }

    /// Check if series key matches metric pattern
    fn matches_metric(&self, series_key: &str, pattern: &str) -> bool {
        let metric = series_key.split('|').next().unwrap_or(series_key);

        if pattern.contains('*') {
            // Simple wildcard matching
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                metric.starts_with(parts[0]) && metric.ends_with(parts[1])
            } else {
                metric.starts_with(parts[0])
            }
        } else {
            metric == pattern
        }
    }

    /// Check if series key matches tag filters
    fn matches_tags(&self, series_key: &str, filters: &HashMap<String, String>) -> bool {
        if filters.is_empty() {
            return true;
        }

        let tags = self.extract_tags(series_key);
        for (key, value) in filters {
            if tags.get(key) != Some(value) {
                return false;
            }
        }
        true
    }

    /// Extract tags from series key
    fn extract_tags(&self, series_key: &str) -> HashMap<String, String> {
        let mut tags = HashMap::new();

        if let Some(tags_part) = series_key.split('|').nth(1) {
            for tag in tags_part.split(',') {
                if let Some((k, v)) = tag.split_once('=') {
                    tags.insert(k.to_string(), v.to_string());
                }
            }
        }

        tags
    }

    /// Run retention cleanup
    pub fn run_retention(&self) -> Result<u64> {
        let cutoff = Utc::now() - Duration::days(self.config.retention_days as i64);
        let cutoff_ts = cutoff.timestamp_millis();

        let mut data = self.data.write().unwrap();
        let mut deleted = 0u64;

        for (_, buckets) in data.iter_mut() {
            let old_keys: Vec<i64> = buckets.range(..cutoff_ts).map(|(k, _)| *k).collect();

            for key in old_keys {
                if let Some(block) = buckets.remove(&key) {
                    deleted += block.count as u64;
                }
            }
        }

        Ok(deleted)
    }

    /// Run downsampling
    pub fn run_downsampling(&self) -> Result<u64> {
        let now = Utc::now();
        let mut downsampled = 0u64;

        for rule in &self.config.downsample_rules {
            let cutoff = now - rule.after;
            let cutoff_ts = cutoff.timestamp_millis();

            let data = self.data.read().unwrap();

            for (_series_key, buckets) in data.iter() {
                // Find blocks that need downsampling
                for (_bucket_ts, block) in buckets.range(..cutoff_ts) {
                    if block.count > 1 {
                        // This block needs downsampling
                        // In a real implementation, we would:
                        // 1. Decompress the block
                        // 2. Aggregate to new resolution
                        // 3. Replace with downsampled block
                        downsampled += block.count as u64;
                    }
                }
            }
        }

        Ok(downsampled)
    }

    /// Get storage statistics
    pub fn stats(&self) -> TimeseriesStats {
        let data = self.data.read().unwrap();

        let mut total_series = 0;
        let mut total_points = 0;
        let mut total_bytes = 0;

        for (_, buckets) in data.iter() {
            total_series += 1;
            for (_, block) in buckets.iter() {
                total_points += block.count;
                total_bytes += block.data.len();
            }
        }

        TimeseriesStats {
            total_series,
            total_points,
            total_bytes,
            retention_days: self.config.retention_days,
            downsample_rules: self.config.downsample_rules.len(),
        }
    }
}

/// Time-series storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeseriesStats {
    pub total_series: usize,
    pub total_points: usize,
    pub total_bytes: usize,
    pub retention_days: u32,
    pub downsample_rules: usize,
}

// ============================================================================
// Global Instance
// ============================================================================

use once_cell::sync::Lazy;

pub static TIMESERIES_ENGINE: Lazy<RwLock<Option<TimeseriesEngine>>> =
    Lazy::new(|| RwLock::new(None));

/// Initialize the time-series engine
pub fn init_timeseries(config: TimeseriesConfig) {
    let engine = TimeseriesEngine::new(config);
    let mut guard = TIMESERIES_ENGINE.write().unwrap();
    *guard = Some(engine);
}

/// Get the time-series engine
pub fn get_timeseries_engine(
) -> Option<std::sync::RwLockReadGuard<'static, Option<TimeseriesEngine>>> {
    let guard = TIMESERIES_ENGINE.read().ok()?;
    if guard.is_some() {
        Some(guard)
    } else {
        None
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolution_align() {
        let ts = 1706788234567i64; // Some timestamp

        assert_eq!(Resolution::Minute.align(ts), 1706788200000);
        // Hour alignment rounds down to the start of the hour
        assert_eq!(Resolution::Hour.align(ts), 1706785200000);
    }

    #[test]
    fn test_series_key() {
        let engine = TimeseriesEngine::new(TimeseriesConfig::default());

        let mut tags = HashMap::new();
        tags.insert("host".to_string(), "server1".to_string());
        tags.insert("region".to_string(), "us-east".to_string());

        let key = engine.series_key("cpu.usage", &tags);
        assert!(key.contains("cpu.usage"));
        assert!(key.contains("host=server1"));
        assert!(key.contains("region=us-east"));
    }

    #[test]
    fn test_write_and_query() {
        let engine = TimeseriesEngine::new(TimeseriesConfig::default());

        // Write some data
        let mut series = TimeSeries::new("cpu.usage");
        series = series.with_tag("host", "server1");

        let now = Utc::now().timestamp_millis();
        for i in 0..100 {
            series.add_point(DataPoint::new(now + i * 1000, 50.0 + (i as f64) * 0.1));
        }

        engine.write(&series).unwrap();

        // Query it back
        let result = engine
            .query(&TimeseriesQuery {
                metric: "cpu.usage".to_string(),
                start: Utc::now() - Duration::hours(1),
                end: Utc::now() + Duration::hours(1),
                tags: HashMap::new(),
                aggregation: None,
                resolution: None,
                limit: None,
            })
            .unwrap();

        assert_eq!(result.series.len(), 1);
        assert_eq!(result.series[0].points.len(), 100);
    }

    #[test]
    fn test_aggregation() {
        let engine = TimeseriesEngine::new(TimeseriesConfig::default());

        let points = vec![
            DataPoint::new(1000, 10.0),
            DataPoint::new(2000, 20.0),
            DataPoint::new(3000, 30.0),
            DataPoint::new(4000, 40.0),
        ];

        let avg = engine.aggregate_points(&points, Aggregation::Average, Resolution::Minute);
        assert_eq!(avg.len(), 1);
        assert_eq!(avg[0].value, 25.0);

        let sum = engine.aggregate_points(&points, Aggregation::Sum, Resolution::Minute);
        assert_eq!(sum[0].value, 100.0);
    }

    #[test]
    fn test_wildcard_matching() {
        let engine = TimeseriesEngine::new(TimeseriesConfig::default());

        assert!(engine.matches_metric("cpu.usage", "cpu.*"));
        assert!(engine.matches_metric("cpu.usage", "*.usage"));
        assert!(engine.matches_metric("cpu.usage", "cpu.usage"));
        assert!(!engine.matches_metric("memory.free", "cpu.*"));
    }
}
