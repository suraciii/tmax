//! Agent integration for local coding tools.
//!
//! Desired native projections, guarded application, protected recovery, and
//! native authorization have separate owners behind this stable public facade.
mod adapters;
mod application;
mod authentication;
pub(crate) mod browser_login;
pub(crate) mod compatibility;
mod discovery;
mod document;
mod dsh_connect;
mod dsh_patch;
mod dsh_projection;
mod installation;
mod planning;
mod recovery;
mod scope;

#[cfg(all(test, unix))]
mod dsh_lifecycle_tests;
#[cfg(test)]
mod test_support;

pub use adapters::supported_roles;
pub(crate) use adapters::{alias_model_metadata, model_metadata};
pub use application::{apply, apply_request};
pub use authentication::{login, login_source_agents, logout};
pub use discovery::discover;
pub use dsh_connect::connect as dsh_connect;
pub use dsh_projection::projection as dsh_projection;
pub use planning::inspect;
pub(crate) use planning::reasoning_projection;
pub use recovery::{recovery, restore, restore_request, secret_references};

use crate::{Result, secrets, storage::Store};

pub fn rename_target(store: &Store, old: &str, new: &str) -> Result<()> {
    let _lock = store.lock("native")?;
    let stable = recovery::owner(store, old)?;
    let mut index = recovery::Index::load(store)?;
    index.rename_target(stable.as_deref(), new);
    let mut logins = authentication::Logins::load(store)?;
    logins.rename_target(old, new);
    index.save(store)?;
    logins.save(store)?;
    secrets::rename_owner(store, "agent", old, new)
}

pub fn remove_target(store: &Store, id: &str) -> Result<()> {
    let _lock = store.lock("native")?;
    let mut logins = authentication::Logins::load(store)?;
    logins.remove_target(id);
    logins.save(store)?;
    secrets::detach_owner(store, "agent", id)
}
