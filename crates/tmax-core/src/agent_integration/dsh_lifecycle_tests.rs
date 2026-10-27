//! Cross-owner DSH application/restoration lifecycle.
use super::{
    application::apply,
    document::{self, Format},
    dsh_patch,
    recovery::{INDEX, Index, restore},
    test_support::inspection_target,
};
use crate::{
    config::{self, Auth, Config},
    secrets,
    storage::{self, Store},
};
use serde_json::json;
use std::{collections::BTreeMap, fs};

#[cfg(unix)]
#[tokio::test]
async fn dsh_apply_restore_preserves_native_preferences_and_shared_credentials() {
    const CHILD: &str = "TMAX_DSH_LIFECYCLE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("dsh");
        fs::write(
            &executable,
            "#!/bin/sh\n[ \"$1\" = --version ] || exit 1\nprintf '0.2.1-rc.2\\n'\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "agent_integration::dsh_lifecycle_tests::dsh_apply_restore_preserves_native_preferences_and_shared_credentials", "--nocapture"])
            .env(CHILD, "1")
            .env("PATH", directory.path())
            .output().unwrap();
        assert!(
            output.status.success(),
            "isolated DSH lifecycle failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path().join("authority"));
    let home = directory.path().join("dsh");
    let path = home.clone();
    fs::create_dir_all(&path).unwrap();
    let native_profile = home.join("profiles/custom");
    fs::create_dir_all(&native_profile).unwrap();
    fs::write(
        native_profile.join("package.json"),
        "native-owned opaque manifest",
    )
    .unwrap();
    let profile_patch = "- id: llm-pi-ai\n  config: !!js process.env.NATIVE_PROVIDERS\n";
    fs::write(native_profile.join("cordis.patch.yml"), profile_patch).unwrap();
    let original = "# native policy\n- id: agent-default-model\n  config:\n    provider: existing\n    model: unchanged\n    reasoningEffort: low\n- id: user-owned\n  config: !!js process.env.NATIVE_POLICY\n";
    fs::write(path.join("cordis.patch.yml"), original).unwrap();
    let credentials = home.join(".credentials.yaml");
    storage::atomic_write(
        &credentials,
        b"version: 1\nrefs:\n  EXISTING: synthetic-existing\nrecords: {}\n",
    )
    .unwrap();
    let reference = secrets::put(&store, "provider:p", b"synthetic-managed").unwrap();
    let mut target = inspection_target(path.clone());
    target.agent = "dsh".into();
    target.default = None;
    let mut config = Config::empty();
    config.providers.insert(
        "p".into(),
        config::Provider {
            integration: Some("custom".into()),
            auth: Some(Auth::ApiKey { key_ref: reference }),
            endpoints: BTreeMap::from([(
                "chat-completions".into(),
                "https://example.invalid/v1".into(),
            )]),
            models: BTreeMap::from([("first".into(), config::Model::default())]),
            ..Default::default()
        },
    );
    config.agents.insert("managed".into(), target);
    store
        .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
        .unwrap();
    let before_credentials = fs::read(&credentials).unwrap();
    let credentials_lock = home.join(".credentials.yaml.lock");
    assert!(!credentials_lock.exists());
    let override_patch = format!(
        "{original}- insert:\n    - id: llm-tmax\n      config:\n        providers:\n          local:\n            modelOverrides:\n              first:\n                contextWindow: 4096\n"
    );
    fs::write(path.join("cordis.patch.yml"), &override_patch).unwrap();
    for dry_run in [true, false] {
        assert_eq!(
            apply(&store, &config, "managed", dry_run)
                .await
                .unwrap_err()
                .code,
            "native_config_incompatible"
        );
        assert!(!credentials_lock.exists());
        assert_eq!(
            fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
            override_patch
        );
        assert_eq!(fs::read(&credentials).unwrap(), before_credentials);
    }
    fs::write(path.join("cordis.patch.yml"), original).unwrap();
    let preview = apply(&store, &config, "managed", true).await.unwrap();
    assert!(!preview.to_string().contains("synthetic-managed"));
    assert_eq!(
        fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
        original
    );
    assert_eq!(fs::read(&credentials).unwrap(), before_credentials);
    assert!(!credentials_lock.exists());
    let external_lock = format!("{}\n", std::process::id());
    storage::atomic_write(&credentials_lock, external_lock.as_bytes()).unwrap();
    apply(&store, &config, "managed", true).await.unwrap();
    assert_eq!(
        apply(&store, &config, "managed", false)
            .await
            .unwrap_err()
            .code,
        "native_credentials_busy"
    );
    assert_eq!(
        fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
        original
    );
    assert_eq!(fs::read(&credentials).unwrap(), before_credentials);
    assert_eq!(
        fs::read_to_string(&credentials_lock).unwrap(),
        external_lock
    );
    fs::remove_file(&credentials_lock).unwrap();
    apply(&store, &config, "managed", false).await.unwrap();
    assert!(!credentials_lock.exists());
    let applied = fs::read_to_string(path.join("cordis.patch.yml")).unwrap();
    assert!(applied.starts_with(original));
    assert!(!applied.contains("synthetic-managed"));
    assert_eq!(
        dsh_patch::parse(&applied).unwrap()["credentials"]["disabled"],
        false
    );
    assert!(home.join("tmax-dsh.mjs").is_file());
    let native_credentials =
        document::parse(&fs::read_to_string(&credentials).unwrap(), Format::Yaml).unwrap();
    assert_eq!(native_credentials["refs"]["EXISTING"], "synthetic-existing");
    assert!(
        native_credentials["refs"]
            .as_object()
            .unwrap()
            .values()
            .any(|v| v == "synthetic-managed")
    );
    let mut renamed = config.clone();
    let target = renamed.agents.get_mut("managed").unwrap();
    let provider = target.providers.remove("local").unwrap();
    target.providers.insert("tmax".into(), provider);
    store
        .write_raw("tmax.toml", toml::to_string(&renamed).unwrap().as_bytes())
        .unwrap();
    let user_settings = dsh_patch::set(
        &applied,
        &[
            "llm-tmax".into(),
            "config".into(),
            "providers".into(),
            "local".into(),
            "compat".into(),
        ],
        Some(&json!({"supportsDeveloperRole":false})),
    )
    .unwrap();
    fs::write(path.join("cordis.patch.yml"), &user_settings).unwrap();
    let credentials_before_rename = fs::read(&credentials).unwrap();
    assert_eq!(
        apply(&store, &renamed, "managed", false)
            .await
            .unwrap_err()
            .code,
        "native_conflict"
    );
    assert_eq!(
        fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
        user_settings
    );
    assert_eq!(fs::read(&credentials).unwrap(), credentials_before_rename);
    assert!(!credentials_lock.exists());
    fs::write(path.join("cordis.patch.yml"), &applied).unwrap();
    apply(&store, &renamed, "managed", false).await.unwrap();
    let renamed_patch = fs::read_to_string(path.join("cordis.patch.yml")).unwrap();
    let renamed_value = dsh_patch::parse(&renamed_patch).unwrap();
    let providers = &renamed_value["llm-tmax"]["config"]["providers"];
    assert!(
        providers.get("local").is_none(),
        "removed route must not remain registered"
    );
    assert_eq!(providers["tmax"]["api"], "openai-completions");
    assert!(renamed_patch.starts_with(original));
    store
        .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
        .unwrap();
    apply(&store, &config, "managed", false).await.unwrap();
    let applied = fs::read_to_string(path.join("cordis.patch.yml")).unwrap();
    assert!(
        dsh_patch::parse(&applied).unwrap()["llm-tmax"]["config"]["providers"]
            .get("tmax")
            .is_none()
    );
    let index: Index = store.read_json(INDEX).unwrap();
    let unowned = dsh_patch::set(
        &applied,
        &["llm-tmax".into(), "config".into(), "custom".into()],
        Some(&json!("keep")),
    )
    .unwrap();
    fs::write(path.join("cordis.patch.yml"), &unowned).unwrap();
    let before_conflict_credentials = fs::read(&credentials).unwrap();
    let before_conflict_plugin = fs::read(home.join("tmax-dsh.mjs")).unwrap();
    assert_eq!(
        restore(&store, &index.records[0].id, false)
            .await
            .unwrap_err()
            .code,
        "native_conflict"
    );
    assert!(!credentials_lock.exists());
    assert_eq!(
        fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
        unowned
    );
    assert_eq!(fs::read(&credentials).unwrap(), before_conflict_credentials);
    assert_eq!(
        fs::read(home.join("tmax-dsh.mjs")).unwrap(),
        before_conflict_plugin
    );
    fs::write(path.join("cordis.patch.yml"), &applied).unwrap();
    assert_eq!(
        fs::read_to_string(native_profile.join("cordis.patch.yml")).unwrap(),
        profile_patch
    );
    fs::write(
        path.join("cordis.patch.yml"),
        format!("{applied}- id: later\n  disabled: true\n"),
    )
    .unwrap();
    let index: Index = store.read_json(INDEX).unwrap();
    let before_restore_patch = fs::read(path.join("cordis.patch.yml")).unwrap();
    let before_restore_credentials = fs::read(&credentials).unwrap();
    restore(&store, &index.records[0].id, true).await.unwrap();
    assert!(!credentials_lock.exists());
    storage::atomic_write(&credentials_lock, external_lock.as_bytes()).unwrap();
    restore(&store, &index.records[0].id, true).await.unwrap();
    assert_eq!(
        restore(&store, &index.records[0].id, false)
            .await
            .unwrap_err()
            .code,
        "native_credentials_busy"
    );
    assert_eq!(
        fs::read(path.join("cordis.patch.yml")).unwrap(),
        before_restore_patch
    );
    assert_eq!(fs::read(&credentials).unwrap(), before_restore_credentials);
    assert_eq!(
        fs::read_to_string(&credentials_lock).unwrap(),
        external_lock
    );
    fs::remove_file(&credentials_lock).unwrap();
    restore(&store, &index.records[0].id, false).await.unwrap();
    assert!(!credentials_lock.exists());
    assert!(!home.join("tmax-dsh.mjs").exists());
    assert_eq!(
        fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
        format!("{original}- id: later\n  disabled: true\n")
    );
    assert_eq!(
        document::parse(&fs::read_to_string(&credentials).unwrap(), Format::Yaml).unwrap(),
        json!({"version":1,"refs":{"EXISTING":"synthetic-existing"},"records":{}})
    );
    assert_eq!(
        fs::read_to_string(native_profile.join("cordis.patch.yml")).unwrap(),
        profile_patch
    );
    assert_eq!(
        fs::read_to_string(native_profile.join("package.json")).unwrap(),
        "native-owned opaque manifest"
    );
    for empty in ["", "# empty native credential store\n", "{}\n"] {
        storage::atomic_write(&credentials, empty.as_bytes()).unwrap();
        apply(&store, &config, "managed", false).await.unwrap();
        assert!(!credentials_lock.exists());
        let stamped =
            document::parse(&fs::read_to_string(&credentials).unwrap(), Format::Yaml).unwrap();
        assert_eq!(stamped["version"], 1);
        assert_eq!(stamped["refs"].as_object().unwrap().len(), 1);
        let index: Index = store.read_json(INDEX).unwrap();
        restore(&store, &index.records.last().unwrap().id, false)
            .await
            .unwrap();
        assert!(!credentials_lock.exists());
        assert_eq!(
            document::parse(&fs::read_to_string(&credentials).unwrap(), Format::Yaml).unwrap(),
            json!({})
        );
        assert_eq!(
            fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
            format!("{original}- id: later\n  disabled: true\n")
        );
    }
    for incompatible in ["refs: {}\n", "version: 2\n"] {
        storage::atomic_write(&credentials, incompatible.as_bytes()).unwrap();
        let before_patch = fs::read(path.join("cordis.patch.yml")).unwrap();
        assert_eq!(
            apply(&store, &config, "managed", false)
                .await
                .unwrap_err()
                .code,
            "native_schema_incompatible"
        );
        assert!(!credentials_lock.exists());
        assert_eq!(
            fs::read(path.join("cordis.patch.yml")).unwrap(),
            before_patch
        );
        assert_eq!(fs::read_to_string(&credentials).unwrap(), incompatible);
    }
    storage::atomic_write(&credentials, &before_credentials).unwrap();
    apply(&store, &config, "managed", false).await.unwrap();
    let owned = fs::read_to_string(path.join("cordis.patch.yml")).unwrap();
    let modified = dsh_patch::set(
        &owned,
        &[
            "llm-tmax".into(),
            "config".into(),
            "providers".into(),
            "local".into(),
            "api".into(),
        ],
        Some(&json!("anthropic-messages")),
    )
    .unwrap();
    fs::write(path.join("cordis.patch.yml"), &modified).unwrap();
    assert_eq!(
        apply(&store, &config, "managed", false)
            .await
            .unwrap_err()
            .code,
        "native_conflict"
    );
    assert!(!credentials_lock.exists());
    let before_failed_restore_credentials = fs::read(&credentials).unwrap();
    let index: Index = store.read_json(INDEX).unwrap();
    assert_eq!(
        restore(&store, &index.records.last().unwrap().id, false)
            .await
            .unwrap_err()
            .code,
        "native_conflict"
    );
    assert!(!credentials_lock.exists());
    assert_eq!(
        fs::read(&credentials).unwrap(),
        before_failed_restore_credentials
    );
    assert_eq!(
        fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
        modified
    );
}
