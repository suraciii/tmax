use super::*;

fn fixture() -> (tempfile::TempDir, Store, Plan, Arc<Flow>) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path().to_owned());
    store.write_raw("tmax.toml", b"# keep this comment\nversion = 1\n[providers.work]\nintegration = \"codex-subscription\"\n").unwrap();
    let config = config::load(&store).unwrap().config;
    crate::routing::reconcile(&store, &Config::empty(), &config, &[]).unwrap();
    let request = Request {
        command: vec!["provider".into(), "login-start".into()],
        operands: vec!["work".into()],
        arguments: [("new-codex".into(), vec!["true".into()])].into(),
        dry_run: false,
        if_revision: None,
    };
    let mut plan = prepare(&store, &request).unwrap();
    assert!(plan.path.starts_with(store.home.join(".codex/tmax")));
    assert!(!plan.path.starts_with(&store.root));
    storage::ensure_directory(&plan.path).unwrap();
    plan.path = scope(&store, &plan.path).unwrap();
    plan.target.scope = plan.path.clone();
    let (cancel, _receiver) = watch::channel(false);
    let flow = Arc::new(Flow {
        cancel,
        progress: Mutex::new(Progress {
            attempt: "attempt".into(),
            provider: "work".into(),
            state: "starting".into(),
            via: plan.via.clone(),
            path: plan.path.clone(),
            credential_owner: "codex",
            method: "browser".into(),
            expires_at: "later".into(),
            authorization_url: None,
            user_code: None,
            account: None,
            native_authorization_may_have_changed: false,
            code: None,
            message: None,
            save: "not_requested",
            activation: Value::Null,
        }),
    });
    (directory, store, plan, flow)
}

// Exercise the real protocol/state/commit path with an in-memory native peer.
// Completion may precede the start reply: RPC ordering cannot lose that event.
fn peer(store: Store, flow: Arc<Flow>, scenario: &'static str) -> Rpc {
    let (client, server) = tokio::io::duplex(64 * 1024);
    let (read, write) = tokio::io::split(client);
    tokio::spawn(async move {
        let (reader, mut writer) = tokio::io::split(server);
        let mut lines = BufReader::new(reader).lines();
        let mut signed_in = false;
        while let Ok(Some(line)) = lines.next_line().await {
            let message: Value = serde_json::from_str(&line).unwrap();
            if message.get("id").is_none() {
                continue;
            }
            let result = match message["method"].as_str().unwrap() {
                "initialize" => json!({}),
                "account/read" => {
                    json!({"account":if signed_in || scenario == "ambient" {json!({"type":"chatgpt","email":"person@example.test","planType":"plus"})}else{Value::Null}})
                }
                "account/login/start" => {
                    if scenario == "cancel" {
                        flow.cancel.send_replace(true);
                    }
                    if scenario == "edit" {
                        store.write_raw("tmax.toml", b"# concurrent user change\nversion = 1\n[providers.work]\nintegration = \"codex-subscription\"\n").unwrap();
                    }
                    let completion = json!({"method":"account/login/completed","params":{"loginId":"native-id","success":scenario != "refused","error":"sensitive-native-output-must-not-escape"}});
                    writer
                        .write_all(format!("{completion}\n").as_bytes())
                        .await
                        .unwrap();
                    signed_in = true;
                    json!({"type":"chatgpt","loginId":"native-id","authUrl":"https://auth.openai.com/oauth/authorize?state=challenge"})
                }
                _ => panic!("unexpected native method"),
            };
            let response = json!({"id":message["id"],"result":result});
            if writer
                .write_all(format!("{response}\n").as_bytes())
                .await
                .is_err()
            {
                break;
            }
        }
    });
    Rpc {
        child: None,
        reader: BufReader::new(Box::new(read)),
        writer: Box::new(write),
        sequence: 0,
        completed: None,
        login_id: None,
    }
}

#[tokio::test]
async fn browser_login_commits_only_completed_native_auth_and_preserves_comments() {
    let (_dir, store, plan, flow) = fixture();
    let mut rpc = peer(store.clone(), flow.clone(), "success");
    run_protocol(&store, &plan, &flow, &mut rpc).await.unwrap();
    let loaded = config::load(&store).unwrap();
    assert!(loaded.document.to_string().contains("# keep this comment"));
    assert!(loaded.config.agents.contains_key(&plan.via));
    assert!(
        matches!(&loaded.config.providers["work"].auth, Some(Auth::Login{via,session_ref:Some(_)}) if via == &plan.via)
    );
    assert!(loaded.config.providers["work"].models.is_empty());
    let logins: Logins = store.read_json(LOGINS).unwrap();
    assert_eq!(logins.entries[0].state, "signed_in");
    assert_eq!(
        logins.entries[0].scope,
        scope(&store, &loaded.config.agents[&plan.via].scope).unwrap()
    );
    assert_eq!(
        retained_status(&store, "work").unwrap()["state"],
        "succeeded"
    );
    let public = serde_json::to_string(&*flow.progress.lock().unwrap()).unwrap();
    assert!(!public.contains("sensitive-native-output"));
    assert!(!public.contains("state=challenge"));
}

#[tokio::test]
async fn browser_login_cancellation_and_concurrent_edit_never_commit() {
    for scenario in ["cancel", "edit", "refused"] {
        let (_dir, store, plan, flow) = fixture();
        let original = config::raw_revision(&store).unwrap();
        let mut rpc = peer(store.clone(), flow.clone(), scenario);
        let error = run_protocol(&store, &plan, &flow, &mut rpc)
            .await
            .unwrap_err();
        assert_eq!(
            error.code,
            match scenario {
                "cancel" => "cancelled",
                "edit" => "inputs_changed",
                _ => "login_failed",
            }
        );
        let loaded = config::load(&store).unwrap();
        assert!(loaded.config.providers["work"].auth.is_none());
        assert!(loaded.config.agents.is_empty());
        if scenario != "edit" {
            assert_eq!(loaded.revision, original);
        } else {
            assert!(
                loaded
                    .document
                    .to_string()
                    .contains("concurrent user change")
            );
        }
        assert!(!error.message.contains("sensitive-native-output"));
        assert_eq!(flow.progress.lock().unwrap().save, "not_requested");
    }
}

#[tokio::test]
async fn browser_login_does_not_adopt_ambient_or_replace_known_account() {
    for scenario in ["ambient", "different"] {
        let (_dir, store, mut plan, flow) = fixture();
        if scenario == "different" {
            plan.previous = Some(Account {
                email: Some("original@example.test".into()),
                plan: None,
            });
        }
        let original = config::raw_revision(&store).unwrap();
        let mut rpc = peer(store.clone(), flow.clone(), scenario);
        let error = run_protocol(&store, &plan, &flow, &mut rpc)
            .await
            .unwrap_err();
        assert_eq!(
            error.code,
            if scenario == "ambient" {
                "native_ambient_login_not_adopted"
            } else {
                "identity_mismatch"
            }
        );
        assert_eq!(config::raw_revision(&store).unwrap(), original);
        assert_eq!(
            flow.progress
                .lock()
                .unwrap()
                .native_authorization_may_have_changed,
            scenario != "ambient"
        );
        if scenario == "different" {
            let logins: Logins = store.read_json(LOGINS).unwrap();
            assert_eq!(logins.entries[0].account, plan.previous);
            assert_eq!(logins.entries[0].state, "native_effect_uncertain");
        }
    }
}

#[test]
fn browser_login_preview_has_no_process_or_scope_effects() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path().to_owned());
    store
        .write_raw(
            "tmax.toml",
            b"version=1\n[providers.p]\nintegration='codex-subscription'\n",
        )
        .unwrap();
    let request = Request {
        command: vec!["provider".into(), "login-start".into()],
        operands: vec!["p".into()],
        arguments: [("new-codex".into(), vec!["true".into()])].into(),
        dry_run: true,
        if_revision: None,
    };
    let before = config::raw_revision(&store).unwrap();
    let result = command(&store, &request).unwrap();
    assert_eq!(result["processes_started"], false);
    assert!(result["path"].is_null());
    assert_eq!(config::raw_revision(&store).unwrap(), before);
    assert!(!store.root.join("native").exists());
    assert!(!store.home.join(".codex").exists());
    assert!(!store.root.join("locks").exists());
}

#[test]
fn browser_login_challenges_reject_untrusted_urls_and_incomplete_device_codes() {
    for url in [
        "http://auth.openai.com/a",
        "https://auth.openai.com.evil.test/a",
        "https://user@auth.openai.com/a",
        "javascript:alert(1)",
    ] {
        assert!(challenge(&json!({"type":"chatgpt","authUrl":url,"loginId":"id"})).is_err());
    }
    assert!(challenge(&json!({"type":"chatgptDeviceCode","verificationUrl":"https://auth.openai.com/codex/device","loginId":"id"})).is_err());
    assert!(challenge(&json!({"type":"chatgptDeviceCode","verificationUrl":"https://auth.openai.com/codex/device","userCode":"ABCD-1234","loginId":"id"})).is_ok());
}

#[tokio::test]
async fn browser_login_rejects_unbounded_native_output() {
    let (client, mut server) = tokio::io::duplex(4096);
    let (reader, writer) = tokio::io::split(client);
    tokio::spawn(async move {
        let _ = server
            .write_all(&vec![b'x'; MESSAGE_BYTES as usize + 1])
            .await;
    });
    let mut rpc = Rpc {
        child: None,
        reader: BufReader::new(Box::new(reader)),
        writer: Box::new(writer),
        sequence: 0,
        completed: None,
        login_id: None,
    };
    assert_eq!(rpc.message().await.unwrap_err().code, "login_unavailable");
}

#[tokio::test]
async fn browser_login_cleared_authorization_does_not_return_cached_success() {
    let (_dir, store, plan, flow) = fixture();
    let mut rpc = peer(store.clone(), flow.clone(), "success");
    run_protocol(&store, &plan, &flow, &mut rpc).await.unwrap();
    flow.progress.lock().unwrap().state = "succeeded".into();
    let key = (store.root.clone(), "work".into());
    flows().lock().unwrap().insert(key.clone(), flow);
    let mut loaded = config::load(&store).unwrap();
    loaded.document["providers"]["work"]
        .as_table_mut()
        .unwrap()
        .remove("auth");
    config::save(&store, &loaded.revision, &loaded.document).unwrap();
    let request = Request {
        command: vec!["provider".into(), "login-status".into()],
        operands: vec!["work".into()],
        arguments: BTreeMap::new(),
        dry_run: false,
        if_revision: None,
    };
    assert_eq!(command(&store, &request).unwrap()["state"], "idle");
    flows().lock().unwrap().remove(&key);
}

#[tokio::test]
async fn browser_login_failed_private_scope_can_be_explicitly_retried() {
    let (_dir, store, plan, flow) = fixture();
    let mut rpc = peer(store.clone(), flow.clone(), "refused");
    assert_eq!(
        run_protocol(&store, &plan, &flow, &mut rpc)
            .await
            .unwrap_err()
            .code,
        "login_failed"
    );
    let request = Request {
        command: vec!["provider".into(), "login-start".into()],
        operands: vec!["work".into()],
        arguments: [("via".into(), vec![plan.via.clone()])].into(),
        dry_run: false,
        if_revision: None,
    };
    let retry = prepare(&store, &request).unwrap();
    assert_eq!(retry.path, plan.path);
    assert!(retry.managed);
    assert!(retry.new_scope);
    let mut rpc = peer(store.clone(), flow.clone(), "success");
    run_protocol(&store, &retry, &flow, &mut rpc).await.unwrap();
    assert_eq!(
        retained_status(&store, "work").unwrap()["state"],
        "succeeded"
    );
}

#[tokio::test]
async fn browser_login_recreated_provider_is_rejected_before_native_changes() {
    let (_dir, store, mut plan, flow) = fixture();
    plan.provider_identity = Some("removed-provider-identity".into());
    let mut rpc = peer(store.clone(), flow.clone(), "success");
    assert_eq!(
        run_protocol(&store, &plan, &flow, &mut rpc)
            .await
            .unwrap_err()
            .code,
        "inputs_changed"
    );
    assert!(
        !flow
            .progress
            .lock()
            .unwrap()
            .native_authorization_may_have_changed
    );
    assert!(
        config::load(&store).unwrap().config.providers["work"]
            .auth
            .is_none()
    );
}

// Isolate executable discovery in a child test process: parallel tests must not
// share PATH mutations, and the fake Codex never contacts an upstream service.
#[cfg(unix)]
#[tokio::test]
async fn browser_login_created_scope_runs_native_process_and_commits_only_completed_auth() {
    const TEST: &str = "native::browser_login::tests::browser_login_created_scope_runs_native_process_and_commits_only_completed_auth";
    if let Ok(scenario) = std::env::var("TMAX_BROWSER_LOGIN_TEST_CHILD") {
        let (_directory, store, mut plan, flow) = fixture();
        // Let run create the scope and its native credential-store configuration.
        fs::remove_dir(&plan.path).unwrap();
        let ambient = store.home.join(".codex/auth.json");
        fs::write(
            &ambient,
            b"ambient-native-authorization-must-remain-untouched",
        )
        .unwrap();
        let before = config::raw_revision(&store).unwrap();
        let mut process = None;
        let result = run(&store, &mut plan, &flow, &mut process).await;
        process
            .as_mut()
            .expect("fake native process started")
            .stop()
            .await;
        let native_config: toml::Value =
            toml::from_str(&fs::read_to_string(plan.path.join("config.toml")).unwrap()).unwrap();
        assert_eq!(
            native_config["cli_auth_credentials_store"].as_str(),
            Some("file")
        );
        assert_eq!(
            fs::read(&ambient).unwrap(),
            b"ambient-native-authorization-must-remain-untouched"
        );
        let loaded = config::load(&store).unwrap();
        if scenario == "success" {
            result.unwrap();
            assert_eq!(flow.progress.lock().unwrap().save, "saved");
            assert!(matches!(&loaded.config.providers["work"].auth,
                Some(Auth::Login { via, session_ref: Some(_) }) if via == &plan.via));
            let target = &loaded.config.agents[&plan.via];
            assert_eq!(scope(&store, &target.scope).unwrap(), plan.path);
            assert_eq!(
                retained_status(&store, "work").unwrap()["state"],
                "succeeded"
            );
        } else {
            assert_eq!(result.unwrap_err().code, "login_failed");
            assert_eq!(loaded.revision, before);
            assert!(loaded.config.providers["work"].auth.is_none());
            assert!(loaded.config.agents.is_empty());
            assert_eq!(flow.progress.lock().unwrap().save, "not_requested");
        }
        return;
    }

    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("codex");
    fs::write(&executable, r#"#!/bin/sh
if [ "$1" = --version ]; then
    printf 'codex-cli 0.159.2\n'
    exit 0
fi
[ "$1" = app-server ] || exit 1
signed_in=false
while IFS= read -r line; do
    case "$line" in *'"id":'*) ;; *) continue ;; esac
    id=${line#*\"id\":}
    id=${id%%,*}
    case "$line" in
        *'"method":"initialize"'*) result='{}' ;;
        *'"method":"account/read"'*)
            if [ "$signed_in" = true ]; then
                result='{"account":{"type":"chatgpt","email":"person@example.test","planType":"plus"}}'
            else
                result='{"account":null}'
            fi ;;
        *'"method":"account/login/start"'*)
            success=false
            [ "$TMAX_BROWSER_LOGIN_TEST_CHILD" = success ] && success=true
            printf '{"method":"account/login/completed","params":{"loginId":"native-id","success":%s}}\n' "$success"
            signed_in=true
            result='{"type":"chatgpt","loginId":"native-id","authUrl":"https://auth.openai.com/oauth/authorize?state=fixture"}' ;;
        *) exit 1 ;;
    esac
    printf '{"id":%s,"result":%s}\n' "$id" "$result"
done
"#).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let mut paths = vec![directory.path().to_owned()];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(paths).unwrap();
    for scenario in ["success", "refused"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env("TMAX_BROWSER_LOGIN_TEST_CHILD", scenario)
            .env("PATH", &path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{scenario}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn browser_login_existing_target_inside_tmax_storage_is_rejected() {
    let (_directory, store, _plan, _flow) = fixture();
    let path = store.root.join("native/user-selected");
    storage::ensure_directory(&path).unwrap();
    let mut loaded = config::load(&store).unwrap();
    loaded.document["agents"]["existing"]["agent"] = toml_edit::value("codex");
    loaded.document["agents"]["existing"]["scope"] =
        toml_edit::value(path.to_string_lossy().as_ref());
    config::save(&store, &loaded.revision, &loaded.document).unwrap();
    let request = Request {
        command: vec!["provider".into(), "login-start".into()],
        operands: vec!["work".into()],
        arguments: [("via".into(), vec!["existing".into()])].into(),
        dry_run: false,
        if_revision: None,
    };
    assert_eq!(
        prepare(&store, &request).err().unwrap().code,
        "invalid_native_scope"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn browser_login_generated_scope_rejects_symlinked_native_parent() {
    let (directory, store, mut plan, flow) = fixture();
    fs::remove_dir(&plan.path).unwrap();
    fs::remove_dir(private_root(&store)).unwrap();
    let elsewhere = directory.path().join("unowned");
    fs::create_dir(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, private_root(&store)).unwrap();
    let before = config::raw_revision(&store).unwrap();
    let mut process = None;
    assert!(run(&store, &mut plan, &flow, &mut process).await.is_err());
    assert!(process.is_none());
    assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
    assert_eq!(config::raw_revision(&store).unwrap(), before);
}
