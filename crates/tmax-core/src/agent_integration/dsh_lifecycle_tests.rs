//! Cross-owner DSH application/restoration: the profile settings tmax writes,
//! and the home entry an earlier release left behind.
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
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// Run one lifecycle body in a child process whose PATH carries a fake `dsh`,
/// because the version floor is probed from the executable, not the home.
#[cfg(unix)]
fn isolated(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    const CHILD: &str = "TMAX_DSH_LIFECYCLE_CHILD";
    let this = format!("agent_integration::dsh_lifecycle_tests::{name}");
    if std::env::var_os(CHILD).is_none() {
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
            .args(["--exact", &this, "--nocapture"])
            .env(CHILD, "1")
            .env("PATH", directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated DSH lifecycle failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return false;
    }
    true
}

/// An DSH home with one desktop profile that carries a provider of its own.
#[cfg(unix)]
fn home(directory: &Path) -> (PathBuf, PathBuf, String) {
    let home = directory.join("dsh");
    let profile = home.join("profiles/desktop");
    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("package.json"), r#"{"name":"desktop"}"#).unwrap();
    let user = "- id: llm-pi-ai\n  config:\n    providers:\n      mine:\n        api: openai-completions\n        baseURL: https://mine.invalid/v1\n        models:\n          - id: own\n".to_owned();
    fs::write(profile.join("cordis.patch.yml"), &user).unwrap();
    (home, profile, user)
}

/// A home patch of the user's own, which apply never touches.
#[cfg(unix)]
fn home_patch(home: &Path) -> String {
    let text = "# native policy\n- id: user-owned\n  config:\n    keep: true\n".to_owned();
    fs::write(home.join("cordis.patch.yml"), &text).unwrap();
    text
}

#[cfg(unix)]
#[tokio::test]
async fn dsh_apply_writes_profile_providers_and_leaves_the_home_patch_alone() {
    if !isolated("dsh_apply_writes_profile_providers_and_leaves_the_home_patch_alone") {
        return;
    }
    {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("authority"));
        let (home, profile, user_settings) = home(directory.path());
        let untouched_home = home_patch(&home);
        let credentials = home.join(".credentials.yaml");
        storage::atomic_write(
            &credentials,
            b"version: 1\nrefs:\n  EXISTING: synthetic-existing\nrecords: {}\n",
        )
        .unwrap();
        let reference = secrets::put(&store, "provider:p", b"synthetic-managed").unwrap();
        let mut target = inspection_target(home.clone());
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

        // Inspection names the profile whose settings the providers go to.
        let inspected = inspect(&store, &config, "managed", None).await.unwrap();
        assert_eq!(inspected["dsh_profile"], "desktop", "{inspected}");
        assert!(
            inspected["blockers"].as_array().unwrap().is_empty(),
            "{inspected}"
        );

        // A dry run writes nothing and never shows the key.
        let before_credentials = fs::read(&credentials).unwrap();
        let preview = apply(&store, &config, "managed", true, None).await.unwrap();
        assert!(!preview.to_string().contains("synthetic-managed"));
        assert_eq!(
            fs::read_to_string(home.join("cordis.patch.yml")).unwrap(),
            untouched_home
        );
        assert_eq!(fs::read(&credentials).unwrap(), before_credentials);

        // A foreign credentials lock blocks before any write.
        let lock = home.join(".credentials.yaml.lock");
        storage::atomic_write(&lock, format!("{}\n", std::process::id()).as_bytes()).unwrap();
        assert_eq!(
            apply(&store, &config, "managed", false, None)
                .await
                .unwrap_err()
                .code,
            "native_credentials_busy"
        );
        assert_eq!(
            fs::read_to_string(home.join("cordis.patch.yml")).unwrap(),
            untouched_home
        );
        fs::remove_file(&lock).unwrap();

        // Apply writes the profile settings DSH's own adapter loads, hands
        // over the credential reference, and retires the embedded adapter.
        apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        let settings =
            dsh_patch::parse(&fs::read_to_string(profile.join("cordis.patch.yml")).unwrap())
                .unwrap();
        let providers = &settings["llm-pi-ai"]["config"]["providers"];
        assert_eq!(providers["mine"]["baseURL"], "https://mine.invalid/v1");
        let provider = providers
            .as_object()
            .unwrap()
            .iter()
            .find(|(name, _)| name.starts_with("tmax-"))
            .expect("the target's provider");
        assert_eq!(provider.1["api"], "openai-completions");
        assert_eq!(provider.1["models"][0]["id"], "first");
        let reference_name = provider.1["apiKeyEnv"].as_str().unwrap();
        assert!(!settings.to_string().contains("synthetic-managed"));

        assert_eq!(
            fs::read_to_string(home.join("cordis.patch.yml")).unwrap(),
            untouched_home,
            "the home patch is the user's and stays as it was"
        );
        let native_credentials =
            document::parse(&fs::read_to_string(&credentials).unwrap(), Format::Yaml).unwrap();
        assert_eq!(native_credentials["refs"]["EXISTING"], "synthetic-existing");
        assert_eq!(
            native_credentials["refs"][reference_name],
            "synthetic-managed"
        );

        // With nothing else touched, restore puts every byte back exactly.
        let index: Index = store.read_json(INDEX).unwrap();
        restore(&store, &index.records[0].id, false).await.unwrap();
        assert_eq!(
            fs::read_to_string(home.join("cordis.patch.yml")).unwrap(),
            untouched_home
        );
        assert_eq!(fs::read(credentials).unwrap(), before_credentials);
        assert_eq!(
            fs::read_to_string(profile.join("cordis.patch.yml")).unwrap(),
            user_settings
        );

        // Applying again reaches the same managed state.
        apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        let reapplied = fs::read_to_string(profile.join("cordis.patch.yml")).unwrap();
        let second = apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        assert_eq!(
            second["changed_files"].as_array().map(Vec::len),
            Some(0),
            "{second}"
        );
        assert_eq!(
            fs::read_to_string(profile.join("cordis.patch.yml")).unwrap(),
            reapplied
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn apply_reports_every_environment_obstacle_in_one_run() {
    if !isolated("apply_reports_every_environment_obstacle_in_one_run") {
        return;
    }
    {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("authority"));
        let home = directory.path().join("dsh");
        fs::create_dir_all(home.join("profiles/desktop")).unwrap();
        fs::write(
            home.join("profiles/desktop/package.json"),
            r#"{"name":"desktop"}"#,
        )
        .unwrap();
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
                endpoints: BTreeMap::from([(
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                )]),
                models: BTreeMap::from([("first".into(), config::Model::default())]),
                ..Default::default()
            },
        );
        config.agents.insert("dsh-main".into(), target);
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        // A qualified client and a home with a profile are all this target
        // needs: nothing about a plugin can block it any more.
        let answer = apply(&store, &config, "dsh-main", true, None)
            .await
            .unwrap();
        assert_eq!(answer["environment"]["native"]["state"], "detected");
        assert_eq!(answer["_code"], Value::Null, "{answer}");
        let inspected = inspect(&store, &config, "dsh-main", None).await.unwrap();
        assert_eq!(inspected["readiness"]["ready"], true, "{inspected}");
        assert!(
            inspected["readiness"]["blocked"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
