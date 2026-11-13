//! Bounded provenance and repeat-detection records.
//!
//! Records live under tmax-owned runtime storage, never in a native Agent store.
//! A malformed or unsupported record store is reported as unavailable; it never
//! blocks a native import because the destination session remains authoritative.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Error, Result, storage::Store};

const RECORD_FILE: &str = "session-imports.json";
const RECORD_VERSION: i64 = 1;
const MAX_RECORDS: usize = 500;
const MAX_SOURCE_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImportRecord {
    pub(crate) source_store: String,
    pub(crate) source_native_id: String,
    pub(crate) source_revision: String,
    pub(crate) destination_store: String,
    pub(crate) destination_native_id: String,
    pub(crate) source_target: String,
    pub(crate) destination_target: String,
    pub(crate) source_schema: String,
    pub(crate) destination_schema: String,
    pub(crate) title: String,
    pub(crate) source_cwd: Option<String>,
    pub(crate) destination_cwd: Option<String>,
    pub(crate) workspace_roots_omitted: u64,
    pub(crate) counts: Value,
    pub(crate) imported_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordFile {
    version: i64,
    #[serde(default)]
    records: Vec<ImportRecord>,
}

#[derive(Clone, Debug)]
pub(crate) struct Records {
    pub(crate) records: Vec<ImportRecord>,
    pub(crate) state: RecordState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordState {
    Available,
    Empty,
    Unavailable,
}

impl Records {
    pub(crate) fn load(store: &Store) -> Self {
        let bytes = match std::fs::read(store.root.join(RECORD_FILE)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Self {
                    records: Vec::new(),
                    state: RecordState::Empty,
                };
            }
            Err(_) => {
                return Self {
                    records: Vec::new(),
                    state: RecordState::Unavailable,
                };
            }
        };
        if bytes.len() > MAX_SOURCE_BYTES {
            return Self {
                records: Vec::new(),
                state: RecordState::Unavailable,
            };
        }
        let file: RecordFile = match serde_json::from_slice::<RecordFile>(&bytes) {
            Ok(file) if file.version == RECORD_VERSION => file,
            _ => {
                return Self {
                    records: Vec::new(),
                    state: RecordState::Unavailable,
                };
            }
        };
        Self {
            state: if file.records.is_empty() {
                RecordState::Empty
            } else {
                RecordState::Available
            },
            records: file.records,
        }
    }

    pub(crate) fn imported_from(&self, store: &str, native_id: &str) -> Option<Value> {
        self.records
            .iter()
            .find(|record| {
                record.destination_store == store && record.destination_native_id == native_id
            })
            .map(|record| {
                json!({
                    "target": record.source_target,
                    "native_id": record.source_native_id,
                })
            })
    }

    pub(crate) fn matching<'a>(
        &'a self,
        source_store: &'a str,
        source_native_id: &'a str,
        destination_store: &'a str,
    ) -> impl Iterator<Item = &'a ImportRecord> + 'a {
        self.records.iter().filter(move |record| {
            record.source_store == source_store
                && record.source_native_id == source_native_id
                && record.destination_store == destination_store
        })
    }

    pub(crate) fn insert(&mut self, record: ImportRecord) -> bool {
        self.records.retain(|item| {
            !(item.source_store == record.source_store
                && item.source_native_id == record.source_native_id
                && item.destination_store == record.destination_store
                && item.destination_native_id == record.destination_native_id)
        });
        self.records.push(record);
        self.records
            .sort_by(|a, b| b.imported_at.cmp(&a.imported_at));
        let pruned = self.records.len() > MAX_RECORDS;
        if pruned {
            self.records.truncate(MAX_RECORDS);
        }
        pruned
    }
}

pub(crate) fn save(store: &Store, records: &Records) -> Result<bool> {
    let mut retained = records.records.clone();
    let bytes = loop {
        let file = RecordFile {
            version: RECORD_VERSION,
            records: retained.clone(),
        };
        let bytes = serde_json::to_vec(&file).map_err(|_| {
            Error::new(
                "record_failed",
                "Import provenance record could not be encoded",
            )
        })?;
        if bytes.len() <= MAX_SOURCE_BYTES || retained.is_empty() {
            break bytes;
        }
        retained.pop();
    };
    let pruned = retained.len() < records.records.len();
    store.write_raw(RECORD_FILE, &bytes).map_err(|_| {
        Error::new(
            "record_failed",
            "Import provenance record could not be written",
        )
    })?;
    Ok(pruned)
}
