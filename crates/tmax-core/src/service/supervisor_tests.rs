use super::*;

fn manager(activation: &str) -> Manager {
    Manager {
        name: "test-manager",
        activation: activation.into(),
        survives_logout: None,
        session_scope: "test",
    }
}

fn addr(value: &str) -> SocketAddr {
    value.parse().unwrap()
}

fn stored(store: &Store) -> Activation {
    read(store).unwrap().expect("activation should be durable")
}

fn recovery_error(store: &Store) -> Error {
    match read(store) {
        Err(error) => error,
        Ok(_) => panic!("invalid recovery metadata must not be accepted"),
    }
}

#[test]
fn requests_are_durable_and_independent() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    let manager = manager("session-a");
    assert!(read(&store).unwrap().is_none());

    let inference = addr("127.0.0.1:41001");
    let first = request(&store, &manager, Some(true), None, Some(inference), None).unwrap();
    let reopened = Store::at(home.path().to_owned());
    let recovered = stored(&reopened);
    assert_eq!(recovered.activation, "session-a");
    assert_eq!(recovered.generation, first.generation);
    assert!(recovered.requested.inference);
    assert!(!recovered.requested.web);
    assert_eq!(recovered.inference_listen, Some(inference));
    assert_eq!(recovered.web_listen, None);

    let web = addr("127.0.0.1:41002");
    let second = request(&store, &manager, None, Some(true), None, Some(web)).unwrap();
    let recovered = stored(&reopened);
    assert!(second.generation > first.generation);
    assert_eq!(recovered.generation, second.generation);
    assert!(recovered.requested.inference && recovered.requested.web);
    assert_eq!(recovered.inference_listen, Some(inference));
    assert_eq!(recovered.web_listen, Some(web));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(store.path(RECORD).unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn stop_intent_survives_reopen_and_fences_pending_recovery() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    let manager = manager("session-a");
    let inference = addr("127.0.0.1:41001");
    let web = addr("127.0.0.1:41002");
    let pending = request(
        &store,
        &manager,
        Some(true),
        Some(true),
        Some(inference),
        Some(web),
    )
    .unwrap();
    let stopped = request(&store, &manager, Some(false), None, None, None).unwrap();
    assert!(stopped.generation > pending.generation);

    let reopened = Store::at(home.path().to_owned());
    let recovery = current(&reopened, &pending).unwrap();
    assert_eq!(recovery.generation, stopped.generation);
    assert!(!recovery.requested.inference);
    assert!(recovery.requested.web);
    assert_eq!(recovery.inference_listen, None);
    assert_eq!(recovery.web_listen, Some(web));

    let final_stop = request(&store, &manager, None, Some(false), None, None).unwrap();
    let recovery = current(&reopened, &pending).unwrap();
    assert_eq!(recovery.generation, final_stop.generation);
    assert!(!recovery.requested.any());
    assert_eq!(recovery.web_listen, None);
    assert_eq!(recovery.state, "stopped");
}

#[test]
fn failed_request_withdraws_only_its_new_surface() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    let manager = manager("session-a");
    let web = addr("127.0.0.1:41002");
    request(&store, &manager, None, Some(true), None, Some(web)).unwrap();
    let attempt = request(
        &store,
        &manager,
        Some(true),
        None,
        Some(addr("127.0.0.1:41001")),
        None,
    )
    .unwrap();
    let error = Error::new("service_readiness_failed", "listener unavailable");
    withdraw(
        &store,
        &attempt.activation,
        attempt.generation,
        true,
        false,
        &error,
    )
    .unwrap();

    let recovered = stored(&Store::at(home.path().to_owned()));
    assert!(recovered.generation > attempt.generation);
    assert!(!recovered.requested.inference);
    assert_eq!(recovered.inference_listen, None);
    assert!(recovered.requested.web);
    assert_eq!(recovered.web_listen, Some(web));
    assert_eq!(
        recovered.reason_code.as_deref(),
        Some("service_readiness_failed")
    );
}

#[test]
fn stale_withdrawal_cannot_remove_a_newer_request_or_stop() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    let manager = manager("session-a");
    let first = request(
        &store,
        &manager,
        Some(true),
        None,
        Some(addr("127.0.0.1:41001")),
        None,
    )
    .unwrap();
    let web = addr("127.0.0.1:41002");
    let newer = request(&store, &manager, Some(false), Some(true), None, Some(web)).unwrap();
    let error = Error::new("service_readiness_failed", "old attempt failed");
    withdraw(
        &store,
        &first.activation,
        first.generation,
        true,
        true,
        &error,
    )
    .unwrap();

    let recovered = stored(&Store::at(home.path().to_owned()));
    assert_eq!(recovered.generation, newer.generation);
    assert!(!recovered.requested.inference);
    assert_eq!(recovered.inference_listen, None);
    assert!(recovered.requested.web);
    assert_eq!(recovered.web_listen, Some(web));
    assert_eq!(recovered.reason_code, None);
}

#[test]
fn acknowledged_addresses_are_recovered_and_stopped_addresses_are_not_revived() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    let manager = manager("session-a");
    let inference = addr("127.0.0.1:41001");
    let allocated_web = addr("127.0.0.1:41002");
    let pending = request(
        &store,
        &manager,
        Some(true),
        Some(true),
        Some(inference),
        None,
    )
    .unwrap();
    acknowledge(
        &store,
        pending,
        "process-a",
        Some(inference),
        Some(allocated_web),
        None,
    )
    .unwrap();

    let reopened = Store::at(home.path().to_owned());
    let recovered = stored(&reopened);
    assert_eq!(recovered.inference_listen, Some(inference));
    assert_eq!(recovered.web_listen, Some(allocated_web));
    assert_eq!(recovered.process.as_deref(), Some("process-a"));
    assert_eq!(recovered.state, "running");
    let repeated = request(&store, &manager, None, Some(true), None, None).unwrap();
    assert_eq!(repeated.web_listen, Some(allocated_web));

    let stopped = request(&store, &manager, None, Some(false), None, None).unwrap();
    acknowledge(
        &store,
        stopped,
        "process-a",
        Some(inference),
        Some(allocated_web),
        None,
    )
    .unwrap();
    let recovered = stored(&reopened);
    assert!(recovered.requested.inference);
    assert!(!recovered.requested.web);
    assert_eq!(recovered.inference_listen, Some(inference));
    assert_eq!(recovered.web_listen, None);
}

#[test]
fn new_activation_does_not_inherit_previous_requests() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    let old_manager = manager("session-a");
    let old = request(
        &store,
        &old_manager,
        Some(true),
        Some(true),
        Some(addr("127.0.0.1:41001")),
        Some(addr("127.0.0.1:41002")),
    )
    .unwrap();
    let new_manager = manager("session-b");
    let new = request(
        &store,
        &new_manager,
        None,
        Some(true),
        None,
        Some(addr("127.0.0.1:41003")),
    )
    .unwrap();
    let recovered = stored(&Store::at(home.path().to_owned()));
    assert_eq!(recovered.activation, "session-b");
    assert_eq!(recovered.generation, new.generation);
    assert!(!recovered.requested.inference);
    assert!(recovered.requested.web);
    assert_eq!(recovered.inference_listen, None);
    assert_eq!(recovered.web_listen, Some(addr("127.0.0.1:41003")));
    let error = match current(&store, &old) {
        Err(error) => error,
        Ok(_) => panic!("old activation must not recover into a new activation"),
    };
    assert_eq!(error.code, "service_recovery_unavailable");
}

#[test]
fn stale_withdrawal_cannot_cross_activation_when_generations_collide() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    let old = request(
        &store,
        &manager("session-a"),
        None,
        Some(true),
        None,
        Some(addr("127.0.0.1:41002")),
    )
    .unwrap();
    let new_address = addr("127.0.0.1:41003");
    let new = request(
        &store,
        &manager("session-b"),
        None,
        Some(true),
        None,
        Some(new_address),
    )
    .unwrap();
    assert_eq!(old.generation, new.generation);

    let error = Error::new("service_readiness_failed", "old activation failed");
    withdraw(&store, &old.activation, old.generation, false, true, &error).unwrap();
    let recovered = stored(&Store::at(home.path().to_owned()));
    assert_eq!(recovered.activation, "session-b");
    assert_eq!(recovered.generation, new.generation);
    assert!(recovered.requested.web);
    assert_eq!(recovered.web_listen, Some(new_address));
    assert_eq!(recovered.reason_code, None);
}

#[test]
fn malformed_foreign_and_unsupported_records_are_not_recovered_or_replaced() {
    let home = tempfile::tempdir().unwrap();
    let other_home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    let foreign = Store::at(other_home.path().to_owned());
    let manager = manager("session-a");
    request(
        &foreign,
        &manager,
        Some(true),
        None,
        Some(addr("127.0.0.1:41001")),
        None,
    )
    .unwrap();
    let foreign_bytes = std::fs::read(foreign.path(RECORD).unwrap()).unwrap();

    let mut unsupported = Activation::new(&store, &manager);
    unsupported.version = RECORD_VERSION + 1;
    let unsupported_bytes = serde_json::to_vec(&unsupported).unwrap();
    let mut invalid_activation = Activation::new(&store, &manager);
    invalid_activation.activation.clear();
    let invalid_activation_bytes = serde_json::to_vec(&invalid_activation).unwrap();
    let mut invalid_address = Activation::new(&store, &manager);
    invalid_address.web_listen = Some(addr("0.0.0.0:41002"));
    let invalid_address_bytes = serde_json::to_vec(&invalid_address).unwrap();

    for bytes in [
        b"{not-json".as_slice(),
        foreign_bytes.as_slice(),
        unsupported_bytes.as_slice(),
        invalid_activation_bytes.as_slice(),
        invalid_address_bytes.as_slice(),
    ] {
        store.write_raw(RECORD, bytes).unwrap();
        assert_eq!(recovery_error(&store).code, "service_recovery_unavailable");
        let error = match request(
            &store,
            &manager,
            Some(true),
            None,
            Some(addr("127.0.0.1:41001")),
            None,
        ) {
            Err(error) => error,
            Ok(_) => panic!("invalid recovery metadata must not be silently overwritten"),
        };
        assert_eq!(error.code, "service_recovery_unavailable");
        assert_eq!(std::fs::read(store.path(RECORD).unwrap()).unwrap(), bytes);
    }
}

#[test]
fn exhausted_generation_does_not_overwrite_existing_state() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    let manager = manager("session-a");
    let mut record = Activation::new(&store, &manager);
    record.generation = u64::MAX;
    record.requested.web = true;
    record.web_listen = Some(addr("127.0.0.1:41002"));
    save(&store, &record).unwrap();

    let error = match request(&store, &manager, None, Some(false), None, None) {
        Err(error) => error,
        Ok(_) => panic!("exhausted generation must not wrap"),
    };
    assert_eq!(error.code, "service_recovery_unavailable");
    let recovered = stored(&Store::at(home.path().to_owned()));
    assert_eq!(recovered.generation, u64::MAX);
    assert!(recovered.requested.web);
    assert_eq!(recovered.web_listen, Some(addr("127.0.0.1:41002")));
}

#[test]
fn failed_new_listener_withdraws_without_a_surviving_cli() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    let manager = manager("session-a");
    let web = addr("127.0.0.1:41002");
    let initial = request(&store, &manager, None, Some(true), None, Some(web)).unwrap();
    acknowledge(&store, initial, "process-a", None, Some(web), None).unwrap();
    let attempt = request(
        &store,
        &manager,
        Some(true),
        None,
        Some(addr("127.0.0.1:41001")),
        None,
    )
    .unwrap();
    let error = Error::new("credential_unavailable", "missing shared key");
    acknowledge(&store, attempt, "process-a", None, Some(web), Some(&error)).unwrap();
    let recovered = stored(&store);
    assert!(!recovered.requested.inference);
    assert!(!recovered.pending.any());
    assert!(recovered.requested.web);
    assert_eq!(recovered.web_listen, Some(web));
    assert_eq!(recovered.state, "running");
    assert_eq!(recovered.last_exit_code, None);
}

#[test]
fn web_stop_keeps_an_existing_inference_recovery_blocker() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    let manager = manager("session-a");
    let inference = addr("127.0.0.1:41001");
    let web = addr("127.0.0.1:41002");
    let initial = request(
        &store,
        &manager,
        Some(true),
        Some(true),
        Some(inference),
        Some(web),
    )
    .unwrap();
    acknowledge(
        &store,
        initial,
        "process-a",
        Some(inference),
        Some(web),
        None,
    )
    .unwrap();
    let error = Error::new("restart_required", "saved address differs");
    acknowledge(
        &store,
        stored(&store),
        "process-b",
        None,
        Some(web),
        Some(&error),
    )
    .unwrap();
    let stop = request(&store, &manager, None, Some(false), None, None).unwrap();
    acknowledge(&store, stop, "process-b", None, None, None).unwrap();
    let recovered = stored(&store);
    assert!(recovered.requested.inference);
    assert!(!recovered.requested.web);
    assert_eq!(recovered.state, "blocked");
    assert_eq!(recovered.reason_code.as_deref(), Some("restart_required"));
    assert_eq!(recovered.inference_listen, Some(inference));
    assert_eq!(recovered.last_exit_code, None);
}

#[test]
fn explicit_repair_does_not_guess_missing_listener_requests() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    store.write_raw(RECORD, b"{broken").unwrap();
    assert_eq!(recovery_error(&store).code, "service_recovery_unavailable");
    repair(&store, &manager("session-a"), None).unwrap();
    let recovered = stored(&store);
    assert!(!recovered.requested.any());
    assert!(recovered.inference_listen.is_none() && recovered.web_listen.is_none());
    assert_eq!(recovered.state, "stopped");
    store.write_raw(RECORD, &vec![b' '; 32 * 1024 + 1]).unwrap();
    assert_eq!(recovery_error(&store).code, "service_recovery_unavailable");
}

#[tokio::test]
async fn readiness_requires_owned_acknowledged_process_and_restart_cannot_revive_a_direct_stop() {
    use crate::service::{
        daemon::{Daemon, Shutdown, daemon_control},
        supervised_control::readiness,
    };
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let home = tempfile::tempdir().unwrap();
    let store = Store::at(home.path().to_owned());
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let inference = reserved.local_addr().unwrap();
    drop(reserved);
    let key = crate::secrets::put(&store, "gateway", b"readiness-test-key").unwrap();
    let mut config = crate::config::Config::empty();
    config.gateway = Some(crate::config::Gateway {
        listen: inference.to_string(),
        key_ref: key,
        autostart: false,
    });
    store
        .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
        .unwrap();
    let m = manager("session-a");
    let process = "actual-process";
    let mut initial = request(&store, &m, None, Some(true), None, None).unwrap();
    initial.process = Some(process.into());
    save(&store, &initial).unwrap();
    let runtime = crate::gateway::Runtime::new(store.clone(), process.into()).unwrap();
    let daemon = Arc::new(Mutex::new(Daemon::new(
        store.clone(),
        process.into(),
        runtime,
        Arc::new(Shutdown::new()),
    )));
    let operation = |group: &str, action: &str| crate::Request {
        command: vec![group.into(), action.into()],
        operands: Vec::new(),
        arguments: Default::default(),
        dry_run: false,
        if_revision: None,
    };
    let mut observed = Observation {
        registration: "owned",
        state: "running",
        pid: Some(std::process::id()),
        attempts: None,
        last_exit_code: None,
        reason_code: None,
        retry_delay_seconds: RESTART_DELAY_SECONDS,
    };
    {
        let daemon = daemon.lock().await;
        assert!(
            !readiness(
                &store,
                &initial,
                Some(&daemon.view()),
                Some((&m, &observed)),
                false,
                true
            )
            .unwrap()
        );
    }
    let opened = {
        let mut guard = daemon.lock().await;
        daemon_control(&daemon, &mut guard, operation("web", "open"))
            .await
            .unwrap()
    };
    assert_eq!(opened["daemon"]["web"]["state"], "running");
    let web: SocketAddr = opened["daemon"]["web"]["listen"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        readiness(
            &store,
            &initial,
            Some(&opened),
            Some((&m, &observed)),
            false,
            true
        )
        .unwrap()
    );

    observed.pid = Some(std::process::id().wrapping_add(1));
    assert!(
        !readiness(
            &store,
            &initial,
            Some(&opened),
            Some((&m, &observed)),
            false,
            true
        )
        .unwrap()
    );
    observed.pid = Some(std::process::id());
    observed.registration = "absent";
    assert!(
        !readiness(
            &store,
            &initial,
            Some(&opened),
            Some((&m, &observed)),
            false,
            true
        )
        .unwrap()
    );
    observed.registration = "owned";
    assert!(
        !readiness(
            &store,
            &initial,
            Some(&opened),
            Some((&manager("session-b"), &observed)),
            false,
            true
        )
        .unwrap()
    );
    let mut unrelated = opened.clone();
    unrelated["daemon"]["identity"]["process_instance"] = json!("unrelated-process");
    assert!(
        !readiness(
            &store,
            &initial,
            Some(&unrelated),
            Some((&m, &observed)),
            false,
            true
        )
        .unwrap()
    );
    unrelated = opened.clone();
    unrelated["daemon"]["web"]["listen"] = json!(format!(
        "127.0.0.1:{}",
        if web.port() == 41003 { 41004 } else { 41003 }
    ));
    assert!(
        !readiness(
            &store,
            &initial,
            Some(&unrelated),
            Some((&m, &observed)),
            false,
            true
        )
        .unwrap()
    );
    let mut wrong_attempt = initial.clone();
    wrong_attempt.web_listen = Some(addr(if web.port() == 41003 {
        "127.0.0.1:41004"
    } else {
        "127.0.0.1:41003"
    }));
    assert!(
        !readiness(
            &store,
            &wrong_attempt,
            Some(&opened),
            Some((&m, &observed)),
            false,
            true
        )
        .unwrap()
    );

    let inference_attempt = request(&store, &m, Some(true), None, Some(inference), None).unwrap();
    let started = {
        let mut guard = daemon.lock().await;
        daemon_control(&daemon, &mut guard, operation("gateway", "start"))
            .await
            .unwrap()
    };
    assert!(
        readiness(
            &store,
            &inference_attempt,
            Some(&started),
            Some((&m, &observed)),
            true,
            true
        )
        .unwrap()
    );
    let mut wrong_inference = started.clone();
    wrong_inference["daemon"]["inference"]["listen"] = json!(format!(
        "127.0.0.1:{}",
        if inference.port() == 41003 {
            41004
        } else {
            41003
        }
    ));
    assert!(
        !readiness(
            &store,
            &inference_attempt,
            Some(&wrong_inference),
            Some((&m, &observed)),
            true,
            true
        )
        .unwrap()
    );

    let restart = request_restart(&store, &m, true, Some(inference), Some(web)).unwrap();
    assert!(restart.pending.inference && restart.pending.web);
    let stopped = {
        let mut guard = daemon.lock().await;
        daemon_control(&daemon, &mut guard, operation("gateway", "stop"))
            .await
            .unwrap()
    };
    assert_eq!(stopped["gateway"]["ready"], false);
    assert_eq!(stopped["daemon"]["web"]["state"], "running");
    let recovered = stored(&Store::at(home.path().to_owned()));
    assert!(recovered.generation > restart.generation);
    assert!(!recovered.requested.inference);
    assert!(recovered.requested.web);
    assert_eq!(recovered.inference_listen, None);
    assert_eq!(recovered.web_listen, Some(web));
    assert_eq!(
        readiness(
            &store,
            &restart,
            Some(&stopped),
            Some((&m, &observed)),
            true,
            true
        )
        .unwrap_err()
        .code,
        "inputs_changed"
    );
    assert_eq!(
        current(&store, &restart).unwrap().generation,
        recovered.generation
    );

    let mut guard = daemon.lock().await;
    daemon_control(&daemon, &mut guard, operation("web", "stop"))
        .await
        .unwrap();
    drop(guard);
    let replacement = request(
        &store,
        &manager("session-b"),
        None,
        Some(true),
        None,
        Some(web),
    )
    .unwrap();
    assert_eq!(replacement.generation, initial.generation);
    assert_eq!(
        readiness(
            &store,
            &initial,
            Some(&opened),
            Some((&m, &observed)),
            false,
            true
        )
        .unwrap_err()
        .code,
        "inputs_changed"
    );
}
