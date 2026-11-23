//! Combined status and its operator-facing brief.
use super::arguments::arg;
use crate::{
    Error, Request, Result,
    config::{self, Config},
    storage::Store,
};
use serde_json::{Value, json};

/// Next action for a configuration that is readable but names no providers.
const NOTHING_CONFIGURED_NEXT: &str = "tmax provider add --help";

pub(super) async fn run(store: &Store, r: &Request) -> Result<Value> {
    let file = config::load(store);
    let daemon = crate::service::status(store).await?;
    if arg(r, "brief").is_some() {
        // Delivery evidence is a soft read: an unreadable index means no target
        // facts, never a hard status failure.
        let delivery = match file.as_ref() {
            Ok(loaded) => crate::agent_integration::delivery(store, &loaded.config)
                .await
                .unwrap_or_else(
                    |error| json!({"state":"unavailable","targets":[],"reason_code":error.code}),
                ),
            Err(_) => json!({"state":"not_configured","targets":[]}),
        };
        return Ok(brief(
            file.as_ref().ok().map(|loaded| &loaded.config),
            file.as_ref().map(|loaded| loaded.exists).unwrap_or(false),
            file.as_ref().err(),
            &daemon,
            &delivery,
        ));
    }
    let failures = daemon.get("failures").cloned().unwrap_or(Value::Null);
    Ok(match file {
        Ok(f) => {
            json!({"config_path":store.root.join("tmax.toml"),"file_revision":f.revision,"configuration_state":if f.exists{"valid"}else{"absent"},"providers":f.config.providers.len(),"targets":f.config.agents.len(),"daemon":daemon,"failures":failures})
        }
        Err(e) => {
            json!({"config_path":store.root.join("tmax.toml"),"configuration_state":"invalid","reason_code":e.code,"field":e.field,"daemon":daemon,"failures":failures,"next_action":if e.field.as_deref()==Some("version"){"config migrate"}else{"config reset or correct supported fields"}})
        }
    })
}

/// Three facts: whether the installation is usable, the first blocker, and the
/// next action.
///
/// The decision is pure over already-loaded facts so the whole priority ladder
/// is testable without a daemon, HTTP, or the live store. `usable` is `false`
/// for every known blocker; it is `true` only when no blocker is known and the
/// remaining uncertainty is irrelevant, and `null` solely when an unreachable
/// daemon is the only uncertainty and no configuration problem is known.
fn brief(
    config: Option<&Config>,
    config_exists: bool,
    config_error: Option<&Error>,
    daemon: &Value,
    delivery: &Value,
) -> Value {
    if let Some(error) = config_error {
        let (code, next_action) = if error.field.as_deref() == Some("version") {
            ("configuration_format", "tmax config migrate")
        } else {
            ("configuration_invalid", "tmax config validate")
        };
        return answer(
            Some(false),
            blocker(
                code,
                error.message.clone(),
                error.field.clone(),
                next_action,
            ),
        );
    }
    // A missing configuration fact without an error is not reachable through
    // `run`; fail closed as unreadable instead of claiming usability.
    let Some(config) = config else {
        return answer(
            Some(false),
            blocker(
                "configuration_invalid",
                "Configuration facts are unavailable",
                None,
                "tmax config validate",
            ),
        );
    };
    if !config_exists {
        return answer(
            Some(false),
            blocker(
                "nothing_configured",
                "No configuration exists yet; add a provider to get started",
                None,
                NOTHING_CONFIGURED_NEXT,
            ),
        );
    }
    if config.providers.is_empty() {
        return answer(
            Some(false),
            blocker(
                "nothing_configured",
                "No providers are configured; add one to get started",
                None,
                NOTHING_CONFIGURED_NEXT,
            ),
        );
    }
    let targets = delivery
        .get("targets")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let config_relayed = relayed_target(config);
    let has_relayed_targets = config_relayed.is_some() || !targets.is_empty();
    let gateway = config.gateway.as_ref();
    let discovery_state = daemon
        .pointer("/daemon/discovery_state")
        .and_then(Value::as_str);
    if has_relayed_targets && !matches!(discovery_state, Some("compatible" | "absent")) {
        let reason_code = daemon
            .pointer("/daemon/reason_code")
            .and_then(Value::as_str);
        let message = match reason_code {
            Some(reason_code) => {
                format!("The tmax daemon could not be identified ({reason_code})")
            }
            None => "The tmax daemon could not be identified".to_owned(),
        };
        // An unidentified or unavailable daemon leaves relayed use unknown,
        // while a known incompatible owner is a definite blocker.
        let usable = match discovery_state {
            Some("unidentified" | "unavailable") | None => None,
            _ => Some(false),
        };
        return answer(
            usable,
            blocker(
                "daemon_identity_unavailable",
                message,
                None,
                "tmax gateway status",
            ),
        );
    }
    let gateway_ready = daemon.pointer("/gateway/ready").and_then(Value::as_bool);
    if has_relayed_targets && gateway_ready != Some(true) {
        return answer(
            Some(false),
            blocker(
                "gateway_not_ready",
                "The gateway is configured but not ready; relayed entries cannot use it",
                None,
                "tmax gateway start",
            ),
        );
    }
    let daemon_key_ref = daemon.pointer("/gateway/key_ref").and_then(Value::as_str);
    if let (Some(daemon_key_ref), Some(gateway)) = (daemon_key_ref, gateway)
        && daemon_key_ref != gateway.key_ref
    {
        return answer(
            Some(false),
            blocker(
                "key_rotated_not_reloaded",
                "The running gateway uses a different shared key than the saved configuration",
                None,
                "tmax gateway restart",
            ),
        );
    }
    if has_relayed_targets && delivery.get("state").and_then(Value::as_str) == Some("unavailable") {
        let reason_code = delivery
            .get("reason_code")
            .and_then(Value::as_str)
            .unwrap_or("delivery_state_unavailable");
        let target = config_relayed.as_deref().unwrap_or("<target>");
        return answer(
            Some(false),
            blocker(
                "delivery_unknown",
                format!(
                    "Retained delivery state could not be read ({reason_code}); inspect the target"
                ),
                None,
                &format!("tmax agent inspect {target}"),
            ),
        );
    }
    let stale = targets.iter().find(|row| {
        row.pointer("/applied_delivery/state")
            .and_then(Value::as_str)
            == Some("stale")
    });
    let blocked = targets.iter().find(|row| {
        row.pointer("/applied_delivery/state")
            .and_then(Value::as_str)
            == Some("blocked")
    });
    if let Some(row) = stale.or(blocked) {
        let target = row
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let reasons = row
            .pointer("/applied_delivery/reasons")
            .and_then(Value::as_array)
            .map(|reasons| {
                reasons
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let detail = if reasons.is_empty() {
            String::new()
        } else {
            format!(" ({reasons})")
        };
        let (code, state) = if stale.is_some() {
            ("delivery_stale", "stale")
        } else {
            ("delivery_blocked", "blocked")
        };
        return answer(
            Some(false),
            blocker(
                code,
                format!("Relayed target {target} has {state} delivery{detail}"),
                None,
                &format!("tmax agent apply {target}"),
            ),
        );
    }
    if let Some(stale_key) = daemon
        .pointer("/failures/stale_shared_key")
        .filter(|value| !value.is_null())
    {
        let count = stale_key.get("count").and_then(Value::as_u64).unwrap_or(0);
        return answer(
            Some(false),
            blocker(
                "stale_shared_key",
                format!(
                    "{count} requests were rejected because clients present a shared key this gateway does not accept"
                ),
                None,
                "tmax gateway key show",
            ),
        );
    }
    answer(Some(true), Value::Null)
}

/// First declared target with a relayed entry, independent of retained delivery
/// evidence; `status` must still name the target when that evidence is unreadable.
fn relayed_target(config: &Config) -> Option<String> {
    config
        .agents
        .iter()
        .find(|(_, target)| {
            target
                .providers
                .values()
                .any(|entry| entry.is_relayed(config))
        })
        .map(|(id, _)| id.clone())
}

fn blocker(
    code: &str,
    message: impl Into<String>,
    field: Option<String>,
    next_action: &str,
) -> Value {
    json!({"code":code,"message":message.into(),"field":field,"next_action":next_action})
}

fn answer(usable: Option<bool>, blocker: Value) -> Value {
    let next_action = blocker.get("next_action").cloned().unwrap_or(Value::Null);
    json!({"brief":true,"usable":usable,"blocker":blocker,"next_action":next_action})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn configured() -> Config {
        let mut config = Config::empty();
        config
            .providers
            .insert("up".into(), config::Provider::default());
        config
    }

    fn with_gateway() -> Config {
        let mut config = configured();
        config.gateway = Some(config::Gateway {
            listen: "127.0.0.1:17840".into(),
            key_ref: "secret:gw".into(),
            autostart: false,
        });
        config
    }

    fn daemon(discovery: &str, ready: bool, key_ref: Option<&str>) -> Value {
        json!({
            "daemon": {"discovery_state": discovery, "reason_code": null},
            "gateway": {"ready": ready, "listen": "127.0.0.1:17840", "key_ref": key_ref, "config_revision": "rev"},
            "failures": null,
        })
    }

    fn with_relayed_target() -> Config {
        let mut config = with_gateway();
        config.agents.insert(
            "dsh-main".into(),
            config::AgentTarget {
                agent: "dsh".into(),
                scope: "/native/dsh-main".into(),
                providers: [(
                    "tmax".into(),
                    config::NativeProvider {
                        provider: config::GATEWAY_PROVIDER_ID.into(),
                        models: vec![config::ModelSelection {
                            id: "route".into(),
                            protocol: "responses".into(),
                        }],
                    },
                )]
                .into(),
                default: None,
                roles: Default::default(),
            },
        );
        config
    }

    fn no_targets() -> Value {
        json!({"state": "not_configured", "targets": []})
    }

    fn targets(rows: Vec<Value>) -> Value {
        json!({"state": "configured", "targets": rows})
    }

    fn applied(state: &str, target: &str, reasons: &[&str]) -> Value {
        json!({"target": target, "entry": "relayed", "applied_delivery": {"state": state, "reasons": reasons}})
    }

    fn decide(
        config: Option<&Config>,
        exists: bool,
        error: Option<&Error>,
        daemon: &Value,
        delivery: &Value,
    ) -> Value {
        brief(config, exists, error, daemon, delivery)
    }

    #[test]
    fn unsupported_configuration_version_requires_migration() {
        let error = Error::new(
            "invalid_configuration",
            "Configuration format version 1 is unsupported; run `tmax config migrate`",
        )
        .field("version");
        let answer = decide(
            None,
            true,
            Some(&error),
            &daemon("absent", false, None),
            &no_targets(),
        );
        assert_eq!(answer["brief"], true);
        assert_eq!(answer["usable"], false);
        assert_eq!(answer["blocker"]["code"], "configuration_format");
        assert_eq!(answer["blocker"]["field"], "version");
        assert_eq!(answer["next_action"], "tmax config migrate");
    }

    #[test]
    fn invalid_configuration_requires_validation() {
        let error = Error::new(
            "invalid_configuration",
            "Configuration field has an unknown name, missing value, or invalid type",
        )
        .field("providers.up");
        let answer = decide(
            None,
            true,
            Some(&error),
            &daemon("absent", false, None),
            &no_targets(),
        );
        assert_eq!(answer["usable"], false);
        assert_eq!(answer["blocker"]["code"], "configuration_invalid");
        assert_eq!(answer["blocker"]["field"], "providers.up");
        assert_eq!(answer["next_action"], "tmax config validate");
    }

    #[test]
    fn absent_and_empty_configurations_name_the_first_provider_action() {
        let absent = decide(
            Some(&Config::empty()),
            false,
            None,
            &daemon("absent", false, None),
            &no_targets(),
        );
        assert_eq!(absent["usable"], false);
        assert_eq!(absent["blocker"]["code"], "nothing_configured");
        assert_eq!(absent["next_action"], "tmax provider add --help");

        let empty = decide(
            Some(&Config::empty()),
            true,
            None,
            &daemon("absent", false, None),
            &no_targets(),
        );
        assert_eq!(empty["blocker"]["code"], "nothing_configured");
        assert_eq!(empty["next_action"], "tmax provider add --help");
    }

    #[test]
    fn configured_gateway_that_is_not_ready_requires_start() {
        let config = with_gateway();
        let delivery = targets(vec![applied("current", "dsh-main", &[])]);
        let answer = decide(
            Some(&config),
            true,
            None,
            &daemon("absent", false, Some("secret:gw")),
            &delivery,
        );
        assert_eq!(answer["usable"], false);
        assert_eq!(answer["blocker"]["code"], "gateway_not_ready");
        assert_eq!(answer["next_action"], "tmax gateway start");
    }

    #[test]
    fn loaded_key_different_from_saved_key_requires_restart() {
        let config = with_gateway();
        let answer = decide(
            Some(&config),
            true,
            None,
            &daemon("compatible", true, Some("secret:old")),
            &no_targets(),
        );
        assert_eq!(answer["usable"], false);
        assert_eq!(answer["blocker"]["code"], "key_rotated_not_reloaded");
        assert_eq!(answer["next_action"], "tmax gateway restart");
    }

    #[test]
    fn stale_delivery_names_the_target_and_apply_action() {
        let config = with_gateway();
        let delivery = targets(vec![applied("stale", "dsh-main", &["key_rotated"])]);
        let answer = decide(
            Some(&config),
            true,
            None,
            &daemon("compatible", true, Some("secret:gw")),
            &delivery,
        );
        assert_eq!(answer["usable"], false);
        assert_eq!(answer["blocker"]["code"], "delivery_stale");
        assert!(
            answer["blocker"]["message"]
                .as_str()
                .unwrap()
                .contains("dsh-main")
        );
        assert!(
            answer["blocker"]["message"]
                .as_str()
                .unwrap()
                .contains("key_rotated")
        );
        assert_eq!(answer["next_action"], "tmax agent apply dsh-main");
    }

    #[test]
    fn a_stale_target_wins_over_a_blocked_target() {
        let config = with_gateway();
        let delivery = targets(vec![
            applied("blocked", "pi-main", &["gateway_not_ready"]),
            applied("stale", "dsh-main", &["listen_changed"]),
        ]);
        let answer = decide(
            Some(&config),
            true,
            None,
            &daemon("compatible", true, Some("secret:gw")),
            &delivery,
        );
        assert_eq!(answer["blocker"]["code"], "delivery_stale");
        assert_eq!(answer["next_action"], "tmax agent apply dsh-main");
    }

    #[test]
    fn blocked_delivery_is_named_when_no_target_is_stale() {
        let config = with_gateway();
        let delivery = targets(vec![applied("blocked", "pi-main", &["gateway_not_ready"])]);
        let answer = decide(
            Some(&config),
            true,
            None,
            &daemon("compatible", true, Some("secret:gw")),
            &delivery,
        );
        assert_eq!(answer["blocker"]["code"], "delivery_blocked");
        assert_eq!(answer["next_action"], "tmax agent apply pi-main");
    }

    #[test]
    fn aggregated_stale_shared_key_is_named_once() {
        let config = configured();
        let mut daemon = daemon("compatible", false, None);
        daemon["failures"] = json!({
            "stale_shared_key": {"count": 7, "first_at": "2026-11-18T10:00:00Z", "last_at": "2026-11-18T11:00:00Z", "reason_code": "stale_shared_key", "message": "Clients are presenting a shared key that this gateway does not accept", "repair": "tmax gateway key show, then update each client's configured key"}
        });
        let answer = decide(Some(&config), true, None, &daemon, &no_targets());
        assert_eq!(answer["usable"], false);
        assert_eq!(answer["blocker"]["code"], "stale_shared_key");
        let message = answer["blocker"]["message"].as_str().unwrap();
        assert!(message.contains('7'));
        assert_eq!(message.matches('7').count(), 1);
        assert_eq!(answer["next_action"], "tmax gateway key show");
    }

    #[test]
    fn healthy_configuration_is_usable() {
        let config = configured();
        let answer = decide(
            Some(&config),
            true,
            None,
            &daemon("compatible", true, None),
            &no_targets(),
        );
        assert_eq!(answer["usable"], true);
        assert!(answer["blocker"].is_null());
        assert!(answer["next_action"].is_null());
        assert_eq!(answer["brief"], true);

        // A stopped daemon does not block direct-only use.
        let stopped = decide(
            Some(&config),
            true,
            None,
            &daemon("absent", false, None),
            &no_targets(),
        );
        assert_eq!(stopped["usable"], true);
        assert!(stopped["blocker"].is_null());
    }

    #[test]
    fn unidentified_daemon_leaves_relayed_use_unknown() {
        let config = with_gateway();
        let delivery = targets(vec![applied("current", "dsh-main", &[])]);
        let mut daemon = daemon("unidentified", false, None);
        daemon["daemon"]["reason_code"] = json!("daemon_identity_unavailable");
        let answer = decide(Some(&config), true, None, &daemon, &delivery);
        assert!(answer["usable"].is_null());
        assert_eq!(answer["blocker"]["code"], "daemon_identity_unavailable");
        assert!(
            answer["blocker"]["message"]
                .as_str()
                .unwrap()
                .contains("daemon_identity_unavailable")
        );
        assert_eq!(answer["next_action"], "tmax gateway status");
    }

    #[test]
    fn incompatible_daemon_is_a_definite_blocker() {
        let config = with_gateway();
        let delivery = targets(vec![applied("current", "dsh-main", &[])]);
        let mut daemon = daemon("incompatible", false, None);
        daemon["daemon"]["reason_code"] = json!("management_protocol_mismatch");
        let answer = decide(Some(&config), true, None, &daemon, &delivery);
        assert_eq!(answer["usable"], false);
        assert_eq!(answer["blocker"]["code"], "daemon_identity_unavailable");
    }

    #[tokio::test]
    async fn full_status_keeps_previous_keys_and_adds_failures() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        store
            .write_raw(
                "tmax.toml",
                b"version = 2\n[providers.up]\nintegration = 'opencode-go'\n",
            )
            .unwrap();
        let request = Request {
            command: vec!["status".into()],
            operands: Vec::new(),
            arguments: BTreeMap::new(),
            dry_run: false,
            if_revision: None,
        };
        let body = run(&store, &request).await.unwrap();
        assert_eq!(body["configuration_state"], "valid");
        assert_eq!(body["providers"], 1);
        assert_eq!(body["targets"], 0);
        assert!(body.get("config_path").is_some());
        assert!(body.get("file_revision").is_some());
        assert!(body.get("daemon").is_some());
        assert!(body.get("failures").is_some());
        assert_eq!(
            body["failures"],
            body["daemon"]
                .get("failures")
                .cloned()
                .unwrap_or(Value::Null)
        );
    }

    #[tokio::test]
    async fn full_status_reports_invalid_configuration_and_failures() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        store.write_raw("tmax.toml", b"version = 1\n").unwrap();
        let request = Request {
            command: vec!["status".into()],
            operands: Vec::new(),
            arguments: BTreeMap::new(),
            dry_run: false,
            if_revision: None,
        };
        let body = run(&store, &request).await.unwrap();
        assert_eq!(body["configuration_state"], "invalid");
        assert_eq!(body["reason_code"], "invalid_configuration");
        assert_eq!(body["field"], "version");
        assert_eq!(body["next_action"], "config migrate");
        assert!(body.get("daemon").is_some());
        assert!(body.get("failures").is_some());
    }

    #[tokio::test]
    async fn brief_through_run_reports_nothing_configured_without_a_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        let request = Request {
            command: vec!["status".into()],
            operands: Vec::new(),
            arguments: BTreeMap::from([("brief".into(), vec!["true".into()])]),
            dry_run: false,
            if_revision: None,
        };
        let body = run(&store, &request).await.unwrap();
        assert_eq!(body["brief"], true);
        assert_eq!(body["usable"], false);
        assert_eq!(body["blocker"]["code"], "nothing_configured");
        assert_eq!(body["next_action"], "tmax provider add --help");
    }

    #[tokio::test]
    async fn brief_through_run_reports_unsupported_format_version() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        store.write_raw("tmax.toml", b"version = 1\n").unwrap();
        let request = Request {
            command: vec!["status".into()],
            operands: Vec::new(),
            arguments: BTreeMap::from([("brief".into(), vec!["true".into()])]),
            dry_run: false,
            if_revision: None,
        };
        let body = run(&store, &request).await.unwrap();
        assert_eq!(body["blocker"]["code"], "configuration_format");
        assert_eq!(body["blocker"]["field"], "version");
        assert_eq!(body["next_action"], "tmax config migrate");
    }

    #[test]
    fn unreadable_delivery_evidence_is_named_once_with_an_inspect_action() {
        let config = with_relayed_target();
        let delivery = json!({
            "state": "unavailable",
            "reason_code": "recovery_invalid",
            "targets": [],
        });
        let answer = decide(
            Some(&config),
            true,
            None,
            &daemon("compatible", true, Some("secret:gw")),
            &delivery,
        );
        assert_eq!(answer["usable"], false);
        assert_eq!(answer["blocker"]["code"], "delivery_unknown");
        assert_eq!(answer["next_action"], "tmax agent inspect dsh-main");
        assert!(
            answer["blocker"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("recovery_invalid")),
            "{answer}"
        );
    }
}
