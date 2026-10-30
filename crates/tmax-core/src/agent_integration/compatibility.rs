//! Effect-free native compatibility shared by selection advice and application.
pub(crate) use super::authentication::AuthorizationSnapshot;
use super::{adapters, authentication::native_login_provider};
use crate::{
    Error, Result,
    config::{self, Auth, Config, Provider},
};

#[derive(Clone, Copy)]
pub(crate) struct Context<'a> {
    pub agent: &'a str,
    pub target: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub gateway: bool,
}

impl Context<'_> {
    pub(crate) fn protocol(&self, protocol: &str) -> Result<()> {
        if !adapters::AGENTS.contains(&self.agent) {
            return Err(Error::new(
                "unsupported_agent",
                "This native agent has no qualified adapter",
            ));
        }
        if self.agent == "codex" && protocol != "responses" {
            return Err(Error::new(
                "incompatible",
                "Codex custom providers require the Responses native protocol",
            ));
        }
        Ok(())
    }

    pub(crate) fn assess(
        &self,
        config: &Config,
        provider_id: &str,
        protocol: &str,
        public_id: &str,
    ) -> Result<()> {
        self.protocol(protocol)?;
        if self
            .provider
            .is_some_and(|required| required != provider_id)
        {
            return Err(Error::new(
                "incompatible",
                "The model belongs to a different upstream native provider entry",
            ));
        }
        let provider = &config.providers[provider_id];
        let integration = crate::provider_integration::integration_id(provider)?;
        if !crate::provider_integration::resolve(provider)?.contains_key(protocol) {
            return Err(Error::new(
                "incompatible",
                "The upstream integration has no endpoint for this native protocol",
            ));
        }
        if self.gateway {
            if config.model_aliases.iter().any(|alias| {
                alias.id == public_id
                    && alias.protocol == protocol
                    && alias.continuity.as_deref().unwrap_or("session") == "session"
            }) && !(matches!(self.agent, "pi" | "opencode") && protocol == "chat-completions")
            {
                return Err(Error::new(
                    "compatibility_unavailable",
                    "This native route lacks a qualified new/resumed session-origin boundary",
                ));
            }
            let native = crate::gateway::NativeRequirements {
                client: match self.agent {
                    "pi" => crate::gateway::NativeClient::Pi,
                    "hermes" => crate::gateway::NativeClient::Hermes,
                    "opencode" => crate::gateway::NativeClient::OpenCode,
                    _ => crate::gateway::NativeClient::Unknown,
                },
                // The adapter enables Pi's Chat Completions affinity; Hermes
                // supplies its separately documented x-opencode-session header.
                go_session: matches!(self.agent, "hermes" | "opencode")
                    || self.agent == "pi" && protocol == "chat-completions",
                // Every other client reaches Go through the gateway, which
                // attributes the request itself.
                unattributed: !(matches!(self.agent, "hermes" | "opencode")
                    || self.agent == "pi" && protocol == "chat-completions"),
            };
            if let Some(reason) = crate::gateway::native_mismatch(provider, &native) {
                return Err(Error::new(
                    reason,
                    "The selected native client has no qualified gateway handoff for this provider",
                ));
            }
        } else {
            if integration == "opencode-go"
                && !matches!(self.agent, "pi" | "hermes" | "opencode" | "omp")
            {
                return Err(Error::new(
                    "compatibility_unavailable",
                    "This exact native client lacks qualified per-conversation Go header coverage; pi 1.2.0, omp 18.12.6, Hermes 0.22.1, and OpenCode 2.0.28 have separately sourced direct paths",
                ));
            }
            if matches!(integration, "kimi-code-plan-cn" | "kimi-code-plan-global")
                && self.agent == "hermes"
                && protocol == "messages"
            {
                return Err(Error::new(
                    "incompatible",
                    "This Hermes Messages path substitutes a Claude Code identity for Kimi; the documented native Chat Completions path remains available",
                ));
            }
            self.login_provider(provider)?;
        }
        match &provider.auth {
            None => Err(Error::new(
                "credential_unavailable",
                "Configure the selected provider's API key or native login",
            )),
            Some(Auth::ApiKey { key_ref }) if key_ref.is_empty() => Err(Error::new(
                "credential_unavailable",
                "The API-key reference is absent",
            )),
            Some(Auth::Login { session_ref, .. }) if session_ref.is_none() => Err(Error::new(
                "credential_unavailable",
                "Sign in explicitly through the selected native owner",
            )),
            _ => Ok(()),
        }
    }

    pub(crate) fn assess_public(&self, config: &Config, id: &str, protocol: &str) -> Result<()> {
        self.protocol(protocol)?;
        if let Some(alias) = config
            .model_aliases
            .iter()
            .find(|a| a.id == id && a.protocol == protocol && a.enabled)
        {
            let mut failure = None;
            let mut supports = |provider: &str| match self.assess(config, provider, protocol, id) {
                Ok(()) => true,
                Err(error) => {
                    failure = Some(error);
                    false
                }
            };
            for candidate in &alias.candidates {
                let (provider, _) = config::qualified(candidate)?;
                if supports(provider) {
                    return Ok(());
                }
            }
            return Err(failure.unwrap_or_else(|| {
                Error::new(
                    "model_unavailable",
                    "No configured alias candidate supports this native handoff",
                )
            }));
        }
        let (provider, _) = config::qualified(id)?;
        config::model(config, id)?;
        self.assess(config, provider, protocol, id)
    }

    pub(crate) fn login_provider(&self, provider: &Provider) -> Result<Option<&'static str>> {
        let Some(Auth::Login { via, .. }) = &provider.auth else {
            return Ok(None);
        };
        if self.target != Some(via.as_str()) {
            return Err(Error::new(
                "native_login_consumer_unqualified",
                "A native-owned session cannot be copied to another target; select its declared owner or a qualified gateway",
            ));
        }
        native_login_provider(
            crate::provider_integration::integration_id(provider)?,
            self.agent,
        )
        .map(Some)
        .ok_or_else(|| {
            Error::new(
                "native_login_unqualified",
                "This provider does not have a qualified login flow for this native owner",
            )
        })
    }
}
