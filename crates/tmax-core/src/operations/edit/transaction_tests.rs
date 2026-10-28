use super::*;
use std::collections::BTreeMap;

const EFFORT_MAP_CONFIG: &str = "# personal model settings\nversion=1\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:39999'\nresponses='http://127.0.0.1:39999'\n[providers.p.models.m]\npriority=7 # retained priority\n[providers.p.models.m.overrides]\nreasoning_options=[{type='effort',values=['low','high']}]\n";

fn effort_map_request(action: &str, operands: &[&str], protocol: &str) -> Request {
    Request {
        command: vec!["model".into(), "effort-map".into(), action.into()],
        operands: operands.iter().map(|value| (*value).into()).collect(),
        arguments: BTreeMap::from([("protocol".into(), vec![protocol.into()])]),
        dry_run: false,
        if_revision: None,
    }
}

#[tokio::test]
async fn effort_map_mutations_upsert_isolate_protocols_and_restore_identity() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().into());
    store
        .write_raw("tmax.toml", EFFORT_MAP_CONFIG.as_bytes())
        .unwrap();
    for (protocol, target) in [
        ("chat-completions", "high"),
        ("responses", "low"),
        ("chat-completions", "low"),
    ] {
        let request = effort_map_request("set", &["p/m", "xhigh", target], protocol);
        crate::operations::commands::validate(&request).unwrap();
        execute(&store, &request).await.unwrap();
    }
    let configured = config::load(&store).unwrap();
    let maps = &configured.config.providers["p"].models["m"].effort_map;
    assert_eq!(maps["chat-completions"]["xhigh"], "low");
    assert_eq!(maps["responses"]["xhigh"], "low");
    let saved = configured.document.to_string();
    assert!(saved.contains("# personal model settings"));
    assert!(saved.contains("priority=7 # retained priority"));
    let remove = effort_map_request("remove", &["p/m", "xhigh"], "chat-completions");
    execute(&store, &remove).await.unwrap();
    let configured = config::load(&store).unwrap();
    let model = &configured.config.providers["p"].models["m"];
    assert!(!model.effort_map.contains_key("chat-completions"));
    assert_eq!(model.effort_map["responses"]["xhigh"], "low");
    let body =
        crate::gateway::RequestFacts::from_value(json!({"reasoning_effort":"xhigh"})).unwrap();
    assert_eq!(
        crate::reasoning::resolve(model, "chat-completions", &body, None),
        Some("xhigh")
    );
    let bytes = std::fs::read(store.root.join("tmax.toml")).unwrap();
    assert_eq!(
        execute(&store, &remove).await.unwrap_err().code,
        "invalid_arguments"
    );
    assert_eq!(std::fs::read(store.root.join("tmax.toml")).unwrap(), bytes);
    let clear = effort_map_request("clear", &["p/m"], "responses");
    execute(&store, &clear).await.unwrap();
    let configured = config::load(&store).unwrap();
    assert!(
        configured.config.providers["p"].models["m"]
            .effort_map
            .is_empty()
    );
    assert!(!configured.document.to_string().contains("effort_map"));
    let bytes = std::fs::read(store.root.join("tmax.toml")).unwrap();
    assert_eq!(execute(&store, &clear).await.unwrap()["save"], "unchanged");
    assert_eq!(std::fs::read(store.root.join("tmax.toml")).unwrap(), bytes);
}

#[tokio::test]
async fn effort_map_dry_run_and_invalid_targets_never_persist() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().into());
    store
        .write_raw("tmax.toml", EFFORT_MAP_CONFIG.as_bytes())
        .unwrap();
    let mut request = effort_map_request("set", &["p/m", "xhigh", "high"], "chat-completions");
    request.dry_run = true;
    request.if_revision = Some(config::revision(EFFORT_MAP_CONFIG.as_bytes()));
    let preview = execute(&store, &request).await.unwrap();
    assert!(preview["changes"].as_array().unwrap().iter().any(|change| {
        change["boundary"] == "file_save"
            && change["path"]
                .as_str()
                .is_some_and(|path| path.contains("effort_map"))
    }));
    assert_eq!(
        std::fs::read(store.root.join("tmax.toml")).unwrap(),
        EFFORT_MAP_CONFIG.as_bytes()
    );
    for dry_run in [false, true] {
        request.dry_run = dry_run;
        request.operands[2] = "unsupported".into();
        let error = execute(&store, &request).await.unwrap_err();
        assert_eq!(error.code, "reasoning_effort_incompatible");
        assert_eq!(
            error.field.as_deref(),
            Some("providers.p.models.m.effort_map.chat-completions.xhigh")
        );
        assert_eq!(
            std::fs::read(store.root.join("tmax.toml")).unwrap(),
            EFFORT_MAP_CONFIG.as_bytes()
        );
    }
    request.operands[2] = "high".into();
    request.if_revision = Some("stale-revision".into());
    assert_eq!(
        execute(&store, &request).await.unwrap_err().code,
        "inputs_changed"
    );
    assert_eq!(
        std::fs::read(store.root.join("tmax.toml")).unwrap(),
        EFFORT_MAP_CONFIG.as_bytes()
    );
}

#[tokio::test]
async fn effort_maps_follow_provider_and_model_renames_and_clear_only_selected_protocol() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().into());
    store
        .write_raw("tmax.toml", EFFORT_MAP_CONFIG.as_bytes())
        .unwrap();
    for (protocol, target) in [("chat-completions", "high"), ("responses", "low")] {
        execute(
            &store,
            &effort_map_request("set", &["p/m", "xhigh", target], protocol),
        )
        .await
        .unwrap();
    }
    for (command, operands) in [
        (["provider", "rename"], ["p", "renamed"]),
        (["model", "rename"], ["renamed/m", "new/with/slash"]),
    ] {
        execute(&store, &request(&command, &operands, &[]))
            .await
            .unwrap();
    }
    let configured = config::load(&store).unwrap();
    assert!(!configured.config.providers.contains_key("p"));
    assert!(
        !configured.config.providers["renamed"]
            .models
            .contains_key("m")
    );
    let model = &configured.config.providers["renamed"].models["new/with/slash"];
    assert_eq!(model.effort_map["chat-completions"]["xhigh"], "high");
    assert_eq!(model.effort_map["responses"]["xhigh"], "low");
    assert!(
        configured
            .document
            .to_string()
            .contains("# personal model settings")
    );
    assert!(
        configured
            .document
            .to_string()
            .contains("# retained priority")
    );
    execute(
        &store,
        &effort_map_request("clear", &["renamed/new/with/slash"], "chat-completions"),
    )
    .await
    .unwrap();
    let configured = config::load(&store).unwrap();
    let model = &configured.config.providers["renamed"].models["new/with/slash"];
    assert!(!model.effort_map.contains_key("chat-completions"));
    assert_eq!(model.effort_map["responses"]["xhigh"], "low");
    assert_eq!(
        crate::reasoning::resolve(
            model,
            "chat-completions",
            &crate::gateway::RequestFacts::from_value(json!({"reasoning_effort":"xhigh"})).unwrap(),
            None,
        ),
        Some("xhigh")
    );
}

#[tokio::test]
async fn legacy_gateway_edit_previews_without_writes_and_retains_shared_key() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::at(dir.path().into());
    let text = "# personal settings\nversion=1\n[proxy]\nlisten='127.0.0.1:17840'\nkey_ref='retained-reference'\nautostart=false\n";
    store.write_raw("tmax.toml", text.as_bytes()).unwrap();
    let mut request = Request {
        command: vec!["gateway".into(), "configure".into()],
        operands: vec![],
        arguments: BTreeMap::from([("listen".into(), vec!["127.0.0.1:17842".into()])]),
        dry_run: true,
        if_revision: Some(config::revision(text.as_bytes())),
    };
    let preview = execute(&store, &request).await.unwrap();
    assert_eq!(
        preview["requested"]["configuration_migration"],
        "gateway_names"
    );
    assert_eq!(
        std::fs::read(store.root.join("tmax.toml")).unwrap(),
        text.as_bytes()
    );
    request.dry_run = false;
    let result = execute(&store, &request).await.unwrap();
    assert_eq!(result["configuration_migration"], "gateway_names");
    assert_eq!(result["save"], "saved");
    let saved = config::load(&store).unwrap();
    assert_eq!(
        saved.config.gateway.as_ref().unwrap().listen,
        "127.0.0.1:17842"
    );
    assert_eq!(
        saved.config.gateway.as_ref().unwrap().key_ref,
        "retained-reference"
    );
    assert!(!saved.document.to_string().contains("[proxy]"));
}
fn request(command: &[&str], operands: &[&str], sets: &[&str]) -> Request {
    Request {
        command: command.iter().map(|s| s.to_string()).collect(),
        operands: operands.iter().map(|s| s.to_string()).collect(),
        arguments: BTreeMap::from([("set".into(), sets.iter().map(|s| s.to_string()).collect())]),
        dry_run: false,
        if_revision: None,
    }
}
#[cfg(unix)]
#[tokio::test]
async fn protected_web_key_descriptor_saves_existing_provider_with_signed_rules() {
    use std::os::fd::AsRawFd;
    let base = "# signed fixture\nversion=1\n[providers.synthetic]\nintegration='custom'\n[providers.synthetic.endpoints]\nchat-completions='http://127.0.0.1:39999'\n[providers.synthetic.models.signed-priority]\npriority=-10\ntimezone='UTC'\nschedule=[{all_day=true,priority=-10,weekdays=['mon']}]\npriority_rules=[{id='synthetic-negative',input='weekly',priority=-10,remaining_at_least_percent='1',reset_within_seconds=86400}]\n[providers.synthetic.inputs.weekly]\nkind='quota'\nmax_age_seconds=3600\nmodels=['signed-priority']\nscope='synthetic-scope'\nsource='manual'\n";
    for existing_auth in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let text = if existing_auth {
            format!(
                "{base}\n[providers.synthetic.auth]\nmethod='api-key'\nkey_ref='opaque-existing-reference'\n"
            )
        } else {
            base.into()
        };
        store.write_raw("tmax.toml", text.as_bytes()).unwrap();
        let config = config::parse(&text).unwrap();
        crate::routing::reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        let (reader, mut writer) = std::io::pipe().unwrap();
        std::io::Write::write_all(&mut writer, b"synthetic-web-form-key\n").unwrap();
        drop(writer);
        let request = Request {
            command: vec!["provider".into(), "key".into(), "set".into()],
            operands: vec!["synthetic".into()],
            arguments: BTreeMap::from([
                ("key-fd".into(), vec![reader.as_raw_fd().to_string()]),
                ("non-interactive".into(), vec!["true".into()]),
            ]),
            dry_run: false,
            if_revision: None,
        };
        crate::operations::commands::validate(&request).unwrap();
        let result = execute(&store, &request).await.unwrap();
        assert_eq!(result["save"], "saved");
        let saved = config::load(&store).unwrap();
        let Some(config::Auth::ApiKey { key_ref }) = &saved.config.providers["synthetic"].auth
        else {
            panic!("API key authentication must be saved")
        };
        assert_eq!(
            crate::secrets::get(&store, key_ref).unwrap().as_slice(),
            b"synthetic-web-form-key"
        );
        assert_eq!(
            saved.config.providers["synthetic"].models["signed-priority"].priority,
            -10
        );
        assert!(
            std::fs::read_to_string(store.path("tmax.toml").unwrap())
                .unwrap()
                .contains("# signed fixture")
        );
    }
}
#[cfg(unix)]
#[tokio::test]
async fn candidate_effort_rejection_precedes_protected_input_and_credential_lineage_changes() {
    use std::os::fd::AsRawFd;
    for existing_auth in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let mut configured = config::parse("version=1\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:39999'\n[providers.p.models.m.overrides]\nreasoning_options=[{type='effort',values=['high']}]\n[[model_aliases]]\nid='fixed'\nprotocol='chat-completions'\ncandidates=['p/m']\ncandidate_reasoning_effort={'p/m'='high'}\n").unwrap();
        store
            .write_raw(
                "tmax.toml",
                toml::to_string(&configured).unwrap().as_bytes(),
            )
            .unwrap();
        crate::routing::reconcile(&store, &Config::empty(), &configured, &[]).unwrap();
        let original_reference = if existing_auth {
            let owner = provider_owner(&store, &configured, "p").unwrap();
            let reference = crate::secrets::put(&store, &owner, b"original-provider-key").unwrap();
            let previous = configured.clone();
            configured.providers.get_mut("p").unwrap().auth = Some(config::Auth::ApiKey {
                key_ref: reference.clone(),
            });
            store
                .write_raw(
                    "tmax.toml",
                    toml::to_string(&configured).unwrap().as_bytes(),
                )
                .unwrap();
            crate::routing::reconcile(&store, &previous, &configured, &[]).unwrap();
            Some(reference)
        } else {
            None
        };
        configured
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .overrides
            .reasoning_options = Some(vec![config::ReasoningOption {
            r#type: "effort".into(),
            values: Some(vec!["low".into()]),
        }]);
        let invalid_authority = toml::to_string(&configured).unwrap();
        store
            .write_raw("tmax.toml", invalid_authority.as_bytes())
            .unwrap();
        let inventory_before = crate::secrets::inventory(&store).unwrap();
        let lineage_before = std::fs::read(store.root.join("routing-state.json")).unwrap();
        let destination_before =
            crate::routing::destination_identity(&store, &configured, "p/m").unwrap();
        let (mut reader, mut writer) = std::io::pipe().unwrap();
        std::io::Write::write_all(&mut writer, b"replacement-provider-key\n").unwrap();
        drop(writer);
        let request = Request {
            command: vec!["provider".into(), "key".into(), "set".into()],
            operands: vec!["p".into()],
            arguments: BTreeMap::from([
                ("key-fd".into(), vec![reader.as_raw_fd().to_string()]),
                ("non-interactive".into(), vec!["true".into()]),
            ]),
            dry_run: false,
            if_revision: None,
        };
        assert_eq!(
            execute(&store, &request).await.unwrap_err().code,
            "reasoning_effort_incompatible"
        );
        let mut unread = Vec::new();
        std::io::Read::read_to_end(&mut reader, &mut unread).unwrap();
        assert_eq!(unread, b"replacement-provider-key\n");
        assert_eq!(crate::secrets::inventory(&store).unwrap(), inventory_before);
        assert_eq!(
            std::fs::read(store.root.join("routing-state.json")).unwrap(),
            lineage_before
        );
        assert_eq!(
            crate::routing::destination_identity(&store, &configured, "p/m").unwrap(),
            destination_before
        );
        assert_eq!(
            std::fs::read(store.root.join("tmax.toml")).unwrap(),
            invalid_authority.as_bytes()
        );
        if let Some(reference) = original_reference {
            assert_eq!(
                crate::secrets::get(&store, &reference).unwrap().as_slice(),
                b"original-provider-key"
            );
        }
    }
}

#[tokio::test]
async fn candidate_effort_edits_validate_capabilities_and_track_membership_and_renames() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path().into());
    store.write_raw("tmax.toml", b"version=1\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:9999/v1'\nresponses='http://127.0.0.1:9999/v1'\n[providers.p.models.a.overrides]\nreasoning_options=[{type='effort',values=['high','max']}]\n[providers.p.models.b.overrides]\nreasoning=true\nreasoning_options=[{type='toggle'}]\n").unwrap();
    let mut request = Request {
        command: vec!["model".into(), "alias".into(), "add".into()],
        operands: vec!["route".into()],
        arguments: BTreeMap::from([
            ("protocol".into(), vec!["chat-completions".into()]),
            ("candidate".into(), vec!["p/a".into(), "p/b".into()]),
            ("candidate-reasoning-effort".into(), vec!["p/a=high".into()]),
        ]),
        dry_run: false,
        if_revision: None,
    };
    execute(&store, &request).await.unwrap();
    request.command[2] = "update".into();
    request.arguments.remove("candidate");
    let saved = std::fs::read(store.root.join("tmax.toml")).unwrap();
    for (assignment, code) in [
        ("p/a=xhigh", "reasoning_effort_incompatible"),
        ("p/b=max", "reasoning_effort_unavailable"),
        ("p/missing=high", "invalid_configuration"),
    ] {
        request
            .arguments
            .insert("candidate-reasoning-effort".into(), vec![assignment.into()]);
        assert_eq!(execute(&store, &request).await.unwrap_err().code, code);
        assert_eq!(std::fs::read(store.root.join("tmax.toml")).unwrap(), saved);
    }
    request.arguments.insert(
        "candidate-reasoning-effort".into(),
        vec!["p/a=high".into(), "p/a=max".into()],
    );
    assert_eq!(
        execute(&store, &request).await.unwrap_err().code,
        "invalid_arguments"
    );
    let rename = Request {
        command: vec!["provider".into(), "rename".into()],
        operands: vec!["p".into(), "renamed".into()],
        arguments: BTreeMap::new(),
        dry_run: false,
        if_revision: None,
    };
    execute(&store, &rename).await.unwrap();
    let rename = Request {
        command: vec!["model".into(), "rename".into()],
        operands: vec!["renamed/a".into(), "new/model".into()],
        arguments: BTreeMap::new(),
        dry_run: false,
        if_revision: None,
    };
    execute(&store, &rename).await.unwrap();
    let alias = config::load(&store).unwrap().config.model_aliases.remove(0);
    assert_eq!(
        alias.candidate_reasoning_effort,
        BTreeMap::from([("renamed/new/model".into(), "high".into())])
    );
    request.arguments.remove("candidate-reasoning-effort");
    request.arguments.insert(
        "clear-candidate-reasoning-effort".into(),
        vec!["renamed/new/model".into()],
    );
    execute(&store, &request).await.unwrap();
    assert!(
        config::load(&store).unwrap().config.model_aliases[0]
            .candidate_reasoning_effort
            .is_empty()
    );
    request.arguments.remove("clear-candidate-reasoning-effort");
    request.arguments.insert(
        "candidate-reasoning-effort".into(),
        vec!["renamed/new/model=max".into()],
    );
    execute(&store, &request).await.unwrap();
    request.arguments.remove("candidate-reasoning-effort");
    request
        .arguments
        .insert("candidate".into(), vec!["renamed/b".into()]);
    execute(&store, &request).await.unwrap();
    assert!(
        config::load(&store).unwrap().config.model_aliases[0]
            .candidate_reasoning_effort
            .is_empty()
    );
}
#[tokio::test]
async fn alias_edits_replace_members_preserve_references_and_reject_partial_invalid_saves() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path().into());
    store.write_raw("tmax.toml", b"# personal settings\nversion=1\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:9999/v1'\n[providers.p.models.a]\npriority=7\n[providers.p.models.b]\n").unwrap();
    let alias_request = |tail: &[&str], candidates: &[&str]| Request {
        command: ["model", "alias"]
            .into_iter()
            .chain(tail.iter().copied())
            .map(String::from)
            .collect(),
        operands: vec!["L9".into()],
        arguments: BTreeMap::from([
            ("protocol".into(), vec!["chat-completions".into()]),
            (
                "candidate".into(),
                candidates.iter().map(|s| s.to_string()).collect(),
            ),
        ]),
        dry_run: false,
        if_revision: None,
    };
    let original = std::fs::read(store.root.join("tmax.toml")).unwrap();
    let missing = execute(&store, &alias_request(&["add"], &[]))
        .await
        .unwrap_err();
    assert_eq!(missing.code, "invalid_arguments");
    assert_eq!(missing.field.as_deref(), Some("candidate"));
    assert_eq!(
        std::fs::read(store.root.join("tmax.toml")).unwrap(),
        original
    );
    execute(&store, &alias_request(&["add"], &["p/a"]))
        .await
        .unwrap();
    execute(&store, &alias_request(&["update"], &["p/b", "p/a"]))
        .await
        .unwrap();
    assert_eq!(
        config::load(&store).unwrap().config.model_aliases[0].candidates,
        ["p/b", "p/a"]
    );
    let mut disable = alias_request(&["update"], &[]);
    disable
        .arguments
        .insert("set".into(), vec!["enabled".into(), "false".into()]);
    execute(&store, &disable).await.unwrap();
    let loaded = config::load(&store).unwrap();
    assert!(!loaded.config.model_aliases[0].enabled);
    assert_eq!(loaded.config.model_aliases[0].candidates, ["p/b", "p/a"]);
    for invalid in [
        alias_request(&["candidates", "add"], &["p/a"]),
        alias_request(&["candidates", "add"], &["missing/model"]),
        alias_request(&["candidates", "remove"], &["p/a", "p/b"]),
        request(&["model", "update"], &["p/a"], &["tier", "L0"]),
    ] {
        let before = std::fs::read(store.root.join("tmax.toml")).unwrap();
        assert!(execute(&store, &invalid).await.is_err());
        assert_eq!(std::fs::read(store.root.join("tmax.toml")).unwrap(), before);
    }
    execute(&store, &alias_request(&["update"], &["p/b"]))
        .await
        .unwrap();
    assert_eq!(
        config::load(&store).unwrap().config.model_aliases[0].candidates,
        ["p/b"]
    );
    execute(&store, &alias_request(&["candidates", "add"], &["p/a"]))
        .await
        .unwrap();
    execute(&store, &alias_request(&["candidates", "remove"], &["p/b"]))
        .await
        .unwrap();
    assert_eq!(
        config::load(&store).unwrap().config.model_aliases[0].candidates,
        ["p/a"]
    );
    assert!(
        execute(&store, &request(&["provider", "remove"], &["p"], &[]))
            .await
            .is_err()
    );
    execute(
        &store,
        &request(&["model", "rename"], &["p/a", "new/with/slash"], &[]),
    )
    .await
    .unwrap();
    execute(
        &store,
        &request(&["provider", "rename"], &["p", "renamed"], &[]),
    )
    .await
    .unwrap();
    let loaded = config::load(&store).unwrap();
    assert_eq!(
        loaded.config.model_aliases[0].candidates,
        ["renamed/new/with/slash"]
    );
    assert_eq!(
        loaded.config.providers["renamed"].models["new/with/slash"].priority,
        7
    );
    assert!(loaded.document.to_string().contains("# personal settings"));
}

#[tokio::test]
async fn actual_update_preserves_inline_siblings_comments_and_reports_no_service() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path().to_path_buf());
    std::fs::create_dir_all(&store.root).unwrap();
    std::fs::write(store.root.join("tmax.toml"),"# owner comment\nversion=1\n[providers.p]\nintegration='deepseek'\noverrides = { name = 'Personal', doc = 'old' } # preserve inline\n").unwrap();
    let result = execute(
        &store,
        &request(&["provider", "update"], &["p"], &["overrides.doc", "new"]),
    )
    .await
    .unwrap();
    let text = std::fs::read_to_string(store.root.join("tmax.toml")).unwrap();
    let loaded = config::load(&store).unwrap();
    assert_eq!(
        loaded.config.providers["p"].overrides.name.as_deref(),
        Some("Personal")
    );
    assert_eq!(
        loaded.config.providers["p"].overrides.doc.as_deref(),
        Some("new")
    );
    assert!(text.contains("# owner comment"));
    assert!(text.contains("# preserve inline"));
    assert_eq!(result["runtime"], "not_running");
    assert!(!store.root.join("autostart.json").exists());
}
#[tokio::test]
async fn stale_guard_and_dry_run_never_initialize_missing_home() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::at(temp.path().join("missing"));
    let mut r = Request {
        command: vec!["provider".into(), "add".into()],
        operands: vec!["p".into()],
        arguments: BTreeMap::from([("integration".into(), vec!["deepseek".into()])]),
        dry_run: true,
        if_revision: Some("absent".into()),
    };
    execute(&store, &r).await.unwrap();
    assert!(!store.root.exists());
    r.if_revision = Some("stale".into());
    assert_eq!(
        execute(&store, &r).await.unwrap_err().code,
        "inputs_changed"
    );
    assert!(!store.root.exists());
}
