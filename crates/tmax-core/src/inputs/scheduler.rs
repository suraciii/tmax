use crate::{Result, config::Config, storage::Store};
use chrono::{Duration, Utc};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{LazyLock, Mutex},
    time::Instant,
};

use super::{
    acquisition::acquire,
    exact::timestamp,
    lineage::{key, selected_binding},
    model::Binding,
    store::read_runtime,
};

pub(super) fn scheduled_next_due(store: &Store, b: &Binding) -> Option<String> {
    let schedules = SCHEDULES.lock().unwrap_or_else(|p| p.into_inner());
    let next = schedules.get(&store.root)?.due.get(&key(b))?.next?;
    let remaining = Duration::from_std(next.saturating_duration_since(Instant::now())).ok()?;
    Utc::now().checked_add_signed(remaining).map(timestamp)
}

struct Due {
    interval: i64,
    next: Option<Instant>,
}
#[derive(Default)]
struct ScheduleState {
    due: BTreeMap<String, Due>,
    cursor: String,
}
static SCHEDULES: LazyLock<Mutex<BTreeMap<std::path::PathBuf, ScheduleState>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
pub async fn automatic_tick(store: &Store, config: &Config) -> Result<()> {
    let runtime = read_runtime(store)?;
    let mut work = Vec::new();
    let mut retained = BTreeSet::new();
    let now = Instant::now();
    {
        let mut schedules = SCHEDULES.lock().unwrap_or_else(|p| p.into_inner());
        let state = schedules.entry(store.root.clone()).or_default();
        for (p, provider) in &config.providers {
            for (name, d) in &provider.inputs {
                if let Some(interval) = d.refresh_interval_seconds {
                    if d.source == "manual" {
                        continue;
                    }
                    let b = selected_binding(&runtime, config, p, name, true)?;
                    let key = key(&b);
                    retained.insert(key.clone());
                    let entry = state.due.entry(key.clone()).or_insert(Due {
                        interval,
                        next: Some(now),
                    });
                    if entry.interval != interval {
                        entry.interval = interval;
                        entry.next = Some(now);
                    }
                    if entry.next.is_some_and(|next| now >= next) {
                        entry.next =
                            now.checked_add(std::time::Duration::from_secs(interval as u64));
                        work.push((key, p.clone(), name.clone()));
                    }
                }
            }
        }
        state.due.retain(|key, _| retained.contains(key));
        // Excess due ticks are skipped, not queued. Rotate to avoid starving a
        // channel when more declarations are due than this bounded batch admits.
        work.sort_by(|a, b| {
            (a.0 <= state.cursor)
                .cmp(&(b.0 <= state.cursor))
                .then_with(|| a.0.cmp(&b.0))
        });
        work.truncate(4);
        if let Some(last) = work.last() {
            state.cursor = last.0.clone();
        }
    }
    use futures_util::{StreamExt, stream};
    let mut pending = stream::iter(work.into_iter().map(|(_, p, name)| async move {
        acquire(store, config, &p, &name, true, "automatic", true).await
    }))
    .buffer_unordered(4);
    let mut failure = None;
    while let Some(result) = pending.next().await {
        if let Err(error) = result
            && error.code != "acquisition_in_progress"
            && failure.is_none()
        {
            failure = Some(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

pub(super) fn clear_store_schedule(store: &Store) {
    SCHEDULES
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&store.root);
}

#[cfg(test)]
mod tests {
    use super::super::{
        acquisition::acquisition_lock,
        lifecycle::{activate, deactivate, reconcile},
        test_support::{fixture, save_intent},
    };
    use super::*;
    use crate::config;

    fn scheduled_fixture(count: usize) -> (tempfile::TempDir, Store, Config) {
        let (directory, store, original) = fixture();
        let mut config = original.clone();
        let provider = config.providers.get_mut("p").unwrap();
        let mut declaration = provider.inputs.remove("i").unwrap();
        declaration.source = "provider".into();
        declaration.refresh_interval_seconds = Some(3600);
        for index in 0..count {
            provider
                .inputs
                .insert(format!("i{index}"), declaration.clone());
        }
        save_intent(&store, &config);
        reconcile(&store, &original, &config, &[]).unwrap();
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        (directory, store, config)
    }

    fn attempt_revisions(store: &Store) -> BTreeMap<String, String> {
        read_runtime(store)
            .unwrap()
            .channels
            .into_iter()
            .filter_map(|(key, channel)| {
                channel.last_attempt.map(|attempt| (key, attempt.revision))
            })
            .collect()
    }

    #[tokio::test]
    async fn bounded_due_batches_rotate_and_excess_ticks_do_not_catch_up() {
        let (_directory, store, config) = scheduled_fixture(6);
        automatic_tick(&store, &config).await.unwrap();
        let first = attempt_revisions(&store);
        assert_eq!(first.len(), 4);

        automatic_tick(&store, &config).await.unwrap();
        assert_eq!(attempt_revisions(&store), first);

        {
            let mut schedules = SCHEDULES.lock().unwrap_or_else(|p| p.into_inner());
            for due in schedules.get_mut(&store.root).unwrap().due.values_mut() {
                due.next = Some(Instant::now());
            }
        }
        automatic_tick(&store, &config).await.unwrap();
        let second = attempt_revisions(&store);
        assert_eq!(second.len(), 6);
        assert_eq!(
            first
                .iter()
                .filter(|(key, revision)| second.get(*key) != Some(*revision))
                .count(),
            2
        );
        let runtime = read_runtime(&store).unwrap();
        assert!(runtime.channels.values().all(|channel| {
            let attempt = channel.last_attempt.as_ref().unwrap();
            attempt.trigger == "automatic"
                && attempt.outcome == "unavailable"
                && attempt.reason_code.as_deref() == Some("unsupported_source")
        }));
        deactivate(&store).unwrap();
    }

    #[tokio::test]
    async fn busy_ticks_skip_until_interval_change_and_activation_starts_fresh() {
        let (_directory, store, config) = scheduled_fixture(1);
        let runtime = read_runtime(&store).unwrap();
        let binding = selected_binding(&runtime, &config, "p", "i0", true).unwrap();
        let exclusive = acquisition_lock(&store, &binding).unwrap();
        automatic_tick(&store, &config).await.unwrap();
        assert!(attempt_revisions(&store).is_empty());
        drop(exclusive);
        automatic_tick(&store, &config).await.unwrap();
        assert!(attempt_revisions(&store).is_empty());

        let mut changed = config.clone();
        changed
            .providers
            .get_mut("p")
            .unwrap()
            .inputs
            .get_mut("i0")
            .unwrap()
            .refresh_interval_seconds = Some(7200);
        save_intent(&store, &changed);
        reconcile(&store, &config, &changed, &[]).unwrap();
        activate(&store, &changed, &config::raw_revision(&store).unwrap()).unwrap();
        automatic_tick(&store, &changed).await.unwrap();
        assert_eq!(attempt_revisions(&store).len(), 1);

        acquire(&store, &changed, "p", "i0", false, "explicit", true)
            .await
            .unwrap();
        let explicit = attempt_revisions(&store);
        automatic_tick(&store, &changed).await.unwrap();
        assert_eq!(attempt_revisions(&store), explicit);

        deactivate(&store).unwrap();
        activate(&store, &changed, &config::raw_revision(&store).unwrap()).unwrap();
        automatic_tick(&store, &changed).await.unwrap();
        assert_ne!(attempt_revisions(&store), explicit);
        deactivate(&store).unwrap();
    }
}
