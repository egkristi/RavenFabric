//! SQLite-backed persistent DTN queue.
//!
//! Provides durable storage for DTN bundles so they survive agent restarts.
//! Behind the `sqlite` feature flag.

#[cfg(feature = "sqlite")]
mod sqlite_impl {
    use std::path::Path;

    use rusqlite::{params, Connection};

    use crate::dtn::{Bundle, Priority};

    /// Error types for the persistent queue.
    #[derive(Debug, thiserror::Error)]
    pub enum PersistentQueueError {
        #[error("SQLite error: {0}")]
        Sqlite(#[from] rusqlite::Error),
        #[error("serialization error: {0}")]
        Serialization(String),
    }

    /// SQLite-backed persistent DTN queue.
    ///
    /// Stores bundles in a SQLite database for durability across restarts.
    /// Maintains priority ordering and deduplication.
    pub struct PersistentDtnQueue {
        conn: Connection,
    }

    impl PersistentDtnQueue {
        /// Open or create a persistent queue at the given path.
        pub fn open(path: &Path) -> Result<Self, PersistentQueueError> {
            let conn = Connection::open(path)?;
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA busy_timeout=5000;",
            )?;
            let queue = Self { conn };
            queue.create_tables()?;
            Ok(queue)
        }

        /// Create an in-memory persistent queue (for testing).
        pub fn open_in_memory() -> Result<Self, PersistentQueueError> {
            let conn = Connection::open_in_memory()?;
            let queue = Self { conn };
            queue.create_tables()?;
            Ok(queue)
        }

        fn create_tables(&self) -> Result<(), PersistentQueueError> {
            self.conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS bundles (
                    id TEXT PRIMARY KEY,
                    source TEXT NOT NULL,
                    destination TEXT NOT NULL,
                    priority INTEGER NOT NULL,
                    ttl_secs INTEGER NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    payload BLOB NOT NULL,
                    custody_requested INTEGER NOT NULL DEFAULT 0,
                    idempotency_key TEXT,
                    hop_count INTEGER NOT NULL DEFAULT 0,
                    max_hops INTEGER NOT NULL DEFAULT 0
                );
                CREATE INDEX IF NOT EXISTS idx_bundles_priority_created
                    ON bundles(priority DESC, created_at_ms ASC);
                CREATE UNIQUE INDEX IF NOT EXISTS idx_bundles_idempotency
                    ON bundles(idempotency_key) WHERE idempotency_key IS NOT NULL;",
            )?;
            Ok(())
        }

        /// Enqueue a bundle. Returns false if duplicate.
        pub fn enqueue(&self, bundle: &Bundle) -> Result<bool, PersistentQueueError> {
            let result = self.conn.execute(
                "INSERT OR IGNORE INTO bundles
                 (id, source, destination, priority, ttl_secs, created_at_ms,
                  payload, custody_requested, idempotency_key, hop_count, max_hops)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    bundle.id,
                    bundle.source,
                    bundle.destination,
                    bundle.priority as i32,
                    bundle.ttl_secs as i64,
                    bundle.created_at_ms as i64,
                    bundle.payload,
                    bundle.custody_requested as i32,
                    bundle.idempotency_key,
                    bundle.hop_count as i32,
                    bundle.max_hops as i32,
                ],
            )?;
            Ok(result > 0)
        }

        /// Dequeue the highest-priority, oldest bundle.
        pub fn dequeue(&self) -> Result<Option<Bundle>, PersistentQueueError> {
            let mut stmt = self.conn.prepare(
                "SELECT id, source, destination, priority, ttl_secs, created_at_ms,
                        payload, custody_requested, idempotency_key, hop_count, max_hops
                 FROM bundles
                 ORDER BY priority DESC, created_at_ms ASC
                 LIMIT 1",
            )?;

            let bundle = stmt
                .query_row([], |row| {
                    Ok(Bundle {
                        id: row.get(0)?,
                        source: row.get(1)?,
                        destination: row.get(2)?,
                        priority: priority_from_i32(row.get::<_, i32>(3)?),
                        ttl_secs: row.get::<_, i64>(4)? as u64,
                        created_at_ms: row.get::<_, i64>(5)? as u64,
                        payload: row.get(6)?,
                        custody_requested: row.get::<_, i32>(7)? != 0,
                        idempotency_key: row.get(8)?,
                        hop_count: row.get::<_, i32>(9)? as u32,
                        max_hops: row.get::<_, i32>(10)? as u32,
                    })
                })
                .optional()?;

            if let Some(ref b) = bundle {
                self.conn
                    .execute("DELETE FROM bundles WHERE id = ?1", params![b.id])?;
            }

            Ok(bundle)
        }

        /// Peek at the highest-priority bundle without removing it.
        pub fn peek(&self) -> Result<Option<Bundle>, PersistentQueueError> {
            let mut stmt = self.conn.prepare(
                "SELECT id, source, destination, priority, ttl_secs, created_at_ms,
                        payload, custody_requested, idempotency_key, hop_count, max_hops
                 FROM bundles
                 ORDER BY priority DESC, created_at_ms ASC
                 LIMIT 1",
            )?;

            let bundle = stmt
                .query_row([], |row| {
                    Ok(Bundle {
                        id: row.get(0)?,
                        source: row.get(1)?,
                        destination: row.get(2)?,
                        priority: priority_from_i32(row.get::<_, i32>(3)?),
                        ttl_secs: row.get::<_, i64>(4)? as u64,
                        created_at_ms: row.get::<_, i64>(5)? as u64,
                        payload: row.get(6)?,
                        custody_requested: row.get::<_, i32>(7)? != 0,
                        idempotency_key: row.get(8)?,
                        hop_count: row.get::<_, i32>(9)? as u32,
                        max_hops: row.get::<_, i32>(10)? as u32,
                    })
                })
                .optional()?;

            Ok(bundle)
        }

        /// Remove expired bundles (except Critical priority).
        pub fn prune_expired(&self, now_ms: u64) -> Result<usize, PersistentQueueError> {
            let deleted = self.conn.execute(
                "DELETE FROM bundles
                 WHERE priority != ?1
                   AND ttl_secs > 0
                   AND ((?2 - created_at_ms) > (ttl_secs * 1000))",
                params![Priority::Critical as i32, now_ms as i64],
            )?;
            Ok(deleted)
        }

        /// Current number of bundles in the queue.
        pub fn len(&self) -> Result<usize, PersistentQueueError> {
            let count: i64 =
                self.conn
                    .query_row("SELECT COUNT(*) FROM bundles", [], |row| row.get(0))?;
            Ok(count as usize)
        }

        /// Whether the queue is empty.
        pub fn is_empty(&self) -> Result<bool, PersistentQueueError> {
            Ok(self.len()? == 0)
        }

        /// Get all bundles destined for a specific agent.
        pub fn bundles_for(
            &self,
            destination: &str,
        ) -> Result<Vec<Bundle>, PersistentQueueError> {
            let mut stmt = self.conn.prepare(
                "SELECT id, source, destination, priority, ttl_secs, created_at_ms,
                        payload, custody_requested, idempotency_key, hop_count, max_hops
                 FROM bundles
                 WHERE destination = ?1
                 ORDER BY priority DESC, created_at_ms ASC",
            )?;

            let bundles = stmt
                .query_map(params![destination], |row| {
                    Ok(Bundle {
                        id: row.get(0)?,
                        source: row.get(1)?,
                        destination: row.get(2)?,
                        priority: priority_from_i32(row.get::<_, i32>(3)?),
                        ttl_secs: row.get::<_, i64>(4)? as u64,
                        created_at_ms: row.get::<_, i64>(5)? as u64,
                        payload: row.get(6)?,
                        custody_requested: row.get::<_, i32>(7)? != 0,
                        idempotency_key: row.get(8)?,
                        hop_count: row.get::<_, i32>(9)? as u32,
                        max_hops: row.get::<_, i32>(10)? as u32,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(bundles)
        }

        /// Delete a specific bundle by ID (e.g., after custody transfer).
        pub fn remove(&self, bundle_id: &str) -> Result<bool, PersistentQueueError> {
            let deleted = self
                .conn
                .execute("DELETE FROM bundles WHERE id = ?1", params![bundle_id])?;
            Ok(deleted > 0)
        }
    }

    /// Convert i32 from DB back to Priority enum.
    fn priority_from_i32(v: i32) -> Priority {
        match v {
            0 => Priority::Low,
            1 => Priority::Normal,
            2 => Priority::High,
            3 => Priority::Critical,
            _ => Priority::Normal,
        }
    }

    /// Extension trait to make rusqlite Optional queries easier.
    trait OptionalExt<T> {
        fn optional(self) -> Result<Option<T>, rusqlite::Error>;
    }

    impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
        fn optional(self) -> Result<Option<T>, rusqlite::Error> {
            match self {
                Ok(v) => Ok(Some(v)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn make_bundle(id: &str, priority: Priority, ttl: u64, created: u64) -> Bundle {
            Bundle {
                id: id.to_string(),
                source: "agent-a".to_string(),
                destination: "agent-b".to_string(),
                priority,
                ttl_secs: ttl,
                created_at_ms: created,
                payload: vec![1, 2, 3],
                custody_requested: false,
                idempotency_key: None,
                hop_count: 0,
                max_hops: 0,
            }
        }

        #[test]
        fn test_persistent_enqueue_dequeue() {
            let queue = PersistentDtnQueue::open_in_memory().unwrap();
            let b = make_bundle("b1", Priority::Normal, 60, 1000);
            assert!(queue.enqueue(&b).unwrap());
            assert_eq!(queue.len().unwrap(), 1);

            let dequeued = queue.dequeue().unwrap().unwrap();
            assert_eq!(dequeued.id, "b1");
            assert!(queue.is_empty().unwrap());
        }

        #[test]
        fn test_persistent_priority_ordering() {
            let queue = PersistentDtnQueue::open_in_memory().unwrap();
            queue
                .enqueue(&make_bundle("low", Priority::Low, 60, 1000))
                .unwrap();
            queue
                .enqueue(&make_bundle("high", Priority::High, 60, 1000))
                .unwrap();
            queue
                .enqueue(&make_bundle("critical", Priority::Critical, 60, 1000))
                .unwrap();
            queue
                .enqueue(&make_bundle("normal", Priority::Normal, 60, 1000))
                .unwrap();

            assert_eq!(queue.dequeue().unwrap().unwrap().id, "critical");
            assert_eq!(queue.dequeue().unwrap().unwrap().id, "high");
            assert_eq!(queue.dequeue().unwrap().unwrap().id, "normal");
            assert_eq!(queue.dequeue().unwrap().unwrap().id, "low");
        }

        #[test]
        fn test_persistent_deduplication() {
            let queue = PersistentDtnQueue::open_in_memory().unwrap();
            // Same ID = duplicate (PRIMARY KEY constraint)
            let b = make_bundle("dup", Priority::Normal, 60, 1000);
            assert!(queue.enqueue(&b).unwrap());
            assert!(!queue.enqueue(&b).unwrap());
            assert_eq!(queue.len().unwrap(), 1);
        }

        #[test]
        fn test_persistent_idempotency_key_dedup() {
            let queue = PersistentDtnQueue::open_in_memory().unwrap();
            let mut b1 = make_bundle("b1", Priority::Normal, 60, 1000);
            b1.idempotency_key = Some("key-1".to_string());
            let mut b2 = make_bundle("b2", Priority::Normal, 60, 2000);
            b2.idempotency_key = Some("key-1".to_string());

            assert!(queue.enqueue(&b1).unwrap());
            assert!(!queue.enqueue(&b2).unwrap()); // Same idempotency key
            assert_eq!(queue.len().unwrap(), 1);
        }

        #[test]
        fn test_persistent_prune_expired() {
            let queue = PersistentDtnQueue::open_in_memory().unwrap();
            queue
                .enqueue(&make_bundle("expired", Priority::Normal, 5, 1000))
                .unwrap();
            queue
                .enqueue(&make_bundle("fresh", Priority::Normal, 60, 10000))
                .unwrap();
            queue
                .enqueue(&make_bundle("critical", Priority::Critical, 1, 0))
                .unwrap();

            let pruned = queue.prune_expired(15000).unwrap();
            assert_eq!(pruned, 1); // Only "expired" removed, not critical
            assert_eq!(queue.len().unwrap(), 2);
        }

        #[test]
        fn test_persistent_bundles_for_destination() {
            let queue = PersistentDtnQueue::open_in_memory().unwrap();
            let mut b1 = make_bundle("b1", Priority::Normal, 60, 1000);
            b1.destination = "agent-x".to_string();
            let mut b2 = make_bundle("b2", Priority::High, 60, 2000);
            b2.destination = "agent-x".to_string();
            let mut b3 = make_bundle("b3", Priority::Normal, 60, 3000);
            b3.destination = "agent-y".to_string();

            queue.enqueue(&b1).unwrap();
            queue.enqueue(&b2).unwrap();
            queue.enqueue(&b3).unwrap();

            let for_x = queue.bundles_for("agent-x").unwrap();
            assert_eq!(for_x.len(), 2);
            assert_eq!(for_x[0].id, "b2"); // High priority first
            assert_eq!(for_x[1].id, "b1");
        }

        #[test]
        fn test_persistent_remove() {
            let queue = PersistentDtnQueue::open_in_memory().unwrap();
            queue
                .enqueue(&make_bundle("b1", Priority::Normal, 60, 1000))
                .unwrap();
            assert!(queue.remove("b1").unwrap());
            assert!(!queue.remove("b1").unwrap()); // Already removed
            assert!(queue.is_empty().unwrap());
        }

        #[test]
        fn test_persistent_peek() {
            let queue = PersistentDtnQueue::open_in_memory().unwrap();
            queue
                .enqueue(&make_bundle("b1", Priority::High, 60, 1000))
                .unwrap();
            let peeked = queue.peek().unwrap().unwrap();
            assert_eq!(peeked.id, "b1");
            // Still in queue
            assert_eq!(queue.len().unwrap(), 1);
        }
    }
}

#[cfg(feature = "sqlite")]
pub use sqlite_impl::{PersistentDtnQueue, PersistentQueueError};
