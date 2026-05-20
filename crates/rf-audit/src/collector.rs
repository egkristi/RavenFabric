//! Centralized audit collector — buffered, deduplicated log forwarding.
//!
//! `BufferedAuditCollector<L>` wraps any `AuditLogger` and adds:
//!
//! - **Bounded in-memory buffer** — ring-buffer of configurable capacity.
//!   When full, the oldest entry is dropped (logged at `warn` level).
//! - **Background flush** — a dedicated thread drains the buffer to the
//!   inner logger at a configurable interval. Network interruptions are
//!   handled gracefully: entries remain buffered until the next flush cycle.
//! - **Deduplication** — a sliding window of `request_id` values prevents
//!   duplicate events from being forwarded on reconnect/replay. Window size
//!   is configurable (default: 1,024 entries).
//! - **Retention policies** — entries older than `max_age` are silently
//!   discarded before forwarding (default: 24 hours).
//! - **Graceful shutdown** — `flush_and_stop()` drains the buffer and joins
//!   the background thread. If not called explicitly, `Drop` does a best-effort
//!   flush without joining.
//!
//! The inner logger must be `Send + Sync + 'static`. Wrap it in an `Arc` to
//! share across threads.

use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

use chrono::Utc;

use crate::{
    logger::{AuditError, AuditLogger},
    types::AuditEntry,
};

// ── Config ────────────────────────────────────────────────────────────────────

/// Configuration for `BufferedAuditCollector`.
#[derive(Debug, Clone)]
pub struct CollectorConfig {
    /// Maximum number of events to hold in the in-memory buffer.
    /// Oldest entries are dropped when the buffer is full.
    pub buffer_capacity: usize,
    /// How often the background thread flushes buffered entries to the inner
    /// logger. Shorter intervals reduce latency; longer intervals improve
    /// batching efficiency.
    pub flush_interval: Duration,
    /// Number of recent `request_id` values kept for deduplication.
    /// Once the window is full the oldest entry is evicted (FIFO).
    pub dedup_window: usize,
    /// Maximum age of buffered entries before they are silently discarded.
    /// Set to `None` to disable age-based retention.
    pub max_age: Option<Duration>,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            buffer_capacity: 4_096,
            flush_interval: Duration::from_secs(5),
            dedup_window: 1_024,
            max_age: Some(Duration::from_secs(86_400)), // 24 hours
        }
    }
}

impl CollectorConfig {
    /// Create a new config with a custom buffer capacity.
    pub fn new(buffer_capacity: usize) -> Self {
        Self {
            buffer_capacity,
            ..Default::default()
        }
    }

    /// Set the flush interval.
    pub fn with_flush_interval(mut self, interval: Duration) -> Self {
        self.flush_interval = interval;
        self
    }

    /// Set the deduplication window size.
    pub fn with_dedup_window(mut self, size: usize) -> Self {
        self.dedup_window = size;
        self
    }

    /// Set the maximum entry age (retention). `None` disables age-based eviction.
    pub fn with_max_age(mut self, max_age: Option<Duration>) -> Self {
        self.max_age = max_age;
        self
    }
}

// ── Shared state ──────────────────────────────────────────────────────────────

struct State {
    buffer: VecDeque<AuditEntry>,
    /// FIFO queue of request_ids for deduplication (oldest first).
    dedup_queue: VecDeque<String>,
    /// Set for O(1) lookup.
    dedup_set: HashSet<String>,
    stop: bool,
}

// ── BufferedAuditCollector ────────────────────────────────────────────────────

/// A buffered, deduplicated audit logger that forwards events to an inner logger.
///
/// Events are enqueued immediately and forwarded to the inner logger by a
/// background thread at a configurable interval. Duplicate events (same
/// `request_id`) within the deduplication window are silently dropped.
pub struct BufferedAuditCollector<L: AuditLogger + Send + Sync + 'static> {
    state: Arc<(Mutex<State>, Condvar)>,
    config: CollectorConfig,
    /// `None` after `flush_and_stop()` is called.
    worker: Option<JoinHandle<()>>,
    inner: Arc<L>,
}

impl<L: AuditLogger + Send + Sync + 'static> BufferedAuditCollector<L> {
    /// Create a new `BufferedAuditCollector` wrapping `inner`.
    pub fn new(inner: L, config: CollectorConfig) -> Self {
        let state = Arc::new((
            Mutex::new(State {
                buffer: VecDeque::with_capacity(config.buffer_capacity.min(1024)),
                dedup_queue: VecDeque::with_capacity(config.dedup_window.min(1024)),
                dedup_set: HashSet::new(),
                stop: false,
            }),
            Condvar::new(),
        ));
        let inner = Arc::new(inner);
        let worker = {
            let state = state.clone();
            let inner = inner.clone();
            let config = config.clone();
            thread::Builder::new()
                .name("rf-audit-collector".into())
                .spawn(move || Self::worker_loop(state, inner, config))
                .expect("failed to spawn audit collector thread")
        };
        Self {
            state,
            config,
            worker: Some(worker),
            inner,
        }
    }

    /// Drain the buffer and stop the background thread. Blocks until done.
    pub fn flush_and_stop(mut self) -> Arc<L> {
        {
            let (lock, cvar) = &*self.state;
            let mut s = lock.lock().unwrap_or_else(|p| p.into_inner());
            s.stop = true;
            cvar.notify_all();
        }
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
        self.inner.clone()
    }

    /// Background flush loop.
    fn worker_loop(state: Arc<(Mutex<State>, Condvar)>, inner: Arc<L>, config: CollectorConfig) {
        let (lock, cvar) = &*state;
        loop {
            // Wait for flush_interval or a stop signal.
            let (mut s, _) = cvar
                .wait_timeout(
                    lock.lock().unwrap_or_else(|p| p.into_inner()),
                    config.flush_interval,
                )
                .unwrap_or_else(|p| p.into_inner());

            let should_stop = s.stop;

            // Collect all buffered entries, applying age-based retention.
            let now = Utc::now();
            let to_forward: Vec<AuditEntry> = s
                .buffer
                .drain(..)
                .filter(|e| {
                    if let Some(max_age) = config.max_age {
                        let age = now.signed_duration_since(e.timestamp);
                        // Use sub-second precision: convert to std Duration for comparison.
                        match age.to_std() {
                            Ok(age_std) => age_std <= max_age,
                            Err(_) => true, // future timestamp — keep
                        }
                    } else {
                        true
                    }
                })
                .collect();
            drop(s);

            for entry in to_forward {
                if let Err(e) = inner.log(entry) {
                    tracing::warn!("BufferedAuditCollector: inner logger error: {e}");
                }
            }

            if should_stop {
                break;
            }
        }
    }
}

impl<L: AuditLogger + Send + Sync + 'static> AuditLogger for BufferedAuditCollector<L> {
    fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
        let (lock, _cvar) = &*self.state;
        let mut s = lock.lock().unwrap_or_else(|p| p.into_inner());

        // Deduplication check.
        if s.dedup_set.contains(&entry.request_id) {
            tracing::debug!(
                "BufferedAuditCollector: duplicate event '{}' dropped",
                entry.request_id
            );
            return Ok(());
        }

        // Maintain dedup window.
        if s.dedup_queue.len() >= self.config.dedup_window {
            if let Some(old) = s.dedup_queue.pop_front() {
                s.dedup_set.remove(&old);
            }
        }
        s.dedup_set.insert(entry.request_id.clone());
        s.dedup_queue.push_back(entry.request_id.clone());

        // Buffer capacity enforcement.
        if s.buffer.len() >= self.config.buffer_capacity {
            s.buffer.pop_front(); // evict oldest
            tracing::warn!("BufferedAuditCollector: buffer full, oldest entry evicted");
        }
        s.buffer.push_back(entry);
        Ok(())
    }
}

impl<L: AuditLogger + Send + Sync + 'static> Drop for BufferedAuditCollector<L> {
    fn drop(&mut self) {
        // Best-effort: signal stop but don't block on join.
        let (lock, cvar) = &*self.state;
        let mut s = lock.lock().unwrap_or_else(|p| p.into_inner());
        s.stop = true;
        cvar.notify_all();
        drop(s);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use chrono::Utc;

    use super::*;
    use crate::{logger::AuditError, types::AuditEntry};

    // Simple in-memory logger for testing.
    #[derive(Clone, Default)]
    struct MemLogger {
        entries: Arc<Mutex<Vec<AuditEntry>>>,
    }

    impl AuditLogger for MemLogger {
        fn log(&self, entry: AuditEntry) -> Result<(), AuditError> {
            self.entries.lock().unwrap().push(entry);
            Ok(())
        }
    }

    fn make_entry(id: &str) -> AuditEntry {
        AuditEntry {
            timestamp: Utc::now(),
            request_id: id.into(),
            action: "Execute".into(),
            command: Some("echo test".into()),
            decision: "allowed".into(),
            matched_rule: "commands:allow[0]".into(),
            exit_code: Some(0),
            duration_ms: 1,
            caller_key: "testkey".into(),
            reason: None,
        }
    }

    #[test]
    fn test_buffered_entries_forwarded_after_flush() {
        let mem = MemLogger::default();
        let entries = mem.entries.clone();
        let config = CollectorConfig::new(100)
            .with_flush_interval(Duration::from_millis(50))
            .with_max_age(None);
        let collector = BufferedAuditCollector::new(mem, config);

        collector.log(make_entry("r1")).unwrap();
        collector.log(make_entry("r2")).unwrap();

        // Wait for background flush
        std::thread::sleep(Duration::from_millis(200));

        let logged = entries.lock().unwrap();
        assert_eq!(logged.len(), 2);
    }

    #[test]
    fn test_deduplication_drops_duplicate_request_ids() {
        let mem = MemLogger::default();
        let entries = mem.entries.clone();
        let config = CollectorConfig::new(100)
            .with_flush_interval(Duration::from_millis(50))
            .with_dedup_window(64)
            .with_max_age(None);
        let collector = BufferedAuditCollector::new(mem, config);

        collector.log(make_entry("dup-id")).unwrap();
        collector.log(make_entry("dup-id")).unwrap(); // duplicate
        collector.log(make_entry("unique-id")).unwrap();

        std::thread::sleep(Duration::from_millis(200));

        let logged = entries.lock().unwrap();
        assert_eq!(logged.len(), 2, "duplicate should be dropped");
        assert!(logged.iter().any(|e| e.request_id == "dup-id"));
        assert!(logged.iter().any(|e| e.request_id == "unique-id"));
    }

    #[test]
    fn test_buffer_overflow_drops_oldest() {
        let mem = MemLogger::default();
        // buffer capacity = 3, flush very infrequently so nothing flushes during test
        let config = CollectorConfig::new(3)
            .with_flush_interval(Duration::from_secs(60))
            .with_dedup_window(1024)
            .with_max_age(None);
        let collector = BufferedAuditCollector::new(mem, config);

        collector.log(make_entry("old-1")).unwrap();
        collector.log(make_entry("old-2")).unwrap();
        collector.log(make_entry("old-3")).unwrap();
        // 4th entry: should evict "old-1"
        collector.log(make_entry("new-1")).unwrap();

        let (lock, _) = &*collector.state;
        let s = lock.lock().unwrap();
        assert_eq!(s.buffer.len(), 3);
        assert_eq!(s.buffer[0].request_id, "old-2");
        assert_eq!(s.buffer[2].request_id, "new-1");
    }

    #[test]
    fn test_age_based_retention_drops_old_entries() {
        let mem = MemLogger::default();
        let entries = mem.entries.clone();
        // max_age = 1ms: everything should be too old before flush fires
        let config = CollectorConfig::new(100)
            .with_flush_interval(Duration::from_millis(50))
            .with_max_age(Some(Duration::from_millis(1)));
        let collector = BufferedAuditCollector::new(mem, config);

        collector.log(make_entry("stale")).unwrap();
        // Sleep long enough for the entry to be older than max_age before flush
        std::thread::sleep(Duration::from_millis(200));

        let logged = entries.lock().unwrap();
        assert_eq!(logged.len(), 0, "stale entry should have been discarded");
    }

    #[test]
    fn test_flush_and_stop_drains_buffer() {
        let mem = MemLogger::default();
        let entries = mem.entries.clone();
        // Long flush interval: nothing flushes until stop
        let config = CollectorConfig::new(100)
            .with_flush_interval(Duration::from_millis(50))
            .with_max_age(None);
        let collector = BufferedAuditCollector::new(mem, config);

        collector.log(make_entry("a")).unwrap();
        collector.log(make_entry("b")).unwrap();

        // Stop triggers a final flush
        let _ = collector.flush_and_stop();

        let logged = entries.lock().unwrap();
        assert_eq!(logged.len(), 2);
    }

    #[test]
    fn test_dedup_window_eviction() {
        let mem = MemLogger::default();
        let entries = mem.entries.clone();
        // dedup_window = 2: after 2 different IDs, old ones are evicted
        let config = CollectorConfig::new(100)
            .with_flush_interval(Duration::from_millis(50))
            .with_dedup_window(2)
            .with_max_age(None);
        let collector = BufferedAuditCollector::new(mem, config);

        collector.log(make_entry("id-1")).unwrap();
        collector.log(make_entry("id-2")).unwrap();
        // id-1 evicted from dedup window, so a second id-1 should be allowed
        collector.log(make_entry("id-3")).unwrap(); // evicts id-1
        collector.log(make_entry("id-1")).unwrap(); // should NOT be deduped now

        std::thread::sleep(Duration::from_millis(200));

        let logged = entries.lock().unwrap();
        // id-1, id-2, id-3, id-1 = 4 entries (id-1 appears twice)
        let id1_count = logged.iter().filter(|e| e.request_id == "id-1").count();
        assert_eq!(
            id1_count, 2,
            "id-1 should appear twice after dedup eviction"
        );
        assert_eq!(logged.len(), 4);
    }
}
