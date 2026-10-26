use super::{BINDING_LIMIT, Bindings};
use crate::{Error, Result, storage::Store};
use serde_json::Value;

const BINDING_FILE_BYTES: u64 = 64 * 1024 * 1024;

fn binding_schema_error() -> Error {
    Error::new(
        "storage_unavailable",
        "Stored session bindings are malformed; preserve them and repair the affected store",
    )
}

// Only retained operational data accepts historical names. Normalize before
// projection so saved blockers and original routing evidence remain authoritative.
fn normalize_binding_evidence(value: &mut Value) -> Result<()> {
    let Some(object) = value.as_object_mut() else {
        return Ok(());
    };
    for (old, new) in [
        ("offering_identity", "model_identity"),
        ("offering", "qualified_model"),
    ] {
        if let Some(value) = object.remove(old) {
            if object.contains_key(new) {
                return Err(binding_schema_error());
            }
            object.insert(new.into(), value);
        }
    }
    for key in ["reason_code", "selection_reason", "tie_break"] {
        if let Some(value) = object.get_mut(key) {
            normalize_binding_code(value);
        }
    }
    // Traverse known routing records only, never maps keyed by model IDs,
    // metadata, user configuration, or arbitrary source payloads.
    for key in [
        "selection",
        "selected",
        "original_decision",
        "decision",
        "selected_evaluation",
    ] {
        if let Some(value) = object.get_mut(key) {
            normalize_binding_evidence(value)?;
        }
    }
    for key in [
        "priority_evaluations",
        "excluded_candidates",
        "evaluations",
        "excluded",
        "candidates",
    ] {
        if let Some(values) = object.get_mut(key).and_then(Value::as_array_mut) {
            for value in values {
                normalize_binding_evidence(value)?;
            }
        }
    }
    Ok(())
}

fn normalize_binding_code(value: &mut Value) {
    let replacement = match value.as_str() {
        Some("offering_removed") => Some("model_removed"),
        Some("offering_disabled") => Some("model_disabled"),
        Some("stable_offering_identity") => Some("stable_model_identity"),
        _ => None,
    };
    if let Some(replacement) = replacement {
        *value = Value::String(replacement.into());
    }
}

fn decode_bindings(bytes: &[u8]) -> Result<Bindings> {
    // Validate before Value deserialization can discard duplicate evidence fields,
    // including historical evidence inside an otherwise canonical binding.
    crate::inputs::check_historical_field_uniqueness(bytes).map_err(|_| binding_schema_error())?;
    if let Ok(mut bindings) = serde_json::from_slice::<Bindings>(bytes) {
        for binding in bindings.entries.values_mut() {
            normalize_binding_evidence(&mut binding.decision)?;
            if let Some(blocker) = &mut binding.blocked {
                match blocker.as_str() {
                    "offering_removed" => *blocker = "model_removed".into(),
                    "offering_disabled" => *blocker = "model_disabled".into(),
                    _ => {}
                }
            }
        }
        return Ok(bindings);
    }
    let mut document: Value = serde_json::from_slice(bytes).map_err(|_| binding_schema_error())?;
    let entries = document
        .get_mut("entries")
        .and_then(Value::as_object_mut)
        .ok_or_else(binding_schema_error)?;
    for entry in entries.values_mut() {
        let entry = entry.as_object_mut().ok_or_else(binding_schema_error)?;
        if let Some(identity) = entry.remove("offering_identity") {
            if entry.contains_key("model_identity") {
                return Err(binding_schema_error());
            }
            entry.insert("model_identity".into(), identity);
        }
        if let Some(blocker) = entry.get_mut("blocked") {
            normalize_binding_code(blocker);
        }
        if let Some(decision) = entry.get_mut("decision") {
            normalize_binding_evidence(decision)?;
        }
    }
    serde_json::from_value(document).map_err(|_| binding_schema_error())
}

pub(super) fn load_bindings(store: &Store) -> Result<Bindings> {
    let path = store.root.join("session-bindings.json");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > BINDING_FILE_BYTES) {
        return Err(Error::new(
            "binding_store_unavailable",
            "Session binding store exceeds its bounded size; preserve it and inspect recovery",
        ));
    }
    let bindings = match crate::storage::read_optional(&path)? {
        Some(bytes) => decode_bindings(&bytes)?,
        None => Bindings::default(),
    };
    if bindings.entries.len() > BINDING_LIMIT {
        return Err(Error::new(
            "binding_store_unavailable",
            "Session binding count exceeds its bounded limit",
        ));
    }
    Ok(bindings)
}

pub(in crate::gateway) fn initialize_bindings(store: &Store) -> Result<Bindings> {
    let path = store.root.join("session-bindings.json");
    let bindings = load_bindings(store)?;
    if !path.exists() {
        store.write_json("session-bindings.json", &bindings)?;
    }
    Ok(bindings)
}

pub(super) fn save_bindings(store: &Store, bindings: &Bindings) -> Result<()> {
    struct Buffer(Vec<u8>);
    impl std::io::Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) > BINDING_FILE_BYTES as usize {
                return Err(std::io::Error::other("binding capacity"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer(Vec::new());
    serde_json::to_writer(&mut buffer, bindings).map_err(|_| {
        Error::new(
            "session_binding_capacity",
            "Durable native binding byte capacity reached; existing destinations were not evicted",
        )
    })?;
    store.write_raw("session-bindings.json", &buffer.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::test_support::{fixture, native_headers, test_snapshot};
    use serde_json::json;

    #[test]
    fn historical_evidence_normalizes_only_known_routing_envelopes_and_rejects_ambiguity() {
        let untouched = json!({"offering":"user-value","offering_identity":"upstream-value","models":{"offering":{"offering_identity":"model-id"}}});
        let mut evidence = json!({
            "selection":{"offering_identity":"stable-token","offering":"p/offering"},
            "selection_reason":"stable_offering_identity",
            "excluded_candidates":[{"offering":"q/m","reason_code":"offering_disabled"}],
            "metadata":untouched,
            "models":{"offering":untouched},
            "original_decision":{"selection":{"offering_identity":"original-token"}}
        });
        normalize_binding_evidence(&mut evidence).unwrap();
        assert_eq!(evidence["selection"]["model_identity"], "stable-token");
        assert_eq!(evidence["selection"]["qualified_model"], "p/offering");
        assert_eq!(evidence["selection_reason"], "stable_model_identity");
        assert_eq!(
            evidence["excluded_candidates"][0]["reason_code"],
            "model_disabled"
        );
        assert_eq!(
            evidence["original_decision"]["selection"]["model_identity"],
            "original-token"
        );
        assert_eq!(evidence["metadata"], untouched);
        assert_eq!(evidence["models"]["offering"], untouched);
        for mut ambiguous in [
            json!({"selection":{"offering_identity":"same","model_identity":"same"}}),
            json!({"excluded_candidates":[{"offering":"p/m","qualified_model":"p/m"}]}),
        ] {
            assert!(normalize_binding_evidence(&mut ambiguous).is_err());
        }
        let (_temporary, runtime, config) = fixture();
        let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
        runtime
            .choose(
                &test_snapshot(&config, "fixture-1"),
                "dynamic",
                "chat-completions",
                &json!({"messages":[{"role":"user","content":"x"}]}),
                &native_headers(at),
                at + chrono::Duration::milliseconds(1),
            )
            .unwrap();
        let current = serde_json::to_value(&*runtime.bindings.lock().unwrap()).unwrap();
        for decision in [
            r#"{"selection":{"offering_identity":"first","offering_identity":"second"}}"#,
            r#"{"selection":{"offering_identity":"first"},"selection":{"model_identity":"second"}}"#,
        ] {
            let mut mixed = current.clone();
            mixed["entries"]
                .as_object_mut()
                .unwrap()
                .values_mut()
                .next()
                .unwrap()["decision"] = Value::Null;
            let encoded = serde_json::to_string(&mixed).unwrap().replacen(
                "\"decision\":null",
                &format!("\"decision\":{decision}"),
                1,
            );
            assert!(decode_bindings(encoded.as_bytes()).is_err());
        }
        let mut ambiguous = current.clone();
        let entry = ambiguous["entries"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        entry["offering_identity"] = entry["model_identity"].clone();
        assert!(decode_bindings(&serde_json::to_vec(&ambiguous).unwrap()).is_err());
        let mut historical = current.clone();
        let entry = historical["entries"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap()
            .as_object_mut()
            .unwrap();
        let identity = entry.remove("model_identity").unwrap();
        entry.insert("offering_identity".into(), identity);
        let encoded = serde_json::to_string(&historical).unwrap();
        let duplicate = encoded.replacen(
            "\"offering_identity\":",
            "\"offering_identity\":\"duplicate\",\"offering_identity\":",
            1,
        );
        assert!(decode_bindings(duplicate.as_bytes()).is_err());
        historical["unexpected"] = Value::Null;
        assert!(decode_bindings(&serde_json::to_vec(&historical).unwrap()).is_err());
        let mut unknown = current;
        unknown["unexpected"] = Value::Null;
        assert!(decode_bindings(&serde_json::to_vec(&unknown).unwrap()).is_err());
    }
}
