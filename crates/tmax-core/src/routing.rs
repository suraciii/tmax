mod calendar;
mod priority;
mod selection;

#[cfg(test)]
mod test_support;

pub use priority::validate_model;
pub(crate) use selection::{Advice, explain_loaded, explain_native};
pub use selection::{
    RequestRequirements, Selection, explain, select, select_request, select_request_loaded,
};

use crate::{
    Error, Result,
    config::{self, Config},
    inputs::{self, Snapshot},
    storage::Store,
};

pub fn reconcile(
    store: &Store,
    old: &Config,
    new: &Config,
    renames: &[(String, String)],
) -> Result<()> {
    inputs::reconcile(store, old, new, renames)
}

pub fn destination_identity(
    store: &Store,
    config: &Config,
    qualified: &str,
) -> Result<(String, String, String)> {
    let (provider, model) = config::qualified(qualified)?;
    config::model(config, qualified)?;
    Snapshot::capture(store, config)
        .destination_identity(provider, model)
        .ok_or_else(|| {
            Error::new(
                "identity_unavailable",
                "The destination's persistent identity or authorization lineage is unavailable",
            )
        })
}

pub fn destination_identity_loaded(
    store: &Store,
    config: &Config,
    qualified: &str,
) -> Result<(String, String, String)> {
    let (provider, model) = config::qualified(qualified)?;
    config::model(config, qualified)?;
    Snapshot::capture_loaded(store,config,None).destination_identity(provider,model)
        .ok_or_else(||Error::new("identity_unavailable","The loaded destination's persistent identity or authorization lineage is unavailable"))
}
