//! Agent construction for every metadata provider. A provider describes what it needs as an
//! [`AgentPolicy`]; this module is the only place that turns one into a ureq configuration.

use std::time::Duration;

use ureq::unversioned::resolver::Resolver;
use ureq::unversioned::transport::DefaultConnector;

/// Where users and operators can reach the maintainer; part of every identified request.
pub(crate) const CONTACT_URL: &str = "https://github.com/marvinbaudach";

/// The user agent every in-boundary provider sends.
#[must_use]
pub(crate) fn user_agent() -> String {
    format!("Reprise/{} ( {CONTACT_URL} )", env!("CARGO_PKG_VERSION"))
}

/// Everything a provider decides about its agent. Fields map one to one to ureq options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AgentPolicy {
    /// The whole-request timeout.
    pub timeout: Duration,
    /// Whether a 4xx or 5xx status surfaces as a transport error instead of a response.
    pub status_as_error: bool,
    /// Refuse plain-HTTP URLs.
    pub https_only: bool,
    /// The redirect limit; `None` keeps ureq's default.
    pub max_redirects: Option<u32>,
    /// Whether the proxy comes from the environment; `false` forces a direct connection.
    pub proxy_from_env: bool,
}

impl AgentPolicy {
    /// Status errors off, everything else default: the callers read the status themselves.
    pub(crate) const fn source(timeout: Duration) -> Self {
        Self {
            timeout,
            status_as_error: false,
            https_only: false,
            max_redirects: None,
            proxy_from_env: true,
        }
    }

    /// Status errors on, everything else default.
    pub(crate) const fn strict(timeout: Duration) -> Self {
        Self {
            status_as_error: true,
            ..Self::source(timeout)
        }
    }
}

/// The one place a provider agent configuration is assembled.
pub(crate) fn build_config(policy: AgentPolicy) -> ureq::config::Config {
    let mut builder = ureq::Agent::config_builder()
        .timeout_global(Some(policy.timeout))
        .user_agent(user_agent())
        .http_status_as_error(policy.status_as_error);
    if policy.https_only {
        builder = builder.https_only(true);
    }
    if let Some(limit) = policy.max_redirects {
        builder = builder.max_redirects(limit);
    }
    if !policy.proxy_from_env {
        builder = builder.proxy(None);
    }
    builder.build()
}

pub(crate) fn build_agent(policy: AgentPolicy) -> ureq::Agent {
    build_config(policy).new_agent()
}

/// Like [`build_agent`], with the caller's own address resolver.
pub(crate) fn build_agent_with_resolver(
    policy: AgentPolicy,
    resolver: impl Resolver,
) -> ureq::Agent {
    ureq::Agent::with_parts(build_config(policy), DefaultConnector::default(), resolver)
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
