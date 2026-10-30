//! Append-only JSONL error logging with one size-rotated backup.
use crate::{
    Error, Result,
    diagnostics::Record,
    storage::{Store, open_append, replace_file, storage_error},
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;
use std::{borrow::Cow, io::Write};

pub const DETAIL_BYTES: usize = 64 * 1024;
pub const ERROR_DETAIL_LIMIT: usize = 16;
pub const DETAIL_IDENTIFIER_BYTES: usize = 512;
const MESSAGE_BYTES: usize = 512;
const FILE: &str = "error.log";
const PREVIOUS: &str = "error.log.1";
const FILE_BYTES: usize = 4 * 1024 * 1024;
const LOCK: &str = "error-log";

#[derive(Clone)]
pub struct ErrorDetail {
    pub time: DateTime<Utc>,
    pub attempt: Option<usize>,
    pub stage: String,
    pub provider: Option<String>,
    pub qualified_model: Option<String>,
    pub upstream_status: Option<u16>,
    pub request_id: Option<String>,
    pub request_id_truncated: bool,
    pub error: String,
    pub truncated: bool,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ErrorBody<'a> {
    Json(Value),
    Text(&'a str),
}

#[derive(Serialize)]
struct LoggedError<'a> {
    time: String,
    attempt: Option<usize>,
    provider: Option<&'a str>,
    model: Option<&'a str>,
    stage: &'a str,
    http_status: Option<u16>,
    upstream_request_id: Option<&'a str>,
    outcome: &'static str,
    error_message: Cow<'a, str>,
    error_details: ErrorBody<'a>,
    truncated: bool,
}

#[derive(Serialize)]
struct ErrorEvent<'a> {
    time: String,
    level: &'static str,
    event: &'static str,
    request_id: &'a str,
    provider: Option<&'a str>,
    model: Option<&'a str>,
    protocol: Option<&'a str>,
    stage: &'a str,
    http_status: Option<u16>,
    outcome: &'a str,
    reason_code: Option<&'a str>,
    duration_ms: Option<u64>,
    upstream_request_id: Option<&'a str>,
    error_message: &'a str,
    errors: &'a [LoggedError<'a>],
    truncated: bool,
    evidence_gap: bool,
}

fn terminal_event(outcome: &str) -> (&'static str, &'static str, &'static str) {
    match outcome {
        "failed" => ("error", "gateway.request_failed", "Request failed"),
        "completed" => (
            "warn",
            "gateway.request_recovered",
            "Request completed after earlier upstream failures",
        ),
        "rejected" => ("warn", "gateway.request_rejected", "Request rejected"),
        "cancelled" => ("info", "gateway.request_cancelled", "Request cancelled"),
        "interrupted" => ("warn", "gateway.request_interrupted", "Request interrupted"),
        _ => (
            "warn",
            "gateway.request_unknown",
            "Request completion unknown",
        ),
    }
}

fn prefix(value: &str, max: usize) -> (&str, bool) {
    if value.len() <= max {
        return (value, false);
    }
    let mut end = max;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (&value[..end], true)
}
fn bounded<'a>(value: &'a str, limit: usize, truncated: &mut bool) -> &'a str {
    let (text, cut) = prefix(value, limit);
    *truncated |= cut;
    text
}

fn identifier<'a>(value: Option<&'a str>, truncated: &mut bool) -> Option<&'a str> {
    value.map(|value| bounded(value, DETAIL_IDENTIFIER_BYTES, truncated))
}

fn describe_error(text: &str, capture_truncated: bool) -> (Cow<'_, str>, ErrorBody<'_>, bool) {
    let value = if capture_truncated {
        None
    } else {
        serde_json::from_str::<Value>(text).ok()
    };
    match value {
        Some(value) => {
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| value.pointer("/error/message").and_then(Value::as_str))
                .or_else(|| {
                    value
                        .pointer("/response/error/message")
                        .and_then(Value::as_str)
                })
                .or_else(|| value.get("error").and_then(Value::as_str))
                .or_else(|| value.as_str())
                .unwrap_or("Upstream returned a JSON error without a message");
            let (message, cut) = prefix(message, MESSAGE_BYTES);
            let message = message.to_owned();
            (Cow::Owned(message), ErrorBody::Json(value), cut)
        }
        None => {
            let (message, cut) = prefix(text, MESSAGE_BYTES);
            (
                Cow::Borrowed(if message.is_empty() {
                    "Error text unavailable"
                } else {
                    message
                }),
                ErrorBody::Text(text),
                cut,
            )
        }
    }
}

fn bounded_details(errors: &[ErrorDetail]) -> Vec<LoggedError<'_>> {
    let mut remaining = DETAIL_BYTES;
    let mut result = Vec::with_capacity(errors.len().min(ERROR_DETAIL_LIMIT));
    for source in errors.iter().take(ERROR_DETAIL_LIMIT) {
        let mut capture_truncated = source.truncated;
        let text = bounded(&source.error, remaining, &mut capture_truncated);
        remaining -= text.len();
        let (error_message, error_details, message_cut) = describe_error(text, capture_truncated);
        let mut truncated = capture_truncated || source.request_id_truncated || message_cut;
        result.push(LoggedError {
            time: source.time.to_rfc3339_opts(SecondsFormat::Millis, true),
            attempt: source.attempt,
            provider: identifier(source.provider.as_deref(), &mut truncated),
            model: identifier(source.qualified_model.as_deref(), &mut truncated),
            stage: bounded(&source.stage, DETAIL_IDENTIFIER_BYTES, &mut truncated),
            http_status: source.upstream_status,
            upstream_request_id: identifier(source.request_id.as_deref(), &mut truncated),
            outcome: "failed",
            error_message,
            error_details,
            truncated,
        });
    }
    if errors.len() > ERROR_DETAIL_LIMIT
        && let Some(last) = result.last_mut()
    {
        last.truncated = true;
    }
    result
}

/// Append one error event; rotate by rename before the active file exceeds its limit.
pub fn append(store: &Store, record: &Record, errors: &[ErrorDetail]) -> Result<()> {
    if record.outcome == "completed" && errors.is_empty() {
        return Ok(());
    }
    let details = bounded_details(errors);
    let mut truncated = details.iter().any(|error| error.truncated);
    let terminal_error = if record.outcome == "failed" {
        errors
            .iter()
            .take(details.len())
            .rposition(|error| {
                error.provider == record.provider && error.qualified_model == record.qualified_model
            })
            .map(|index| &details[index])
    } else {
        None
    };
    let (level, event, message) = terminal_event(&record.outcome);
    let entry = ErrorEvent {
        time: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        level,
        event,
        request_id: bounded(&record.request, DETAIL_IDENTIFIER_BYTES, &mut truncated),
        provider: identifier(record.provider.as_deref(), &mut truncated),
        model: identifier(record.qualified_model.as_deref(), &mut truncated),
        protocol: identifier(record.protocol.as_deref(), &mut truncated),
        stage: bounded(&record.stage, DETAIL_IDENTIFIER_BYTES, &mut truncated),
        http_status: record.upstream_status,
        outcome: bounded(&record.outcome, DETAIL_IDENTIFIER_BYTES, &mut truncated),
        reason_code: identifier(record.reason_code.as_deref(), &mut truncated),
        duration_ms: record.duration_ms,
        upstream_request_id: terminal_error.and_then(|error| error.upstream_request_id),
        error_message: terminal_error
            .map(|error| error.error_message.as_ref())
            .unwrap_or(message),
        errors: &details,
        truncated,
        evidence_gap: record.evidence_gap || truncated,
    };
    let mut bytes = serde_json::to_vec(&entry).map_err(|_| storage_error())?;
    bytes.push(b'\n');
    if bytes.len() > FILE_BYTES {
        return Err(Error::new(
            "storage_limit_exceeded",
            "Error event exceeds the log file size limit",
        ));
    }
    let _lock = store.try_lock(LOCK)?.ok_or_else(|| {
        Error::new(
            "error_log_busy",
            "Error log is busy; event was not retained",
        )
    })?;
    let path = store.path(FILE)?;
    let mut file = open_append(&path)?;
    let mut size = file.metadata().map_err(|_| storage_error())?.len();
    if size.saturating_add(bytes.len() as u64) > FILE_BYTES as u64 {
        drop(file);
        replace_file(&path, &store.path(PREVIOUS)?)?;
        file = open_append(&path)?;
        size = 0;
    }
    if file.write_all(&bytes).is_err() {
        // Best effort: retain earlier complete rows if a write stopped mid-event.
        let _ = file.set_len(size);
        return Err(storage_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::History;
    use serde_json::json;
    use std::{fs, io::Read};

    fn detail(text: &str) -> ErrorDetail {
        ErrorDetail {
            time: Utc::now(),
            attempt: Some(1),
            stage: "upstream_response".into(),
            provider: Some("provider".into()),
            qualified_model: Some("provider/model".into()),
            upstream_status: Some(429),
            request_id: Some("upstream-id".into()),
            request_id_truncated: false,
            error: text.into(),
            truncated: false,
        }
    }
    fn failed_record() -> Record {
        let mut record = History::new("p".into()).begin();
        record.outcome = "failed".into();
        record.stage = "upstream_response".into();
        record.provider = Some("provider".into());
        record.qualified_model = Some("provider/model".into());
        record.upstream_status = Some(429);
        record
    }
    fn entries(store: &Store, name: &str) -> Vec<Value> {
        fs::read_to_string(store.path(name).unwrap())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn truncated_json_capture_remains_text_even_if_the_retained_prefix_parses() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().into());
        let text = r#"{"message":"available prefix"}"#;
        let mut error = detail(text);
        error.truncated = true;
        append(&store, &failed_record(), &[error]).unwrap();
        let event = entries(&store, FILE).pop().unwrap();
        assert_eq!(event["errors"][0]["error_details"], text);
        assert_eq!(event["errors"][0]["truncated"], true);
    }

    #[test]
    fn truncated_request_id_preserves_complete_json_body() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().into());
        let mut error = detail(r#"{"message":"complete","code":17}"#);
        error.request_id_truncated = true;
        append(&store, &failed_record(), &[error]).unwrap();
        let event = entries(&store, FILE).pop().unwrap();
        assert_eq!(
            event["errors"][0]["error_details"],
            json!({"message":"complete","code":17})
        );
        assert_eq!(event["errors"][0]["truncated"], true);
        assert_eq!(event["truncated"], true);
        assert_eq!(event["evidence_gap"], true);
    }

    #[test]
    fn held_lock_reports_busy_without_appending() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().into());
        append(&store, &failed_record(), &[detail("first")]).unwrap();
        let path = store.path(FILE).unwrap();
        let before = fs::read(&path).unwrap();
        let lock = store.lock(LOCK).unwrap();
        let error = append(&store, &failed_record(), &[detail("blocked")]).unwrap_err();
        assert_eq!(error.code, "error_log_busy");
        assert_eq!(fs::read(&path).unwrap(), before);
        drop(lock);
        append(&store, &failed_record(), &[detail("next")]).unwrap();
        let retained = entries(&store, FILE);
        assert_eq!(retained.len(), 2);
        assert_eq!(retained[0]["errors"][0]["error_details"], "first");
        assert_eq!(retained[1]["errors"][0]["error_details"], "next");
    }

    #[test]
    fn readers_see_new_json_lines_without_reopening_or_replacing_the_file() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().into());
        let mut record = failed_record();
        record.outcome = "completed".into();
        append(&store, &record, &[detail("upstream refused\nwith details")]).unwrap();
        let path = store.path(FILE).unwrap();
        let mut reader = fs::File::open(&path).unwrap();
        let mut first = Vec::new();
        reader.read_to_end(&mut first).unwrap();
        assert_eq!(first.iter().filter(|&&byte| byte == b'\n').count(), 1);
        record.request = "second-request".into();
        append(&store, &record, &[detail("second failure")]).unwrap();
        let mut appended = Vec::new();
        reader.read_to_end(&mut appended).unwrap();
        let second: Value = serde_json::from_slice(&appended).unwrap();
        assert_eq!(second["request_id"], "second-request");
        assert_eq!(second["errors"][0]["error_details"], "second failure");
        first.extend_from_slice(&appended);
        assert_eq!(fs::read(&path).unwrap(), first);
        let reopened = Store::at(home.path().into());
        let retained = entries(&reopened, FILE);
        assert_eq!(retained[0]["outcome"], "completed");
        assert_eq!(
            retained[0]["errors"][0]["error_details"],
            "upstream refused\nwith details"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn errors_are_directly_readable_without_losing_json_types_or_text() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().into());
        let record = failed_record();
        for (raw, message, expected) in [
            (
                r#"{"error":{"message":"Bad arguments","code":17,"retryable":false}}"#,
                "Bad arguments",
                json!({"error":{"message":"Bad arguments","code":17,"retryable":false}}),
            ),
            (
                r#"{"response":{"error":{"message":"Nested stream error"}}}"#,
                "Nested stream error",
                json!({"response":{"error":{"message":"Nested stream error"}}}),
            ),
            (
                "plain error\nwith details",
                "plain error\nwith details",
                json!("plain error\nwith details"),
            ),
            (
                "{\"message\":\"unfinished",
                "{\"message\":\"unfinished",
                json!("{\"message\":\"unfinished"),
            ),
        ] {
            append(&store, &record, &[detail(raw)]).unwrap();
            let retained = entries(&store, FILE);
            let event = retained.last().unwrap();
            assert_eq!(event["error_message"], message);
            assert_eq!(event["errors"][0]["error_message"], message);
            assert_eq!(event["errors"][0]["error_details"], expected);
        }
        append(
            &store,
            &record,
            &[detail(r#"{"vendor_specific":{"code":17}}"#)],
        )
        .unwrap();
        let retained = entries(&store, FILE);
        assert_eq!(
            retained.last().unwrap()["errors"][0]["error_details"],
            json!({"vendor_specific":{"code":17}})
        );
    }

    #[test]
    fn terminal_levels_and_events_distinguish_recovered_cancelled_and_failed() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().into());
        let mut record = failed_record();
        record.received_at = DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        record.decision =
            json!({"priority_evaluations":[{"large_snapshot":"not-log-content".repeat(10000)}]});
        for (outcome, level, event) in [
            ("failed", "error", "gateway.request_failed"),
            ("completed", "warn", "gateway.request_recovered"),
            ("cancelled", "info", "gateway.request_cancelled"),
            ("rejected", "warn", "gateway.request_rejected"),
            ("interrupted", "warn", "gateway.request_interrupted"),
            ("unknown", "warn", "gateway.request_unknown"),
        ] {
            record.outcome = outcome.into();
            append(&store, &record, &[detail("upstream failed")]).unwrap();
            let retained = entries(&store, FILE);
            let logged = retained.last().unwrap();
            assert_eq!(logged["level"], level);
            assert_eq!(logged["event"], event);
            assert_eq!(logged["outcome"], outcome);
            assert_eq!(logged["errors"][0]["outcome"], "failed");
            let logged_time =
                DateTime::parse_from_rfc3339(logged["time"].as_str().unwrap()).unwrap();
            assert!(logged_time > record.received_at);
            assert!(logged.get("record").is_none());
            assert!(logged.get("decision").is_none());
            assert!(!logged.to_string().contains("not-log-content"));
        }
        record.outcome = "completed".into();
        let untouched = tempfile::tempdir().unwrap();
        let untouched_store = Store::at(untouched.path().into());
        append(&untouched_store, &record, &[]).unwrap();
        assert!(!untouched_store.root.exists());
    }

    #[test]
    fn failed_fallback_details_do_not_replace_the_final_request_error() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().into());
        let mut record = failed_record();
        record.provider = Some("different-final-provider".into());
        record.qualified_model = Some("different-final-provider/model".into());
        append(&store, &record, &[detail("earlier provider error")]).unwrap();
        let logged = entries(&store, FILE).pop().unwrap();
        assert_ne!(logged["error_message"], "earlier provider error");
        assert!(logged["upstream_request_id"].is_null());
        assert_eq!(logged["errors"][0]["upstream_request_id"], "upstream-id");
    }

    #[test]
    fn truncation_bounds_messages_and_identifiers_but_retains_raw_evidence() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().into());
        let mut record = failed_record();
        record.provider = Some("p".repeat(10000));
        let errors = vec![detail(&"界".repeat(DETAIL_BYTES)); ERROR_DETAIL_LIMIT + 1];
        append(&store, &record, &errors).unwrap();
        let event = entries(&store, FILE).pop().unwrap();
        assert_eq!(event["truncated"], true);
        assert_eq!(event["evidence_gap"], true);
        assert_eq!(
            event["provider"].as_str().unwrap().len(),
            DETAIL_IDENTIFIER_BYTES
        );
        let errors = event["errors"].as_array().unwrap();
        assert_eq!(errors.len(), ERROR_DETAIL_LIMIT);
        assert!(errors[0]["error_message"].as_str().unwrap().len() <= MESSAGE_BYTES);
        let raw = errors[0]["error_details"].as_str().unwrap();
        assert_eq!(raw.len(), DETAIL_BYTES - DETAIL_BYTES % "界".len());
        assert!(raw.chars().all(|character| character == '界'));
        assert!(errors.iter().all(|error| error["truncated"] == true));
    }

    #[test]
    fn rotates_only_when_next_line_exceeds_limit_and_replaces_old_backup() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().into());
        let mut record = failed_record();
        append(&store, &record, &[]).unwrap();
        let path = store.path(FILE).unwrap();
        let first = fs::read(&path).unwrap();
        // Fixed-width millisecond timestamps keep this exact real size boundary deterministic.
        let padding = vec![b' '; FILE_BYTES - 2 * first.len()];
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&padding)
            .unwrap();
        append(&store, &record, &[]).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), FILE_BYTES as u64);
        assert!(!store.path(PREVIOUS).unwrap().exists());
        append(&store, &record, &[]).unwrap();
        let previous = store.path(PREVIOUS).unwrap();
        assert_eq!(fs::metadata(&previous).unwrap().len(), FILE_BYTES as u64);
        assert_eq!(fs::read(&path).unwrap().len(), first.len());
        assert_eq!(entries(&store, PREVIOUS).len(), 2);
        record.request = "next-generation".into();
        append(&store, &record, &[]).unwrap();
        let second_generation = fs::read(&path).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(FILE_BYTES as u64)
            .unwrap();
        append(&store, &record, &[]).unwrap();
        let backup = fs::read(&previous).unwrap();
        assert!(backup.starts_with(&second_generation));
        assert_eq!(backup.len(), FILE_BYTES);
        assert_eq!(entries(&store, FILE)[0]["request_id"], "next-generation");
        assert!(
            fs::metadata(&path).unwrap().len() + fs::metadata(&previous).unwrap().len()
                <= (2 * FILE_BYTES) as u64
        );
    }

    #[test]
    fn existing_log_content_is_not_parsed_or_rewritten() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().into());
        let prefix = b"unparseable previous log line\n";
        store.write_raw(FILE, prefix).unwrap();
        append(&store, &failed_record(), &[detail("new failure")]).unwrap();
        let bytes = fs::read(store.path(FILE).unwrap()).unwrap();
        assert_eq!(&bytes[..prefix.len()], prefix);
        let last: Value = serde_json::from_slice(&bytes[prefix.len()..]).unwrap();
        assert_eq!(last["error_message"], "new failure");
    }

    #[test]
    fn concurrent_writers_append_complete_distinct_lines() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().into());
        let threads: Vec<_> = (0..4)
            .map(|i| {
                let store = store.clone();
                std::thread::spawn(move || {
                    let history = History::new(format!("p{i}"));
                    (0..8)
                        .map(|_| {
                            let mut record = history.begin();
                            record.outcome = "failed".into();
                            let result = append(&store, &record, &[]);
                            (record.request, result)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut successful = std::collections::BTreeSet::new();
        for thread in threads {
            for (request, result) in thread.join().unwrap() {
                match result {
                    Ok(()) => assert!(successful.insert(request)),
                    Err(error) => assert_eq!(error.code, "error_log_busy"),
                }
            }
        }
        let retained = entries(&store, FILE);
        let requests: std::collections::BTreeSet<_> = retained
            .iter()
            .map(|entry| entry["request_id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(retained.len(), requests.len());
        assert_eq!(requests, successful);
    }

    #[cfg(unix)]
    #[test]
    fn partial_write_rolls_back_without_damaging_other_rows() {
        use std::io::{Seek, SeekFrom};
        const CHILD: &str = "TMAX_ERROR_LOG_PARTIAL_WRITE_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg(
                    "diagnostics::error_log::tests::partial_write_rolls_back_without_damaging_other_rows",
                )
                .arg("--nocapture")
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "child failed: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
            return;
        }

        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().into());
        let mut record = failed_record();
        record.request = "first".into();
        append(&store, &record, &[]).unwrap();
        let path = store.path(FILE).unwrap();
        let first = fs::read(&path).unwrap();

        // Resource limits and signal disposition are process-wide, so this runs only
        // in the isolated child. The probe proves the limit causes partial writes.
        let mut original = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        unsafe {
            assert_eq!(libc::getrlimit(libc::RLIMIT_FSIZE, &mut original), 0);
            assert_ne!(libc::signal(libc::SIGXFSZ, libc::SIG_IGN), libc::SIG_ERR);
        }
        let limit = libc::rlimit {
            rlim_cur: (first.len() + 64) as libc::rlim_t,
            rlim_max: original.rlim_max,
        };
        assert!(limit.rlim_cur <= original.rlim_max);
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &limit) }, 0);
        let probe_path = home.path().join("partial-write-probe");
        let mut probe = fs::File::create(&probe_path).unwrap();
        probe.set_len(first.len() as u64).unwrap();
        probe.seek(SeekFrom::End(0)).unwrap();
        assert!(probe.write_all(&[b'x'; 128]).is_err());
        assert_eq!(probe.metadata().unwrap().len(), first.len() as u64 + 64);

        record.request = "failed-partial".into();
        let error = append(&store, &record, &[detail(&"x".repeat(256))]).unwrap_err();
        assert_eq!(error.code, "storage_unavailable");
        assert_eq!(fs::read(&path).unwrap(), first);
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &original) }, 0);
        record.request = "next".into();
        append(&store, &record, &[]).unwrap();
        let retained = entries(&store, FILE);
        assert_eq!(retained.len(), 2);
        assert_eq!(retained[0]["request_id"], "first");
        assert_eq!(retained[1]["request_id"], "next");

        // A failed first append after rotation must leave the new active file empty.
        let before_rotation = fs::read(&path).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(FILE_BYTES as u64)
            .unwrap();
        let limit = libc::rlimit {
            rlim_cur: 64,
            rlim_max: original.rlim_max,
        };
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &limit) }, 0);
        record.request = "failed-after-rotation".into();
        let error = append(&store, &record, &[detail(&"x".repeat(256))]).unwrap_err();
        assert_eq!(error.code, "storage_unavailable");
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        let backup = fs::read(store.path(PREVIOUS).unwrap()).unwrap();
        assert!(backup.starts_with(&before_rotation));
        assert_eq!(backup.len(), FILE_BYTES);
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &original) }, 0);
        record.request = "after-rotation".into();
        append(&store, &record, &[]).unwrap();
        let retained = entries(&store, FILE);
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0]["request_id"], "after-rotation");
    }
}
