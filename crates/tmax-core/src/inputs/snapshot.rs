use crate::{
    Error, Result,
    config::{self, Config},
    storage::Store,
};
use chrono::Utc;
use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::{
    exact::ExactInstant,
    lineage::{binding, key, project_view, provider_scope_binding_in_view, signature},
    model::{Binding, Runtime, View, revision},
    store::read_runtime,
    value::{availability_checked, same_models},
};

#[derive(Clone)]
pub(crate) struct Snapshot<'a> {
    runtime: Runtime,
    view: Option<View>,
    observations: Option<crate::observations::ObservationSnapshot>,
    error: Option<&'static str>,
    metadata: Option<std::borrow::Cow<'a, crate::catalog::Snapshot>>,
}
impl<'a> Snapshot<'a> {
    pub(crate) fn capture(store: &Store, config: &Config) -> Self {
        Self::capture_view(store, config, false, None)
    }
    pub(crate) fn capture_loaded(
        store: &Store,
        config: &Config,
        metadata: Option<&'a crate::catalog::Snapshot>,
    ) -> Self {
        Self::capture_view(store, config, true, metadata)
    }
    fn capture_view(
        store: &Store,
        config: &Config,
        loaded: bool,
        supplied_metadata: Option<&'a crate::catalog::Snapshot>,
    ) -> Self {
        // Readers never initialize storage. If a first writer creates the lock
        // while we read restored files, recapture once under that existing lock.
        let captured = (|| -> Result<Self> {
            let capture = || -> Result<Self> {
                let saved_revision = if loaded {
                    None
                } else {
                    Some(config::raw_revision(store)?)
                };
                let saved_file = if loaded {
                    None
                } else {
                    config::load(store).ok()
                };
                let runtime = read_runtime(store)?;
                let metadata = if loaded {
                    supplied_metadata.map(std::borrow::Cow::Borrowed)
                } else {
                    crate::catalog::read(store, "accepted")
                        .ok()
                        .flatten()
                        .map(std::borrow::Cow::Owned)
                };
                if let Some(revision) = &saved_revision {
                    config::guard(store, revision)?;
                }
                let hash = signature(config);
                let view = if loaded {
                    runtime
                        .loaded
                        .as_ref()
                        .filter(|view| signature(&view.config) == hash)
                        .cloned()
                } else {
                    let mut view = project_view(&runtime.saved, config);
                    view.revision = saved_file
                        .filter(|file| {
                            Some(&file.revision) == saved_revision.as_ref()
                                && signature(&file.config) == hash
                        })
                        .map(|file| file.revision);
                    Some(view)
                };
                Ok(Self {
                    runtime,
                    view,
                    observations: if loaded {
                        crate::observations::snapshot(store).ok()
                    } else {
                        crate::observations::snapshot_saved(store).ok()
                    },
                    error: None,
                    metadata,
                })
            };
            let mut guard = store.read_lock("routing")?;
            let mut snapshot = capture()?;
            if guard.is_none() {
                guard = store.read_lock("routing")?;
                if guard.is_some() {
                    snapshot = capture()?;
                }
            }
            Ok(snapshot)
        })();
        captured.unwrap_or_else(|error| Self {
            runtime: Runtime::default(),
            view: None,
            observations: None,
            error: Some(match error.code.as_str() {
                "invalid_record" => "invalid_record",
                "inputs_changed" => "inputs_changed",
                _ => "storage_unavailable",
            }),
            metadata: None,
        })
    }
    pub(crate) fn configuration_revision(&self) -> Option<&str> {
        self.view.as_ref()?.revision.as_deref()
    }
    pub(crate) fn metadata_revision(&self) -> Option<&str> {
        self.metadata
            .as_ref()
            .map(|snapshot| snapshot.revision.as_str())
    }
    pub(crate) fn observations(&self) -> Option<&crate::observations::ObservationSnapshot> {
        self.observations.as_ref()
    }
    pub(crate) fn provider_scope_binding(&self, provider: &str) -> Option<(String, String)> {
        provider_scope_binding_in_view(&self.runtime, self.view.as_ref()?, provider)
    }
    pub(crate) fn input_revision(&self, provider: &str, input: &str) -> Value {
        let b = self
            .view
            .as_ref()
            .and_then(|view| binding(view, &self.runtime, provider, input));
        let current_revision = b
            .as_ref()
            .filter(|b| !b.declaration_revision.is_empty())
            .map(|b| revision(self.runtime.channels.get(&key(b))));
        json!({"provider":provider,"input":input,
            "provider_identity":b.as_ref().map(|b|b.provider_identity.as_str()).filter(|s|!s.is_empty()),
            "authorization_generation":b.as_ref().map(|b|b.authorization_generation.as_str()).filter(|s|!s.is_empty()),
            "declaration_revision":b.as_ref().map(|b|b.declaration_revision.as_str()).filter(|s|!s.is_empty()),"current_revision":current_revision})
    }
    pub(crate) fn destination_identity(
        &self,
        provider: &str,
        model: &str,
    ) -> Option<(String, String, String)> {
        let (identity, authorization) =
            provider_scope_binding_in_view(&self.runtime, self.view.as_ref()?, provider)?;
        let model_identity = self
            .view
            .as_ref()?
            .providers
            .get(provider)?
            .models
            .get(model)?;
        Some((identity, authorization, model_identity.clone()))
    }
    pub(crate) fn input(&self, provider: &str, input: &str, at: &ExactInstant) -> Value {
        let Some(view) = &self.view else {
            return json!({"provider":provider,"input":input,"current_revision":null,
            "availability":{"state":"unavailable","reason_code":self.error.unwrap_or("missing")}});
        };
        let Some(b) = binding(view, &self.runtime, provider, input) else {
            return json!({"provider":provider,"input":input,"current_revision":"absent","availability":{"state":"unavailable","reason_code":"missing"}});
        };
        let channel = self.runtime.channels.get(&key(&b));
        let head = channel.and_then(|c| c.head.as_ref());
        let availability = availability_checked(
            head,
            &b,
            at,
            head.is_none_or(|head| same_models(&head.binding.models, &b.models)),
            Some(&view.config.providers[provider].inputs[input]),
        );
        let mut basis = serde_json::to_value(&b).expect("binding serialization");
        basis["current_revision"] = if b.declaration_revision.is_empty() {
            Value::Null
        } else {
            json!(
                channel
                    .and_then(|c| c.head.as_ref())
                    .map(|h| h.revision.as_str())
                    .unwrap_or("absent")
            )
        };
        for field in [
            "provider_identity",
            "authorization_generation",
            "declaration_revision",
        ] {
            if basis[field] == "" {
                basis[field] = Value::Null;
            }
        }
        basis["availability"] = availability;
        let unsafe_basis = basis["availability"]["reason_code"]
            .as_str()
            .is_some_and(|reason| {
                [
                    "invalid_record",
                    "declaration_mismatch",
                    "authorization_mismatch",
                    "storage_unavailable",
                ]
                .contains(&reason)
            });
        if !unsafe_basis
            && let Some(value) = channel
                .and_then(|c| c.head.as_ref())
                .and_then(|h| h.value.as_ref())
        {
            let mut values = serde_json::to_value(value).expect("value serialization");
            values.as_object_mut().unwrap().remove("reference");
            values.as_object_mut().unwrap().remove("received_at");
            basis
                .as_object_mut()
                .unwrap()
                .extend(values.as_object().unwrap().clone());
        }
        basis
    }
}

pub(super) fn use_projection(
    runtime: &Runtime,
    selected: &Binding,
    loaded: bool,
    service_running: bool,
) -> Value {
    if loaded && !service_running {
        return json!({"state":"no_loaded_snapshot","reason_code":null});
    }
    let view = if loaded {
        runtime.loaded.as_ref()
    } else {
        Some(&runtime.saved)
    };
    let Some(view) = view else {
        return json!({"state":if loaded {"no_loaded_snapshot"} else {"unavailable"},"reason_code":null});
    };
    let other = view
        .providers
        .iter()
        .find(|(_, p)| p.identity == selected.provider_identity)
        .and_then(|(p, _)| binding(view, runtime, p, &selected.input));
    let matches = other.as_ref().is_some_and(|b| key(b) == key(selected));
    json!({"state":if matches {"applicable"} else if loaded {"pending"} else if other.is_none() {"no_saved_declaration"} else {"not_applicable"},"reason_code":if matches {None} else {Some("declaration_mismatch")}})
}

pub fn snapshot(store: &Store, config: &Config) -> Result<Value> {
    let snapshot = Snapshot::capture(store, config);
    let at = ExactInstant::from_chrono(Utc::now());
    let mut providers = BTreeMap::new();
    for (name, p) in &config.providers {
        let mut inputs = BTreeMap::new();
        for input in p.inputs.keys() {
            inputs.insert(input.clone(), snapshot.input(name, input, &at));
        }
        providers.insert(name.clone(), inputs);
    }
    Ok(
        json!({"at":at.canonical(),"configuration_revision":snapshot.configuration_revision(),"metadata_revision":snapshot.metadata_revision(),"providers":providers}),
    )
}

pub fn provider_identity(store: &Store, config: &Config, provider: &str) -> Result<Option<String>> {
    let snapshot = Snapshot::capture(store, config);
    if let Some(reason) = snapshot.error {
        return Err(Error::new(
            reason,
            "Provider incarnation state is unavailable",
        ));
    }
    Ok(snapshot
        .view
        .as_ref()
        .and_then(|view| view.providers.get(provider))
        .filter(|ids| !ids.identity.is_empty())
        .map(|ids| ids.identity.clone()))
}

#[cfg(test)]
mod tests {
    use super::super::{
        lifecycle::activate,
        store::STATE,
        test_support::{fixture, retain_value},
    };
    use super::*;
    #[test]
    fn restored_runtime_and_catalog_survive_missing_routing_lock_without_mutation() {
        let (_directory, store, config) = fixture();
        let head = retain_value(&store, &config);
        let revision = config::raw_revision(&store).unwrap();
        activate(&store, &config, &revision).unwrap();
        let metadata = crate::catalog::Snapshot {
            revision: "accepted-restored".into(),
            ..Default::default()
        };
        store
            .write_json("catalog/accepted.json", &metadata)
            .unwrap();
        std::fs::remove_file(store.root.join("locks/routing.lock")).unwrap();
        let bytes = std::fs::read(store.root.join(STATE)).unwrap();
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        let saved = Snapshot::capture(&store, &config);
        assert_eq!(saved.metadata_revision(), Some("accepted-restored"));
        assert_eq!(saved.configuration_revision(), Some(revision.as_str()));
        assert!(saved.destination_identity("p", "m").is_some());
        assert_eq!(saved.input("p", "i", &at)["current_revision"], head);
        assert_eq!(
            saved.input("p", "i", &at)["availability"]["state"],
            "available"
        );
        let loaded = Snapshot::capture_loaded(&store, &config, Some(&metadata));
        assert_eq!(
            loaded.destination_identity("p", "m"),
            saved.destination_identity("p", "m")
        );
        assert_eq!(loaded.metadata_revision(), Some("accepted-restored"));
        assert_eq!(std::fs::read(store.root.join(STATE)).unwrap(), bytes);
        assert!(!store.root.join("locks/routing.lock").exists());
    }

    #[test]
    fn comment_only_saved_edit_reports_current_bytes_without_rewriting_runtime() {
        let (_directory, store, config) = fixture();
        let before = config::raw_revision(&store).unwrap();
        activate(&store, &config, &before).unwrap();
        let bytes = std::fs::read(store.root.join(STATE)).unwrap();
        let edited = format!(
            "# Pending manual comment\n{}",
            toml::to_string(&config).unwrap()
        );
        store.write_raw("tmax.toml", edited.as_bytes()).unwrap();
        let after = config::raw_revision(&store).unwrap();
        assert_ne!(before, after);
        assert_eq!(
            Snapshot::capture(&store, &config).configuration_revision(),
            Some(after.as_str())
        );
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).configuration_revision(),
            Some(before.as_str())
        );
        assert_eq!(std::fs::read(store.root.join(STATE)).unwrap(), bytes);
        let mut hypothetical = config.clone();
        hypothetical
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .priority = 5;
        assert!(
            Snapshot::capture(&store, &hypothetical)
                .configuration_revision()
                .is_none()
        );
    }

    #[test]
    fn loaded_capture_ignores_invalid_pending_file_and_saved_revision_is_unqualified() {
        let (_directory, store, config) = fixture();
        retain_value(&store, &config);
        let revision = config::raw_revision(&store).unwrap();
        activate(&store, &config, &revision).unwrap();
        let destination =
            Snapshot::capture_loaded(&store, &config, None).destination_identity("p", "m");
        store
            .write_raw("tmax.toml", b"this is not valid TOML [")
            .unwrap();
        let loaded = Snapshot::capture_loaded(&store, &config, None);
        assert_eq!(loaded.configuration_revision(), Some(revision.as_str()));
        assert_eq!(loaded.destination_identity("p", "m"), destination);
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(
            loaded.input("p", "i", &at)["availability"]["state"],
            "available"
        );
        assert!(
            Snapshot::capture(&store, &config)
                .configuration_revision()
                .is_none()
        );
    }
}
