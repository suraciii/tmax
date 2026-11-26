//! Best-effort append of dated allowance readings published by observations.
use crate::{Error, Result, storage::Store, usage::ReadingRow};
use sha2::{Digest, Sha256};

/// Opaque, stable handle for one observed authorization incarnation.
///
/// Only local opaque identity tokens (a provider-incarnation uuid and an
/// authorization-generation uuid) enter the digest, behind a domain prefix. No
/// label, scope, endpoint, contract, credential, or upstream identity is
/// included, so the handle stays stable across renames and scopes and changes
/// exactly when the authorization incarnation or generation changes.
pub(crate) fn binding_handle(provider_identity: &str, authorization_generation: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"tmax.usage.allowance_reading_binding.v1");
    digest.update([0]);
    digest.update(provider_identity.as_bytes());
    digest.update([0]);
    digest.update(authorization_generation.as_bytes());
    format!("{:x}", digest.finalize())
}

/// Append one publication's readings outside the daemon writer.
///
/// The rows go through one bounded direct transaction, so a multi-window
/// observation never holds several write transactions open and value-based
/// dedupe skips every unchanged row. A failed append is reported to the caller,
/// which must count it as a coverage gap and never fail the observation that
/// produced it.
pub(crate) fn append(store: &Store, rows: &[ReadingRow]) -> Result<()> {
    let jobs = rows
        .iter()
        .cloned()
        .map(crate::usage::WriteJob::Reading)
        .collect::<Vec<_>>();
    crate::usage::ledger::append_direct(store, &jobs)
}

/// Record readings that could not be appended as a reading coverage gap.
///
/// The counter lives in the usage store's `meta` table beside the ledger
/// counters, so a report can state the gap without reading a second store.
pub(crate) fn record_dropped(store: &Store, count: i64) -> Result<()> {
    if count <= 0 {
        return Ok(());
    }
    let conn = crate::usage::ledger::open_write(store)?;
    let next = crate::usage::ledger::meta_i64(&conn, "readings_dropped")
        .saturating_add(count)
        .max(0);
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('readings_dropped', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![next.to_string()],
    )
    .map_err(|_| Error::new("storage_unavailable", "The usage ledger is unavailable"))?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn test_rows(store: &Store) -> Vec<ReadingRow> {
    let Ok(Some(conn)) = crate::usage::ledger::open_read(store) else {
        return Vec::new();
    };
    let mut statement = conn
        .prepare(
            "SELECT observed_at_ms, received_at_ms, provider, binding, scope_window, basis,
                    percent, counters, unit, source, access_state, reset, freshness
             FROM allowance_reading ORDER BY id",
        )
        .expect("readings statement");
    statement
        .query_map([], |row| {
            Ok(ReadingRow {
                observed_at_ms: row.get(0)?,
                received_at_ms: row.get(1)?,
                provider: row.get(2)?,
                binding: row.get(3)?,
                window: row.get(4)?,
                basis: row.get(5)?,
                percent: row.get(6)?,
                counters: row.get(7)?,
                unit: row.get(8)?,
                source: row.get(9)?,
                access_state: row.get(10)?,
                reset: serde_json::from_str(&row.get::<_, String>(11)?)
                    .unwrap_or(serde_json::Value::Null),
                freshness: row.get(12)?,
            })
        })
        .expect("reading rows")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("valid reading rows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_handle_is_domain_separated_and_stable_across_coordinates() {
        let handle = binding_handle("identity", "generation");
        assert_eq!(handle.len(), 64);
        assert!(handle.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(handle, binding_handle("identity", "generation"));
        assert_ne!(handle, binding_handle("identity", "replacement"));
        assert_ne!(handle, binding_handle("recreated", "generation"));
        // A concatenation that would collide without domain separation differs.
        assert_ne!(binding_handle("ab", "c"), binding_handle("a", "bc"),);
    }

    #[test]
    fn dropped_counter_is_persisted_and_saturates() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_path_buf());
        record_dropped(&store, 2).unwrap();
        record_dropped(&store, 3).unwrap();
        let conn = crate::usage::ledger::open_read(&store)
            .unwrap()
            .expect("created ledger");
        assert_eq!(crate::usage::ledger::meta_i64(&conn, "readings_dropped"), 5);
        record_dropped(&store, 0).unwrap();
        assert_eq!(crate::usage::ledger::meta_i64(&conn, "readings_dropped"), 5);
    }
}
