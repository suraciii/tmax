use crate::{
    config::{self, Config, InputDeclaration, Provider},
    storage::Store,
};

use super::{
    exact::instant,
    lifecycle::reconcile,
    lineage::{binding, key},
    model::{InputValue, revision},
    store::{read_runtime, write_runtime},
    value::publish,
};

pub(super) fn intent() -> Config {
    let mut config = Config::empty();
    let mut provider = Provider {
        enabled: true,
        integration: Some("custom".into()),
        ..Default::default()
    };
    provider.endpoints.insert(
        "chat-completions".into(),
        "http://127.0.0.1:19090/v1".into(),
    );
    provider.models.insert(
        "m".into(),
        config::Model {
            enabled: true,
            ..Default::default()
        },
    );
    provider.inputs.insert("i".into(), declaration());
    config.providers.insert("p".into(), provider);
    config
}
pub(super) fn save_intent(store: &Store, config: &Config) {
    store
        .write_raw("tmax.toml", toml::to_string(config).unwrap().as_bytes())
        .unwrap();
}
pub(super) fn fixture() -> (tempfile::TempDir, Store, Config) {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path().to_path_buf());
    let config = intent();
    save_intent(&store, &config);
    reconcile(&store, &Config::empty(), &config, &[]).unwrap();
    (directory, store, config)
}
pub(super) fn retain_value(store: &Store, config: &Config) -> String {
    let _lock = store.lock("routing").unwrap();
    let mut runtime = read_runtime(store).unwrap();
    let b = binding(&runtime.saved, &runtime, "p", "i").unwrap();
    let channel = runtime.channels.entry(key(&b)).or_default();
    publish(
        channel,
        &b,
        &config.providers["p"].inputs["i"],
        value(),
        instant("2026-10-23T10:00:02Z").unwrap(),
    )
    .unwrap();
    let revision = revision(Some(channel)).to_owned();
    write_runtime(store, &runtime).unwrap();
    revision
}

pub(super) fn declaration() -> InputDeclaration {
    InputDeclaration {
        kind: "quota".into(),
        source: "manual".into(),
        scope: "weekly".into(),
        models: vec!["m".into()],
        max_age_seconds: 3600,
        refresh_interval_seconds: None,
    }
}

pub(super) fn value() -> InputValue {
    InputValue {
        observed_at: "2026-10-23T10:00:00Z".into(),
        observed_at_basis: "manual".into(),
        received_at: "2026-10-23T10:00:01Z".into(),
        valid_until: "2026-10-23T11:00:00Z".into(),
        reset_at: "2026-10-23T12:00:00Z".into(),
        remaining_percent: None,
        remaining: Some("1".into()),
        total: Some("3".into()),
        unit: Some("requests".into()),
        reference: None,
    }
}
