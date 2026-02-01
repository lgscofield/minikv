//! Change Data Capture - captures data changes and streams them to sinks.

use crate::common::{Error, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;

/// CDC event representing a data change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CDCEvent {
    /// Event ID (UUID)
    pub id: String,

    /// Sequence number for ordering
    pub sequence: u64,

    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,

    /// Type of operation
    pub operation: CDCOperation,

    /// Key that was modified
    pub key: String,

    /// Value before the change (for updates and deletes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<Vec<u8>>,

    /// Value after the change (for inserts and updates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<Vec<u8>>,

    /// Tenant that owns the data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,

    /// Additional metadata
    #[serde(default)]
    pub metadata: CDCMetadata,
}

/// CDC operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CDCOperation {
    /// New key inserted
    Insert,
    /// Existing key updated
    Update,
    /// Key deleted
    Delete,
    /// Snapshot of all data
    Snapshot,
}

/// Additional metadata for CDC events
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CDCMetadata {
    /// Source coordinator/volume ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_node: Option<String>,

    /// Datacenter ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datacenter: Option<String>,

    /// Transaction ID (if part of a transaction)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,

    /// User/API key that made the change
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,

    /// Custom tags
    #[serde(default)]
    pub tags: std::collections::HashMap<String, String>,
}

/// CDC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CDCConfig {
    /// Enable CDC
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Buffer size for events
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,

    /// Batch size for sink delivery
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Flush interval in milliseconds
    #[serde(default = "default_flush_interval")]
    pub flush_interval_ms: u64,

    /// Sink configurations
    #[serde(default)]
    pub sinks: Vec<SinkConfig>,

    /// Filter by operation types
    #[serde(default)]
    pub filter_operations: Vec<CDCOperation>,

    /// Filter by key prefix
    #[serde(default)]
    pub filter_key_prefix: Option<String>,

    /// Include old values in events
    #[serde(default = "default_true")]
    pub include_old_values: bool,
}

fn default_true() -> bool {
    true
}

fn default_buffer_size() -> usize {
    10000
}

fn default_batch_size() -> usize {
    100
}

fn default_flush_interval() -> u64 {
    1000
}

impl Default for CDCConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            buffer_size: 10000,
            batch_size: 100,
            flush_interval_ms: 1000,
            sinks: vec![],
            filter_operations: vec![],
            filter_key_prefix: None,
            include_old_values: true,
        }
    }
}

/// Sink configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SinkConfig {
    /// Webhook sink - sends events to an HTTP endpoint
    Webhook {
        url: String,
        #[serde(default)]
        headers: std::collections::HashMap<String, String>,
        #[serde(default = "default_timeout")]
        timeout_ms: u64,
        #[serde(default = "default_retries")]
        max_retries: u32,
    },

    /// Kafka sink - sends events to a Kafka topic
    Kafka {
        brokers: Vec<String>,
        topic: String,
        #[serde(default)]
        client_id: Option<String>,
    },

    /// File sink - appends events to a file
    File {
        path: String,
        #[serde(default)]
        rotate_size_mb: Option<u64>,
        #[serde(default)]
        max_files: Option<u32>,
    },

    /// Memory sink - stores events in memory (for testing)
    Memory {
        #[serde(default = "default_memory_limit")]
        max_events: usize,
    },
}

fn default_timeout() -> u64 {
    5000
}

fn default_retries() -> u32 {
    3
}

fn default_memory_limit() -> usize {
    1000
}

/// Trait for CDC sinks
#[async_trait]
pub trait CDCSink: Send + Sync {
    /// Name of the sink
    fn name(&self) -> &str;

    /// Send a batch of events
    async fn send(&self, events: Vec<CDCEvent>) -> Result<()>;

    /// Check if the sink is healthy
    async fn health_check(&self) -> Result<bool>;
}

/// Webhook sink implementation
pub struct WebhookSink {
    name: String,
    url: String,
    headers: std::collections::HashMap<String, String>,
    timeout_ms: u64,
    max_retries: u32,
    client: reqwest::Client,
}

impl WebhookSink {
    pub fn new(
        url: String,
        headers: std::collections::HashMap<String, String>,
        timeout_ms: u64,
        max_retries: u32,
    ) -> Self {
        Self {
            name: format!("webhook:{}", url),
            url,
            headers,
            timeout_ms,
            max_retries,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl CDCSink for WebhookSink {
    fn name(&self) -> &str {
        &self.name
    }

    async fn send(&self, events: Vec<CDCEvent>) -> Result<()> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            let body = serde_json::to_string(&events)
                .map_err(|e| Error::Other(format!("Failed to serialize events: {}", e)))?;
            let mut request = self
                .client
                .post(&self.url)
                .timeout(std::time::Duration::from_millis(self.timeout_ms))
                .header("Content-Type", "application/json")
                .body(body);

            for (key, value) in &self.headers {
                request = request.header(key, value);
            }

            match request.send().await {
                Ok(response) if response.status().is_success() => {
                    return Ok(());
                }
                Ok(response) => {
                    last_error = Some(format!("HTTP error: {}", response.status()));
                }
                Err(e) => {
                    last_error = Some(format!("Request error: {}", e));
                }
            }

            if attempt < self.max_retries {
                tokio::time::sleep(std::time::Duration::from_millis(100 * (attempt as u64 + 1)))
                    .await;
            }
        }

        Err(Error::Other(format!(
            "Webhook sink failed after {} retries: {}",
            self.max_retries,
            last_error.unwrap_or_default()
        )))
    }

    async fn health_check(&self) -> Result<bool> {
        // Try to connect to the webhook URL
        match self
            .client
            .head(&self.url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// File sink implementation
pub struct FileSink {
    name: String,
    #[allow(dead_code)]
    path: String,
    writer: Arc<RwLock<Option<std::fs::File>>>,
}

impl FileSink {
    pub fn new(path: String) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| Error::Other(format!("Failed to open CDC file: {}", e)))?;

        Ok(Self {
            name: format!("file:{}", path),
            path,
            writer: Arc::new(RwLock::new(Some(file))),
        })
    }
}

#[async_trait]
impl CDCSink for FileSink {
    fn name(&self) -> &str {
        &self.name
    }

    async fn send(&self, events: Vec<CDCEvent>) -> Result<()> {
        use std::io::Write;

        let mut writer = self.writer.write().unwrap();
        let file = writer
            .as_mut()
            .ok_or_else(|| Error::Other("File sink not initialized".to_string()))?;

        for event in events {
            let json = serde_json::to_string(&event)
                .map_err(|e| Error::Other(format!("Failed to serialize event: {}", e)))?;
            writeln!(file, "{}", json)
                .map_err(|e| Error::Other(format!("Failed to write event: {}", e)))?;
        }

        file.flush()
            .map_err(|e| Error::Other(format!("Failed to flush file: {}", e)))?;

        Ok(())
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(self.writer.read().unwrap().is_some())
    }
}

/// Memory sink implementation (for testing)
pub struct MemorySink {
    name: String,
    events: Arc<RwLock<VecDeque<CDCEvent>>>,
    max_events: usize,
}

impl MemorySink {
    pub fn new(max_events: usize) -> Self {
        Self {
            name: "memory".to_string(),
            events: Arc::new(RwLock::new(VecDeque::with_capacity(max_events))),
            max_events,
        }
    }

    pub fn get_events(&self) -> Vec<CDCEvent> {
        self.events.read().unwrap().iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.events.write().unwrap().clear();
    }
}

#[async_trait]
impl CDCSink for MemorySink {
    fn name(&self) -> &str {
        &self.name
    }

    async fn send(&self, events: Vec<CDCEvent>) -> Result<()> {
        let mut buffer = self.events.write().unwrap();
        for event in events {
            if buffer.len() >= self.max_events {
                buffer.pop_front();
            }
            buffer.push_back(event);
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

/// CDC Manager - coordinates event capture and delivery
pub struct CDCManager {
    config: CDCConfig,
    sinks: Vec<Arc<dyn CDCSink>>,
    sequence: Arc<RwLock<u64>>,
    buffer: Arc<RwLock<Vec<CDCEvent>>>,
    event_tx: mpsc::Sender<CDCEvent>,
}

impl CDCManager {
    /// Create a new CDC manager
    pub fn new(config: CDCConfig) -> (Self, mpsc::Receiver<CDCEvent>) {
        let (event_tx, event_rx) = mpsc::channel(config.buffer_size);

        let manager = Self {
            config,
            sinks: vec![],
            sequence: Arc::new(RwLock::new(0)),
            buffer: Arc::new(RwLock::new(Vec::new())),
            event_tx,
        };

        (manager, event_rx)
    }

    /// Add a sink
    pub fn add_sink(&mut self, sink: Arc<dyn CDCSink>) {
        self.sinks.push(sink);
    }

    /// Create sinks from configuration
    pub fn create_sinks_from_config(&mut self) -> Result<()> {
        for sink_config in &self.config.sinks {
            let sink: Arc<dyn CDCSink> = match sink_config {
                SinkConfig::Webhook {
                    url,
                    headers,
                    timeout_ms,
                    max_retries,
                } => Arc::new(WebhookSink::new(
                    url.clone(),
                    headers.clone(),
                    *timeout_ms,
                    *max_retries,
                )),
                SinkConfig::File { path, .. } => Arc::new(FileSink::new(path.clone())?),
                SinkConfig::Memory { max_events } => Arc::new(MemorySink::new(*max_events)),
                SinkConfig::Kafka { .. } => {
                    // Kafka sink would require additional dependencies
                    // For now, skip Kafka sinks
                    continue;
                }
            };
            self.sinks.push(sink);
        }
        Ok(())
    }

    /// Generate next sequence number
    fn next_sequence(&self) -> u64 {
        let mut seq = self.sequence.write().unwrap();
        *seq += 1;
        *seq
    }

    /// Check if event passes filters
    fn passes_filter(&self, event: &CDCEvent) -> bool {
        // Check operation filter
        if !self.config.filter_operations.is_empty()
            && !self.config.filter_operations.contains(&event.operation)
        {
            return false;
        }

        // Check key prefix filter
        if let Some(ref prefix) = self.config.filter_key_prefix {
            if !event.key.starts_with(prefix) {
                return false;
            }
        }

        true
    }

    /// Capture an insert event
    pub async fn capture_insert(
        &self,
        key: &str,
        value: Vec<u8>,
        tenant: Option<String>,
        metadata: CDCMetadata,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let event = CDCEvent {
            id: uuid::Uuid::new_v4().to_string(),
            sequence: self.next_sequence(),
            timestamp: Utc::now(),
            operation: CDCOperation::Insert,
            key: key.to_string(),
            old_value: None,
            new_value: Some(value),
            tenant,
            metadata,
        };

        if self.passes_filter(&event) {
            self.event_tx
                .send(event)
                .await
                .map_err(|e| Error::Other(format!("Failed to send CDC event: {}", e)))?;
        }

        Ok(())
    }

    /// Capture an update event
    pub async fn capture_update(
        &self,
        key: &str,
        old_value: Option<Vec<u8>>,
        new_value: Vec<u8>,
        tenant: Option<String>,
        metadata: CDCMetadata,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let event = CDCEvent {
            id: uuid::Uuid::new_v4().to_string(),
            sequence: self.next_sequence(),
            timestamp: Utc::now(),
            operation: CDCOperation::Update,
            key: key.to_string(),
            old_value: if self.config.include_old_values {
                old_value
            } else {
                None
            },
            new_value: Some(new_value),
            tenant,
            metadata,
        };

        if self.passes_filter(&event) {
            self.event_tx
                .send(event)
                .await
                .map_err(|e| Error::Other(format!("Failed to send CDC event: {}", e)))?;
        }

        Ok(())
    }

    /// Capture a delete event
    pub async fn capture_delete(
        &self,
        key: &str,
        old_value: Option<Vec<u8>>,
        tenant: Option<String>,
        metadata: CDCMetadata,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let event = CDCEvent {
            id: uuid::Uuid::new_v4().to_string(),
            sequence: self.next_sequence(),
            timestamp: Utc::now(),
            operation: CDCOperation::Delete,
            key: key.to_string(),
            old_value: if self.config.include_old_values {
                old_value
            } else {
                None
            },
            new_value: None,
            tenant,
            metadata,
        };

        if self.passes_filter(&event) {
            self.event_tx
                .send(event)
                .await
                .map_err(|e| Error::Other(format!("Failed to send CDC event: {}", e)))?;
        }

        Ok(())
    }

    /// Flush events to all sinks
    pub async fn flush(&self) -> Result<()> {
        let events: Vec<CDCEvent> = {
            let mut buffer = self.buffer.write().unwrap();
            std::mem::take(&mut *buffer)
        };

        if events.is_empty() {
            return Ok(());
        }

        for sink in &self.sinks {
            if let Err(e) = sink.send(events.clone()).await {
                tracing::error!("CDC sink {} failed: {}", sink.name(), e);
            }
        }

        Ok(())
    }

    /// Get the current sequence number
    pub fn current_sequence(&self) -> u64 {
        *self.sequence.read().unwrap()
    }

    /// Get health status of all sinks
    pub async fn health_check(&self) -> Vec<(String, bool)> {
        let mut results = vec![];
        for sink in &self.sinks {
            let healthy = sink.health_check().await.unwrap_or(false);
            results.push((sink.name().to_string(), healthy));
        }
        results
    }
}

/// Global CDC manager instance
pub static CDC_MANAGER: once_cell::sync::Lazy<RwLock<Option<CDCManager>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(None));

/// Initialize the global CDC manager
pub fn init_cdc(config: CDCConfig) -> mpsc::Receiver<CDCEvent> {
    let (mut manager, rx) = CDCManager::new(config);
    let _ = manager.create_sinks_from_config();
    *CDC_MANAGER.write().unwrap() = Some(manager);
    rx
}

/// Get the global CDC manager
pub fn get_cdc_manager() -> Option<std::sync::RwLockReadGuard<'static, Option<CDCManager>>> {
    let guard = CDC_MANAGER.read().unwrap();
    if guard.is_some() {
        Some(guard)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_sink() {
        let sink = MemorySink::new(100);

        let event = CDCEvent {
            id: "test-1".to_string(),
            sequence: 1,
            timestamp: Utc::now(),
            operation: CDCOperation::Insert,
            key: "test-key".to_string(),
            old_value: None,
            new_value: Some(b"test-value".to_vec()),
            tenant: None,
            metadata: CDCMetadata::default(),
        };

        sink.send(vec![event.clone()]).await.unwrap();

        let events = sink.get_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].key, "test-key");
    }

    #[tokio::test]
    async fn test_memory_sink_overflow() {
        let sink = MemorySink::new(2);

        for i in 0..5 {
            let event = CDCEvent {
                id: format!("test-{}", i),
                sequence: i,
                timestamp: Utc::now(),
                operation: CDCOperation::Insert,
                key: format!("key-{}", i),
                old_value: None,
                new_value: Some(b"value".to_vec()),
                tenant: None,
                metadata: CDCMetadata::default(),
            };
            sink.send(vec![event]).await.unwrap();
        }

        let events = sink.get_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].key, "key-3");
        assert_eq!(events[1].key, "key-4");
    }
}
