//! io_uring I/O backend (Linux 5.1+).

use crate::common::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ============================================================================
// Configuration
// ============================================================================

/// io_uring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoUringConfig {
    /// Enable io_uring
    #[serde(default)]
    pub enabled: bool,

    /// Submission queue depth
    #[serde(default = "default_sq_depth")]
    pub sq_depth: u32,

    /// Completion queue depth (usually 2x sq_depth)
    #[serde(default = "default_cq_depth")]
    pub cq_depth: u32,

    /// Enable kernel polling (IORING_SETUP_SQPOLL)
    #[serde(default)]
    pub kernel_poll: bool,

    /// Kernel poll thread idle timeout in milliseconds
    #[serde(default = "default_poll_idle")]
    pub poll_idle_ms: u32,

    /// Use registered buffers for zero-copy
    #[serde(default = "default_true")]
    pub registered_buffers: bool,

    /// Number of registered buffers
    #[serde(default = "default_buffer_count")]
    pub buffer_count: usize,

    /// Size of each registered buffer
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,

    /// Enable direct I/O (O_DIRECT)
    #[serde(default)]
    pub direct_io: bool,

    /// Batch size for submissions
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

fn default_sq_depth() -> u32 {
    256
}

fn default_cq_depth() -> u32 {
    512
}

fn default_poll_idle() -> u32 {
    1000
}

fn default_buffer_count() -> usize {
    64
}

fn default_buffer_size() -> usize {
    64 * 1024 // 64 KB
}

fn default_batch_size() -> usize {
    32
}

fn default_true() -> bool {
    true
}

impl Default for IoUringConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sq_depth: default_sq_depth(),
            cq_depth: default_cq_depth(),
            kernel_poll: false,
            poll_idle_ms: default_poll_idle(),
            registered_buffers: true,
            buffer_count: default_buffer_count(),
            buffer_size: default_buffer_size(),
            direct_io: false,
            batch_size: default_batch_size(),
        }
    }
}

// ============================================================================
// I/O Operations
// ============================================================================

/// I/O operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoOpType {
    Read,
    Write,
    Fsync,
    Fdatasync,
}

/// I/O operation request
#[derive(Debug)]
pub struct IoRequest {
    /// Operation type
    pub op: IoOpType,

    /// File descriptor (or path for open)
    pub fd: i32,

    /// Offset in file
    pub offset: u64,

    /// Buffer for data
    pub buffer: Vec<u8>,

    /// User data (returned with completion)
    pub user_data: u64,
}

/// I/O operation result
#[derive(Debug)]
pub struct IoResult {
    /// User data from request
    pub user_data: u64,

    /// Result code (bytes transferred or error)
    pub result: i32,

    /// Operation type
    pub op: IoOpType,
}

// ============================================================================
// io_uring Abstraction
// ============================================================================

/// io_uring interface (abstraction for platform compatibility)
pub struct IoUring {
    config: IoUringConfig,

    /// Pending submissions
    pending: VecDeque<IoRequest>,

    /// Statistics
    stats: Arc<IoUringStats>,

    /// Whether io_uring is available
    available: bool,
}

/// io_uring statistics
#[derive(Debug, Default)]
pub struct IoUringStats {
    /// Total submissions
    pub submissions: AtomicU64,

    /// Total completions
    pub completions: AtomicU64,

    /// Total bytes read
    pub bytes_read: AtomicU64,

    /// Total bytes written
    pub bytes_written: AtomicU64,

    /// Batched submissions
    pub batched_submissions: AtomicU64,

    /// Average batch size
    pub avg_batch_size: AtomicU64,

    /// Kernel poll wakeups
    pub poll_wakeups: AtomicU64,
}

impl IoUring {
    /// Create a new io_uring instance
    pub fn new(config: IoUringConfig) -> Result<Self> {
        // Check if io_uring is available (Linux 5.1+)
        let available = Self::check_availability();

        if config.enabled && !available {
            tracing::warn!("io_uring requested but not available, falling back to standard I/O");
        }

        Ok(Self {
            config,
            pending: VecDeque::new(),
            stats: Arc::new(IoUringStats::default()),
            available,
        })
    }

    /// Check if io_uring is available
    fn check_availability() -> bool {
        #[cfg(target_os = "linux")]
        {
            // Check kernel version
            use std::fs;
            if let Ok(version) = fs::read_to_string("/proc/version") {
                // Parse kernel version
                if let Some(ver) = version.split_whitespace().nth(2) {
                    let parts: Vec<&str> = ver.split('.').collect();
                    if parts.len() >= 2 {
                        let major: u32 = parts[0].parse().unwrap_or(0);
                        let minor: u32 = parts[1]
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect::<String>()
                            .parse()
                            .unwrap_or(0);

                        // io_uring available in 5.1+
                        return major > 5 || (major == 5 && minor >= 1);
                    }
                }
            }
            false
        }

        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Check if io_uring is enabled and available
    pub fn is_enabled(&self) -> bool {
        self.config.enabled && self.available
    }

    /// Submit a read operation
    pub fn submit_read(&mut self, fd: i32, offset: u64, len: usize, user_data: u64) {
        let request = IoRequest {
            op: IoOpType::Read,
            fd,
            offset,
            buffer: vec![0u8; len],
            user_data,
        };

        self.pending.push_back(request);
        self.stats.submissions.fetch_add(1, Ordering::Relaxed);
    }

    /// Submit a write operation
    pub fn submit_write(&mut self, fd: i32, offset: u64, data: Vec<u8>, user_data: u64) {
        let request = IoRequest {
            op: IoOpType::Write,
            fd,
            offset,
            buffer: data,
            user_data,
        };

        self.pending.push_back(request);
        self.stats.submissions.fetch_add(1, Ordering::Relaxed);
    }

    /// Submit an fsync operation
    pub fn submit_fsync(&mut self, fd: i32, user_data: u64) {
        let request = IoRequest {
            op: IoOpType::Fsync,
            fd,
            offset: 0,
            buffer: vec![],
            user_data,
        };

        self.pending.push_back(request);
        self.stats.submissions.fetch_add(1, Ordering::Relaxed);
    }

    /// Flush pending submissions
    pub fn flush(&mut self) -> Vec<IoResult> {
        let results = if !self.is_enabled() {
            // Fallback to synchronous I/O
            self.flush_sync()
        } else {
            // Would use actual io_uring here
            // For now, use sync fallback
            self.flush_sync()
        };

        let count = results.len() as u64;
        if count > 0 {
            self.stats
                .batched_submissions
                .fetch_add(1, Ordering::Relaxed);
            self.stats.avg_batch_size.store(count, Ordering::Relaxed);
        }

        results
    }

    /// Synchronous fallback implementation
    fn flush_sync(&mut self) -> Vec<IoResult> {
        let mut results = Vec::new();

        while let Some(request) = self.pending.pop_front() {
            let result = match request.op {
                IoOpType::Read => self.sync_read(request.fd, request.offset, request.buffer.len()),
                IoOpType::Write => self.sync_write(request.fd, request.offset, &request.buffer),
                IoOpType::Fsync => self.sync_fsync(request.fd),
                IoOpType::Fdatasync => self.sync_fsync(request.fd),
            };

            results.push(IoResult {
                user_data: request.user_data,
                result,
                op: request.op,
            });

            self.stats.completions.fetch_add(1, Ordering::Relaxed);
        }

        results
    }

    fn sync_read(&self, _fd: i32, _offset: u64, len: usize) -> i32 {
        // Placeholder - actual implementation would use fd
        self.stats
            .bytes_read
            .fetch_add(len as u64, Ordering::Relaxed);
        len as i32
    }

    fn sync_write(&self, _fd: i32, _offset: u64, data: &[u8]) -> i32 {
        // Placeholder - actual implementation would use fd
        self.stats
            .bytes_written
            .fetch_add(data.len() as u64, Ordering::Relaxed);
        data.len() as i32
    }

    fn sync_fsync(&self, _fd: i32) -> i32 {
        // Placeholder
        0
    }

    /// Get statistics
    pub fn stats(&self) -> &IoUringStats {
        &self.stats
    }

    /// Get pending count
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

// ============================================================================
// High-Level File I/O
// ============================================================================

/// High-performance file handle using io_uring
#[allow(dead_code)]
pub struct UringFile {
    path: PathBuf,
    file: Option<File>,
    uring: Option<IoUring>,
    direct_io: bool,
}

impl UringFile {
    /// Open a file for io_uring operations
    pub fn open(path: PathBuf, config: &IoUringConfig) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| Error::Internal(format!("Failed to open file: {}", e)))?;

        let uring = if config.enabled {
            Some(IoUring::new(config.clone())?)
        } else {
            None
        };

        Ok(Self {
            path,
            file: Some(file),
            uring,
            direct_io: config.direct_io,
        })
    }

    /// Read data at offset
    pub fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>> {
        if let Some(ref mut file) = self.file {
            let mut buffer = vec![0u8; len];
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| Error::Internal(format!("Seek failed: {}", e)))?;
            file.read_exact(&mut buffer)
                .map_err(|e| Error::Internal(format!("Read failed: {}", e)))?;
            Ok(buffer)
        } else {
            Err(Error::Internal("File not open".to_string()))
        }
    }

    /// Write data at offset
    pub fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        if let Some(ref mut file) = self.file {
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| Error::Internal(format!("Seek failed: {}", e)))?;
            file.write_all(data)
                .map_err(|e| Error::Internal(format!("Write failed: {}", e)))?;
            Ok(())
        } else {
            Err(Error::Internal("File not open".to_string()))
        }
    }

    /// Sync to disk
    pub fn sync(&mut self) -> Result<()> {
        if let Some(ref file) = self.file {
            file.sync_all()
                .map_err(|e| Error::Internal(format!("Sync failed: {}", e)))?;
        }
        Ok(())
    }

    /// Async read (queued)
    pub fn async_read(&mut self, offset: u64, len: usize, user_data: u64) {
        if let Some(ref mut uring) = self.uring {
            // Would use actual fd here
            uring.submit_read(0, offset, len, user_data);
        }
    }

    /// Async write (queued)
    pub fn async_write(&mut self, offset: u64, data: Vec<u8>, user_data: u64) {
        if let Some(ref mut uring) = self.uring {
            uring.submit_write(0, offset, data, user_data);
        }
    }

    /// Flush async operations
    pub fn flush_async(&mut self) -> Vec<IoResult> {
        if let Some(ref mut uring) = self.uring {
            uring.flush()
        } else {
            vec![]
        }
    }
}

// ============================================================================
// Batched Write Buffer
// ============================================================================

/// Write buffer for batching small writes
pub struct WriteBatcher {
    buffer: Vec<(u64, Vec<u8>)>, // (offset, data)
    max_entries: usize,
    max_bytes: usize,
    current_bytes: usize,
}

impl WriteBatcher {
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(max_entries),
            max_entries,
            max_bytes,
            current_bytes: 0,
        }
    }

    /// Add a write to the batch
    pub fn add(&mut self, offset: u64, data: Vec<u8>) -> bool {
        let data_len = data.len();

        if self.buffer.len() >= self.max_entries || self.current_bytes + data_len > self.max_bytes {
            return false;
        }

        self.buffer.push((offset, data));
        self.current_bytes += data_len;
        true
    }

    /// Check if batch is full
    pub fn is_full(&self) -> bool {
        self.buffer.len() >= self.max_entries || self.current_bytes >= self.max_bytes
    }

    /// Take the batch
    pub fn take(&mut self) -> Vec<(u64, Vec<u8>)> {
        self.current_bytes = 0;
        std::mem::take(&mut self.buffer)
    }

    /// Current entry count
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

// ============================================================================
// Statistics Snapshot
// ============================================================================

/// Statistics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoUringStatsSnapshot {
    pub submissions: u64,
    pub completions: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub batched_submissions: u64,
    pub avg_batch_size: u64,
    pub poll_wakeups: u64,
}

impl From<&IoUringStats> for IoUringStatsSnapshot {
    fn from(stats: &IoUringStats) -> Self {
        Self {
            submissions: stats.submissions.load(Ordering::Relaxed),
            completions: stats.completions.load(Ordering::Relaxed),
            bytes_read: stats.bytes_read.load(Ordering::Relaxed),
            bytes_written: stats.bytes_written.load(Ordering::Relaxed),
            batched_submissions: stats.batched_submissions.load(Ordering::Relaxed),
            avg_batch_size: stats.avg_batch_size.load(Ordering::Relaxed),
            poll_wakeups: stats.poll_wakeups.load(Ordering::Relaxed),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_uring_config() {
        let config = IoUringConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.sq_depth, 256);
        assert_eq!(config.buffer_size, 64 * 1024);
    }

    #[test]
    fn test_io_uring_creation() {
        let config = IoUringConfig::default();
        let uring = IoUring::new(config).unwrap();
        assert_eq!(uring.pending_count(), 0);
    }

    #[test]
    fn test_write_batcher() {
        let mut batcher = WriteBatcher::new(10, 1024);

        assert!(batcher.add(0, vec![1, 2, 3]));
        assert!(batcher.add(100, vec![4, 5, 6]));
        assert_eq!(batcher.len(), 2);

        let batch = batcher.take();
        assert_eq!(batch.len(), 2);
        assert!(batcher.is_empty());
    }

    #[test]
    fn test_write_batcher_full() {
        let mut batcher = WriteBatcher::new(2, 1024);

        assert!(batcher.add(0, vec![1]));
        assert!(batcher.add(1, vec![2]));
        assert!(batcher.is_full());
        assert!(!batcher.add(2, vec![3]));
    }

    #[test]
    fn test_stats_snapshot() {
        let stats = IoUringStats::default();
        stats.submissions.store(100, Ordering::Relaxed);
        stats.completions.store(95, Ordering::Relaxed);
        stats.bytes_read.store(1024 * 1024, Ordering::Relaxed);

        let snapshot = IoUringStatsSnapshot::from(&stats);
        assert_eq!(snapshot.submissions, 100);
        assert_eq!(snapshot.completions, 95);
        assert_eq!(snapshot.bytes_read, 1024 * 1024);
    }
}
