//! SQLite-backed usage ledger: schema, bounded writer thread, retention, clear.
use crate::{
    Error, Result,
    storage::{Store, ensure_directory},
    usage::{ReadingRow, UsageRow, WriteJob},
};
use rusqlite::{Connection, OpenFlags, params};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError, sync_channel},
    },
    thread::JoinHandle,
    time::Duration,
};

pub(crate) const DB_RELATIVE: &str = "usage/usage.db";
const SCHEMA_VERSION: i64 = 1;
const RETENTION_MS: i64 = 90 * 24 * 60 * 60 * 1000;
const READINGS_RETENTION_MS: i64 = 45 * 24 * 60 * 60 * 1000;
const USAGE_ROW_LIMIT: i64 = 200_000;
const READINGS_ROW_LIMIT: i64 = 100_000;
const MAX_PAGE_COUNT: i64 = 32_768;
const CHANNEL_CAPACITY: usize = 4_096;
const BATCH_LIMIT: usize = 256;
const BUSY_MS: u64 = 250;

pub(crate) fn db_path(store: &Store) -> Result<PathBuf> {
    store.path(DB_RELATIVE)
}

fn unavailable() -> Error {
    Error::new("storage_unavailable", "The usage ledger is unavailable")
}

fn corrupt() -> Error {
    Error::new(
        "storage_record_invalid",
        "The usage ledger is malformed; preserve it and repair the affected store",
    )
}

fn unsupported() -> Error {
    Error::new(
        "storage_version_unsupported",
        "The usage ledger has an unsupported storage version",
    )
}

/// Open an existing ledger for reading. `None` means no usage was recorded yet.
/// A corrupt or unsupported store is an error, never an empty ledger.
pub(crate) fn open_read(store: &Store) -> Result<Option<Connection>> {
    let path = db_path(store)?;
    if !path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| unavailable())?;
    let _ = conn.busy_timeout(Duration::from_millis(BUSY_MS));
    match version(&conn)? {
        SCHEMA_VERSION => Ok(Some(conn)),
        0 => Err(corrupt()),
        _ => Err(unsupported()),
    }
}

/// Open the ledger for writing, creating its directory and schema when needed.
pub(crate) fn open_write(store: &Store) -> Result<Connection> {
    let path = db_path(store)?;
    if let Some(parent) = path.parent() {
        ensure_directory(parent)?;
    }
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| unavailable())?;
    let _ = conn.busy_timeout(Duration::from_millis(BUSY_MS));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    conn.execute_batch(&format!(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA max_page_count={MAX_PAGE_COUNT};"
    ))
    .map_err(|_| unavailable())?;
    match version(&conn)? {
        0 => create_schema(&conn)?,
        SCHEMA_VERSION => {}
        _ => return Err(unsupported()),
    }
    Ok(conn)
}

fn version(conn: &Connection) -> Result<i64> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| corrupt())
}

fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS usage_record (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           received_at_ms INTEGER NOT NULL,
           admitted_at_ms INTEGER,
           membership_ms INTEGER NOT NULL,
           origin TEXT NOT NULL,
           requested_model TEXT,
           model_kind TEXT NOT NULL,
           provider TEXT,
           qualified_model TEXT,
           target TEXT,
           agent_provider TEXT,
           protocol TEXT,
           outcome TEXT NOT NULL,
           stage TEXT NOT NULL,
           reason_code TEXT,
           input_tokens INTEGER,
           output_tokens INTEGER,
           cache_read_tokens INTEGER,
           cache_write_tokens INTEGER,
           usage_state TEXT NOT NULL,
           duration_ms INTEGER,
           cost_micros INTEGER,
           price_source TEXT,
           billing_kind TEXT NOT NULL,
           cost_state TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS usage_record_membership ON usage_record(membership_ms);
         CREATE INDEX IF NOT EXISTS usage_record_provider ON usage_record(provider, membership_ms);
         CREATE INDEX IF NOT EXISTS usage_record_duration ON usage_record(membership_ms, duration_ms);
         CREATE TABLE IF NOT EXISTS allowance_reading (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           observed_at_ms INTEGER NOT NULL,
           received_at_ms INTEGER NOT NULL,
           provider TEXT NOT NULL,
           binding TEXT NOT NULL,
           scope_window TEXT NOT NULL,
           basis TEXT NOT NULL,
           percent TEXT,
           counters TEXT,
           unit TEXT,
           source TEXT NOT NULL,
           access_state TEXT,
           reset TEXT NOT NULL,
           freshness TEXT
         );
         CREATE INDEX IF NOT EXISTS allowance_reading_observed ON allowance_reading(observed_at_ms);
         CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         PRAGMA user_version = 1;
         COMMIT;",
    )
    .map_err(|_| unavailable())
}

pub(crate) fn meta_get(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
        row.get(0)
    })
    .ok()
}

pub(crate) fn meta_i64(conn: &Connection, key: &str) -> i64 {
    meta_get(conn, key)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
}

fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES(?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|_| unavailable())?;
    Ok(())
}

fn bump_meta(conn: &Connection, key: &str, delta: i64) -> Result<()> {
    if delta == 0 {
        return Ok(());
    }
    let current = meta_i64(conn, key);
    let next = current.saturating_add(delta).max(0);
    meta_set(conn, key, &next.to_string())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn insert_usage(conn: &Connection, row: &UsageRow) -> Result<()> {
    conn.execute(
        "INSERT INTO usage_record(
           received_at_ms, admitted_at_ms, membership_ms, origin, requested_model,
           model_kind, provider, qualified_model, target, agent_provider, protocol,
           outcome, stage, reason_code, input_tokens, output_tokens,
           cache_read_tokens, cache_write_tokens, usage_state, duration_ms,
           cost_micros, price_source, billing_kind, cost_state)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24)",
        params![
            row.received_at_ms,
            row.admitted_at_ms,
            row.membership_ms,
            row.origin,
            row.requested_model,
            row.model_kind,
            row.provider,
            row.qualified_model,
            row.target,
            row.agent_provider,
            row.protocol,
            row.outcome,
            row.stage,
            row.reason_code,
            row.input_tokens,
            row.output_tokens,
            row.cache_read_tokens,
            row.cache_write_tokens,
            row.usage_state,
            row.duration_ms,
            row.cost_micros,
            row.price_source,
            row.billing_kind,
            row.cost_state,
        ],
    )
    .map_err(|_| unavailable())?;
    Ok(())
}

fn reading_changed(conn: &Connection, row: &ReadingRow) -> Result<bool> {
    let mut statement = conn
        .prepare(
            "SELECT basis, percent, counters, unit, access_state, reset
             FROM allowance_reading
             WHERE binding = ?1 AND scope_window = ?2 AND source = ?3
             ORDER BY observed_at_ms DESC, id DESC LIMIT 1",
        )
        .map_err(|_| unavailable())?;
    let last = statement.query_row(params![row.binding, row.window, row.source], |result| {
        Ok((
            result.get::<_, String>(0)?,
            result.get::<_, Option<String>>(1)?,
            result.get::<_, Option<String>>(2)?,
            result.get::<_, Option<String>>(3)?,
            result.get::<_, Option<String>>(4)?,
            result.get::<_, String>(5)?,
        ))
    });
    match last {
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(true),
        Err(_) => Err(unavailable()),
        Ok((basis, percent, counters, unit, access_state, reset)) => Ok(basis != row.basis
            || percent != row.percent
            || counters != row.counters
            || unit != row.unit
            || access_state != row.access_state
            || reset != reset_json(&row.reset)),
    }
}

fn reset_json(reset: &serde_json::Value) -> String {
    serde_json::to_string(reset).unwrap_or_else(|_| "null".into())
}

fn insert_reading(conn: &Connection, row: &ReadingRow) -> Result<()> {
    conn.execute(
        "INSERT INTO allowance_reading(
           observed_at_ms, received_at_ms, provider, binding, scope_window, basis,
           percent, counters, unit, source, access_state, reset, freshness)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            row.observed_at_ms,
            row.received_at_ms,
            row.provider,
            row.binding,
            row.window,
            row.basis,
            row.percent,
            row.counters,
            row.unit,
            row.source,
            row.access_state,
            reset_json(&row.reset),
            row.freshness,
        ],
    )
    .map_err(|_| unavailable())?;
    Ok(())
}

#[derive(Debug, Clone, Copy, Default)]
struct Pruned {
    usage: i64,
    readings: i64,
}

fn prune(conn: &Connection) -> Result<Pruned> {
    let cutoff = now_ms() - RETENTION_MS;
    let mut evicted = Pruned {
        usage: conn
            .execute(
                "DELETE FROM usage_record WHERE membership_ms < ?1",
                params![cutoff],
            )
            .map_err(|_| unavailable())? as i64,
        readings: 0,
    };
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM usage_record", [], |row| row.get(0))
        .map_err(|_| unavailable())?;
    if count > USAGE_ROW_LIMIT {
        let excess = count - USAGE_ROW_LIMIT;
        evicted.usage += conn
            .execute(
                "DELETE FROM usage_record WHERE id IN (
                   SELECT id FROM usage_record ORDER BY membership_ms ASC, id ASC LIMIT ?1)",
                params![excess],
            )
            .map_err(|_| unavailable())? as i64;
    }
    let reading_cutoff = now_ms() - READINGS_RETENTION_MS;
    evicted.readings = conn
        .execute(
            "DELETE FROM allowance_reading WHERE observed_at_ms < ?1",
            params![reading_cutoff],
        )
        .map_err(|_| unavailable())? as i64;
    let readings: i64 = conn
        .query_row("SELECT COUNT(*) FROM allowance_reading", [], |row| {
            row.get(0)
        })
        .map_err(|_| unavailable())?;
    if readings > READINGS_ROW_LIMIT {
        evicted.readings += conn
            .execute(
                "DELETE FROM allowance_reading WHERE id IN (
                   SELECT id FROM allowance_reading ORDER BY observed_at_ms ASC, id ASC LIMIT ?1)",
                params![readings - READINGS_ROW_LIMIT],
            )
            .map_err(|_| unavailable())? as i64;
    }
    Ok(evicted)
}

fn append_batch(conn: &mut Connection, jobs: &[WriteJob], extra_dropped: i64) -> Result<()> {
    let transaction = conn.transaction().map_err(|_| unavailable())?;
    for job in jobs {
        match job {
            WriteJob::Usage(row) => insert_usage(&transaction, row)?,
            WriteJob::Reading(row) => {
                if reading_changed(&transaction, row)? {
                    insert_reading(&transaction, row)?;
                }
            }
        }
    }
    let evicted = prune(&transaction)?;
    bump_meta(&transaction, "evicted", evicted.usage)?;
    bump_meta(&transaction, "readings_evicted", evicted.readings)?;
    bump_meta(&transaction, "dropped", extra_dropped)?;
    transaction.commit().map_err(|_| unavailable())
}

/// Append one or more jobs immediately, for explicit actions outside the
/// daemon (paid probes and allowance readings).
pub(crate) fn append_direct(store: &Store, jobs: &[WriteJob]) -> Result<()> {
    // Explicit probe and allowance-reading appends are auxiliary writers. They
    // must not touch the daemon writer's clean-shutdown marker.
    let mut conn = open_write(store)?;
    append_batch(&mut conn, jobs, 0)
}

fn mark_open(conn: &Connection) {
    if meta_get(conn, "open").as_deref() == Some("1") {
        let _ = meta_set(conn, "restart_gap", "1");
    }
    let _ = meta_set(conn, "open", "1");
}

fn mark_closed(conn: &Connection) {
    let _ = meta_set(conn, "open", "0");
}

/// Bounded, non-blocking writer for gateway request capture.
pub(crate) struct UsageWriter {
    sender: Option<SyncSender<WriteJob>>,
    join: Option<JoinHandle<()>>,
    dropped: Arc<AtomicU64>,
}

impl UsageWriter {
    pub(crate) fn start(store: Store) -> Self {
        let (sender, receiver) = sync_channel(CHANNEL_CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let counter = dropped.clone();
        let join = std::thread::Builder::new()
            .name("tmax-usage-ledger".into())
            .spawn(move || writer_loop(store, receiver, counter))
            .ok();
        Self {
            sender: Some(sender),
            join,
            dropped,
        }
    }

    /// Enqueue one capture without blocking and without failing the request.
    pub(crate) fn capture(&self, job: WriteJob) {
        if let Some(sender) = &self.sender
            && sender.try_send(job).is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Drop for UsageWriter {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn writer_loop(store: Store, receiver: Receiver<WriteJob>, dropped: Arc<AtomicU64>) {
    let mut conn: Option<Connection> = None;
    let mut pending_dropped = 0i64;
    loop {
        let first = match receiver.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(job) => job,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Queue-full drops must become durable even when no later
                // request arrives to trigger a flush.
                let lost = dropped.swap(0, Ordering::Relaxed) as i64;
                pending_dropped = pending_dropped.saturating_add(lost);
                if pending_dropped == 0 {
                    continue;
                }
                if conn.is_none() {
                    conn = open_write(&store).ok().inspect(mark_open);
                }
                if let Some(connection) = conn.as_mut() {
                    let extra = std::mem::take(&mut pending_dropped);
                    if append_batch(connection, &[], extra).is_err() {
                        pending_dropped = pending_dropped.saturating_add(extra);
                        conn = None;
                    }
                }
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let mut batch = vec![first];
        for _ in 1..BATCH_LIMIT {
            match receiver.try_recv() {
                Ok(job) => batch.push(job),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        pending_dropped = pending_dropped.saturating_add(dropped.swap(0, Ordering::Relaxed) as i64);
        if conn.is_none() {
            conn = open_write(&store).ok().inspect(|conn| {
                mark_open(conn);
            });
        }
        let extra = std::mem::take(&mut pending_dropped);
        match conn.as_mut() {
            Some(connection) => {
                if append_batch(connection, &batch, extra).is_err() {
                    pending_dropped = pending_dropped.saturating_add(extra + batch.len() as i64);
                    conn = None;
                }
            }
            None => {
                pending_dropped = pending_dropped.saturating_add(extra + batch.len() as i64);
            }
        }
    }
    if let Some(connection) = conn.as_mut() {
        let extra = std::mem::take(&mut pending_dropped)
            .saturating_add(dropped.swap(0, Ordering::Relaxed) as i64);
        if append_batch(connection, &[], extra).is_ok() {
            mark_closed(connection);
        }
    }
}

/// Persist a dropped-capture marker without touching the daemon writer's
/// clean-shutdown marker. `count` saturates at zero.
pub(crate) fn record_dropped(store: &Store, count: i64) -> Result<()> {
    if count <= 0 {
        return Ok(());
    }
    let conn = open_write(store)?;
    bump_meta(&conn, "dropped", count)
}

/// Counts removed by `usage clear`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ClearCounts {
    pub usage_records: i64,
    pub allowance_readings: i64,
}

pub(crate) fn counts(conn: &Connection) -> Result<ClearCounts> {
    Ok(ClearCounts {
        usage_records: conn
            .query_row("SELECT COUNT(*) FROM usage_record", [], |row| row.get(0))
            .map_err(|_| unavailable())?,
        allowance_readings: conn
            .query_row("SELECT COUNT(*) FROM allowance_reading", [], |row| {
                row.get(0)
            })
            .map_err(|_| unavailable())?,
    })
}

/// Delete every retained row and start a new coverage epoch.
pub(crate) fn clear(conn: &mut Connection) -> Result<ClearCounts> {
    let transaction = conn.transaction().map_err(|_| unavailable())?;
    let removed = counts(&transaction)?;
    transaction
        .execute("DELETE FROM usage_record", [])
        .map_err(|_| unavailable())?;
    transaction
        .execute("DELETE FROM allowance_reading", [])
        .map_err(|_| unavailable())?;
    meta_set(&transaction, "evicted", "0")?;
    meta_set(&transaction, "dropped", "0")?;
    meta_set(&transaction, "readings_evicted", "0")?;
    meta_set(&transaction, "readings_dropped", "0")?;
    meta_set(&transaction, "restart_gap", "0")?;
    meta_set(&transaction, "cleared_at_ms", &now_ms().to_string())?;
    transaction.commit().map_err(|_| unavailable())?;
    Ok(removed)
}

/// Epoch markers for coverage output.
pub(crate) struct Epoch {
    pub evicted: i64,
    pub dropped: i64,
    pub readings_evicted: i64,
    pub readings_dropped: i64,
    pub cleared_at_ms: i64,
    pub restart_gap: bool,
}

pub(crate) fn epoch(conn: &Connection) -> Epoch {
    Epoch {
        evicted: meta_i64(conn, "evicted"),
        dropped: meta_i64(conn, "dropped"),
        readings_evicted: meta_i64(conn, "readings_evicted"),
        readings_dropped: meta_i64(conn, "readings_dropped"),
        cleared_at_ms: meta_i64(conn, "cleared_at_ms"),
        restart_gap: meta_get(conn, "restart_gap").as_deref() == Some("1"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{ReadingRow, UsageRow};

    fn store() -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_path_buf());
        (directory, store)
    }

    fn reading(observed_at_ms: i64) -> ReadingRow {
        ReadingRow {
            observed_at_ms,
            received_at_ms: now_ms(),
            provider: "p".into(),
            binding: "b".into(),
            window: "w".into(),
            basis: "consumed_percent".into(),
            percent: Some("10.0".into()),
            counters: None,
            unit: None,
            source: "go_usage_api".into(),
            access_state: Some("available".into()),
            reset: serde_json::Value::Null,
            freshness: None,
        }
    }

    fn usage(membership_ms: i64) -> UsageRow {
        UsageRow {
            received_at_ms: membership_ms,
            admitted_at_ms: Some(membership_ms),
            membership_ms,
            origin: "client".into(),
            model_kind: "concrete".into(),
            outcome: "completed".into(),
            stage: "upstream_response".into(),
            usage_state: "unknown".into(),
            billing_kind: "unknown".into(),
            cost_state: "none".into(),
            ..Default::default()
        }
    }

    #[test]
    fn reading_age_pruning_is_counted_separately_and_cleared_with_the_epoch() {
        let (_directory, store) = store();
        append_direct(
            &store,
            &[WriteJob::Reading(reading(
                now_ms() - READINGS_RETENTION_MS - 1_000,
            ))],
        )
        .unwrap();
        // A later batch runs retention and evicts the aged reading.
        append_direct(&store, &[WriteJob::Usage(usage(now_ms()))]).unwrap();
        let conn = open_read(&store).unwrap().expect("ledger");
        assert_eq!(meta_i64(&conn, "readings_evicted"), 1);
        let markers = epoch(&conn);
        assert_eq!(markers.readings_evicted, 1);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM allowance_reading", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            0
        );
        drop(conn);

        let mut conn = open_write(&store).unwrap();
        meta_set(&conn, "readings_dropped", "3").unwrap();
        clear(&mut conn).unwrap();
        let markers = epoch(&conn);
        assert_eq!(markers.evicted, 0);
        assert_eq!(markers.dropped, 0);
        assert_eq!(markers.readings_evicted, 0);
        assert_eq!(markers.readings_dropped, 0);
        assert_eq!(meta_i64(&conn, "readings_dropped"), 0);
    }

    #[test]
    fn direct_appends_do_not_create_restart_gap_markers() {
        let (_directory, store) = store();
        append_direct(&store, &[WriteJob::Usage(usage(now_ms()))]).unwrap();
        append_direct(
            &store,
            &[WriteJob::Reading(reading(
                now_ms() - READINGS_RETENTION_MS - 1_000,
            ))],
        )
        .unwrap();
        let conn = open_read(&store).unwrap().expect("ledger");
        assert_eq!(meta_get(&conn, "open").as_deref(), None);
        assert!(!epoch(&conn).restart_gap);
    }
}
