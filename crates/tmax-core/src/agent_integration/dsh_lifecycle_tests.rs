//! Cross-owner DSH application/restoration lifecycle around the one bundle plugin.
use super::{
    application::apply,
    document::{self, Format},
    dsh_patch,
    planning::inspect,
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
async fn dsh_apply_configures_the_installed_plugin_retires_the_embedded_one_and_restores() {
    const CHILD: &str = "TMAX_DSH_LIFECYCLE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("dsh");
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\n[ \"$1\" = --version ] || exit 1\nprintf '{}\\n'\n",
                super::adapters::DSH_QUALIFIED
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "agent_integration::dsh_lifecycle_tests::dsh_apply_configures_the_installed_plugin_retires_the_embedded_one_and_restores", "--nocapture"])
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
    // The one DSH plugin is the bundle DSH installs into a profile; a home
    // without such a profile has nothing for apply to configure.
    let bundle_profile = home.join("profiles/desktop");
    fs::create_dir_all(bundle_profile.join("node_modules/tmax")).unwrap();
    fs::write(
        bundle_profile.join("package.json"),
        r#"{"name":"desktop","dsh":{"profile":{"bundles":["tmax"]}}}"#,
    )
    .unwrap();
    fs::write(
        bundle_profile.join("node_modules/tmax/package.json"),
        r#"{"name":"tmax","version":"0.1.0"}"#,
    )
    .unwrap();
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
    // A home last touched by the release that embedded its own adapter.
    let legacy_plugin = home.join("tmax-dsh.mjs");
    let legacy_plugin_bytes = b"export const name = 'llm-tmax'\n";
    fs::write(&legacy_plugin, legacy_plugin_bytes).unwrap();
    let legacy = format!(
        "{original}- insert:\n    - id: llm-tmax\n      name: {}/tmax-dsh.mjs\n      config:\n        providers:\n          local:\n            models:\n              - id: first\n",
        path.display()
    );
    fs::write(path.join("cordis.patch.yml"), &legacy).unwrap();
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
    let credentials_lock = home.join(".credentials.yaml.lock");
    assert!(!credentials_lock.exists());

    // Inspection names the plugin the way apply will find it.
    let inspected = inspect(&store, &config, "managed").await.unwrap();
    assert_eq!(inspected["plugin"]["state"], "installed");
    let desktop = inspected["plugin"]["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["profile"] == "desktop")
        .unwrap();
    assert_eq!(desktop["listed"], true);
    assert_eq!(desktop["version"], "0.1.0");
    assert!(
        !inspected["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["code"] == "dsh_plugin_not_installed")
    );

    // A dry run writes nothing and never shows the key.
    let before_credentials = fs::read(&credentials).unwrap();
    let preview = apply(&store, &config, "managed", true).await.unwrap();
    assert!(!preview.to_string().contains("synthetic-managed"));
    assert_eq!(
        fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
        legacy
    );
    assert_eq!(fs::read(&credentials).unwrap(), before_credentials);
    assert_eq!(fs::read(&legacy_plugin).unwrap(), legacy_plugin_bytes);

    // A foreign credentials lock blocks before any write.
    let external_lock = format!("{}\n", std::process::id());
    storage::atomic_write(&credentials_lock, external_lock.as_bytes()).unwrap();
    assert_eq!(
        apply(&store, &config, "managed", false)
            .await
            .unwrap_err()
            .code,
        "native_credentials_busy"
    );
    assert_eq!(
        fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
        legacy
    );
    assert_eq!(fs::read(&credentials).unwrap(), before_credentials);
    assert_eq!(
        fs::read_to_string(&credentials_lock).unwrap(),
        external_lock
    );
    fs::remove_file(&credentials_lock).unwrap();

    // Apply configures the bundle entry, hands over the credential reference,
    // and retires the embedded plugin row and file.
    apply(&store, &config, "managed", false).await.unwrap();
    assert!(!credentials_lock.exists());
    let applied = fs::read_to_string(path.join("cordis.patch.yml")).unwrap();
    assert!(applied.starts_with(original));
    assert!(!applied.contains("synthetic-managed"));
    let value = dsh_patch::parse(&applied).unwrap();
    assert!(value.get("llm-tmax").is_none(), "legacy row retired");
    assert_eq!(
        value["tmax"]["config"]["home"],
        path.to_string_lossy().as_ref()
    );
    assert_eq!(
        value["tmax"]["config"]["tmaxPath"],
        std::env::current_exe().unwrap().to_string_lossy().as_ref()
    );
    assert_eq!(value["credentials"]["disabled"], false);
    assert!(!legacy_plugin.exists(), "legacy file retired");
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

    // With nothing else touched, restore puts every byte back exactly.
    let index: Index = store.read_json(INDEX).unwrap();
    restore(&store, &index.records[0].id, false).await.unwrap();
    assert!(!credentials_lock.exists());
    assert_eq!(
        fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
        legacy
    );
    assert_eq!(fs::read(&legacy_plugin).unwrap(), legacy_plugin_bytes);
    assert_eq!(fs::read(&credentials).unwrap(), before_credentials);

    // Applying again reaches the same managed state.
    apply(&store, &config, "managed", false).await.unwrap();
    let reapplied = fs::read_to_string(path.join("cordis.patch.yml")).unwrap();
    assert_eq!(reapplied, applied);
    assert!(!legacy_plugin.exists());

    // A user field under the entry tmax does not own is not tmax's to remove:
    // restore takes only tmax's own fields back and leaves that field alone.
    let index: Index = store.read_json(INDEX).unwrap();
    let unowned = dsh_patch::set(
        &applied,
        &["tmax".into(), "config".into(), "refreshSeconds".into()],
        Some(&json!(30)),
    )
    .unwrap();
    fs::write(path.join("cordis.patch.yml"), &unowned).unwrap();
    let latest = index.records.last().unwrap();
    restore(&store, &latest.id, true).await.unwrap();
    restore(&store, &latest.id, false).await.unwrap();
    assert!(!credentials_lock.exists());
    let restored = fs::read_to_string(path.join("cordis.patch.yml")).unwrap();
    let restored_value = dsh_patch::parse(&restored).unwrap();
    assert_eq!(restored_value["tmax"]["config"]["refreshSeconds"], 30);
    assert!(restored_value["tmax"]["config"].get("home").is_none());
    assert!(restored_value["tmax"]["config"].get("tmaxPath").is_none());
    assert!(restored_value.get("credentials").is_none());
    assert_eq!(
        restored_value["llm-tmax"]["name"],
        format!("{}/tmax-dsh.mjs", path.display())
    );
    assert!(restored.contains(r#"{"insert":[{"config":{"providers":{"local":{"models":[{"id":"first"}]}}},"id":"llm-tmax""#));
    assert!(restored.starts_with(original));
    assert_eq!(fs::read(&legacy_plugin).unwrap(), legacy_plugin_bytes);
    assert_eq!(fs::read(&credentials).unwrap(), before_credentials);
    assert_eq!(
        fs::read_to_string(native_profile.join("cordis.patch.yml")).unwrap(),
        profile_patch
    );

    // The user's field survives the next apply too, and a later unrelated row
    // survives the next restore.
    apply(&store, &config, "managed", false).await.unwrap();
    let reapplied = fs::read_to_string(path.join("cordis.patch.yml")).unwrap();
    let reapplied_value = dsh_patch::parse(&reapplied).unwrap();
    assert!(reapplied.starts_with(original));
    assert_eq!(
        reapplied_value["tmax"]["config"]["home"],
        path.to_string_lossy().as_ref()
    );
    assert_eq!(
        reapplied_value["tmax"]["config"]["tmaxPath"],
        std::env::current_exe().unwrap().to_string_lossy().as_ref()
    );
    assert_eq!(reapplied_value["tmax"]["config"]["refreshSeconds"], 30);
    assert_eq!(reapplied_value["credentials"]["disabled"], false);
    assert!(reapplied_value.get("llm-tmax").is_none());
    assert!(!legacy_plugin.exists());
    fs::write(
        path.join("cordis.patch.yml"),
        format!("{reapplied}- id: later\n  disabled: true\n"),
    )
    .unwrap();
    let index: Index = store.read_json(INDEX).unwrap();
    let latest = index.records.last().unwrap();
    restore(&store, &latest.id, true).await.unwrap();
    restore(&store, &latest.id, false).await.unwrap();
    assert!(!credentials_lock.exists());
    let final_text = fs::read_to_string(path.join("cordis.patch.yml")).unwrap();
    let final_value = dsh_patch::parse(&final_text).unwrap();
    // A restored insert row is appended after rows the user added later.
    assert!(final_text.contains("- id: later\n  disabled: true\n"));
    assert_eq!(final_value["tmax"]["config"]["refreshSeconds"], 30);
    assert!(final_value["tmax"]["config"].get("home").is_none());
    assert!(final_value.get("credentials").is_none());
    assert_eq!(
        final_value["agent-default-model"]["config"]["model"],
        "unchanged"
    );
    assert_eq!(
        final_value["llm-tmax"]["config"]["providers"]["local"]["models"][0]["id"],
        "first"
    );
    assert_eq!(fs::read(&legacy_plugin).unwrap(), legacy_plugin_bytes);
    assert_eq!(fs::read(&credentials).unwrap(), before_credentials);

    // A home whose bundle is gone is blocked before any native write.
    fs::remove_dir_all(bundle_profile).unwrap();
    let untouched = final_text;
    for dry_run in [true, false] {
        let blocked = apply(&store, &config, "managed", dry_run).await.unwrap();
        assert_eq!(blocked["_code"], "dsh_plugin_not_installed", "{dry_run}");
        assert_eq!(blocked["plugin"]["state"], "absent");
        assert_eq!(blocked["effects"]["native_writes"], false);
    }
    let inspected = inspect(&store, &config, "managed").await.unwrap();
    assert_eq!(inspected["plugin"]["state"], "absent");
    assert!(
        inspected["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|b| b["code"] == "dsh_plugin_not_installed")
    );
    assert_eq!(
        fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
        untouched
    );
}

#[cfg(unix)]
#[tokio::test]
async fn apply_reports_every_environment_obstacle_in_one_run() {
    const CHILD: &str = "TMAX_DSH_ENVIRONMENT_CHILD";
    const THIS: &str = "agent_integration::dsh_lifecycle_tests::apply_reports_every_environment_obstacle_in_one_run";
    if std::env::var_os(CHILD).is_none() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("dsh");
        // An older client, below the version floor tmax generates for, and a
        // home whose profiles carry no plugin.
        fs::write(
            &executable,
            "#!/bin/sh\n[ \"$1\" = --version ] || exit 1\nprintf '0.1.0\\n'\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", THIS, "--nocapture"])
            .env(CHILD, "1")
            .env("PATH", directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated environment run failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path().join("authority"));
    let home = directory.path().join("dsh");
    fs::create_dir_all(&home).unwrap();
    let mut target = inspection_target(home.clone());
    target.agent = "dsh".into();
    let mut config = Config::empty();
    config.providers.insert(
        "p".into(),
        config::Provider {
            integration: Some("custom".into()),
            auth: Some(Auth::ApiKey {
                key_ref: "secret:test-upstream".into(),
            }),
            endpoints: BTreeMap::from([
                ("responses".into(), "https://example.invalid/v1".into()),
                (
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                ),
            ]),
            models: BTreeMap::from([("first".into(), config::Model::default())]),
            ..Default::default()
        },
    );
    config.agents.insert("dsh-main".into(), target);
    store
        .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
        .unwrap();
    // Both obstacles are reported together, so fixing one does not simply reveal
    // the next on the following run.
    let answer = apply(&store, &config, "dsh-main", true).await.unwrap();
    let codes: Vec<&str> = answer["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|blocker| blocker["code"].as_str())
        .collect();
    assert_eq!(
        codes,
        vec!["native_version_unqualified", "dsh_plugin_not_installed"],
        "{answer}"
    );
    assert_eq!(answer["_code"], "native_version_unqualified");
    assert_eq!(answer["blockers"][0]["detected"], "0.1.0");
    assert_eq!(answer["environment"]["native"]["state"], "detected");
    assert_eq!(answer["environment"]["plugin"]["state"], "absent");
    assert_eq!(answer["effects"]["native_writes"], false);
    // Inspection names each obstacle with the dimension it belongs to, so a
    // reader can act on the one they can act on now.
    let inspected = inspect(&store, &config, "dsh-main").await.unwrap();
    let readiness = &inspected["readiness"];
    assert_eq!(readiness["ready"], false);
    let blocked: Vec<&str> = readiness["blocked"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["dimension"].as_str())
        .collect();
    assert_eq!(
        blocked,
        vec!["native_installation", "plugin"],
        "{inspected}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_second_apply_never_resurrects_the_retired_artifact() {
    const CHILD: &str = "TMAX_DSH_REAPPLY_CHILD";
    const THIS: &str = "agent_integration::dsh_lifecycle_tests::a_second_apply_never_resurrects_the_retired_artifact";
    if std::env::var_os(CHILD).is_none() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("dsh");
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\n[ \"$1\" = --version ] || exit 1\nprintf '{}\\n'\n",
                super::adapters::DSH_QUALIFIED
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", THIS, "--nocapture"])
            .env(CHILD, "1")
            .env("PATH", directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated reapply run failed: {}{}",
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
    let bundle = home.join("profiles/desktop");
    fs::create_dir_all(bundle.join("node_modules/tmax")).unwrap();
    fs::write(
        bundle.join("package.json"),
        r#"{"name":"desktop","dsh":{"profile":{"bundles":["tmax"]}}}"#,
    )
    .unwrap();
    fs::write(
        bundle.join("node_modules/tmax/package.json"),
        r#"{"name":"tmax","version":"0.1.0"}"#,
    )
    .unwrap();
    let artifact = home.join("tmax-dsh.mjs");
    let artifact_bytes = b"export const name = 'llm-tmax'\n";
    fs::write(&artifact, artifact_bytes).unwrap();
    let legacy = format!(
        "- insert:\n    - id: llm-tmax\n      name: {}/tmax-dsh.mjs\n      config:\n        providers: {{}}\n",
        path.display()
    );
    fs::write(path.join("cordis.patch.yml"), &legacy).unwrap();
    let reference = secrets::put(&store, "provider:p", b"synthetic-reapply").unwrap();
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
    apply(&store, &config, "managed", false).await.unwrap();
    assert!(!artifact.exists(), "the first apply retires the artifact");
    let applied = fs::read_to_string(path.join("cordis.patch.yml")).unwrap();
    // The retired file is recorded as absent, so replaying its record must not
    // restore it: an apply that recreates the adapter would reintroduce exactly
    // the duplicate route registration the retirement removes.
    let second = apply(&store, &config, "managed", false).await.unwrap();
    assert!(
        !artifact.exists(),
        "a second apply resurrected the retired artifact"
    );
    assert_eq!(
        second["changed_files"].as_array().map(Vec::len),
        Some(0),
        "{second}"
    );
    assert_eq!(
        fs::read_to_string(path.join("cordis.patch.yml")).unwrap(),
        applied
    );
}
