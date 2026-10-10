//! The engine's one HTTP boundary: agent construction (`client`), request spacing (`rate`),
//! the host circuit breaker (`breaker`) and the fixture seam (`fixtures`).
//!
//! Each provider keeps its own error type, endpoints, headers and fixture variable; what it hands
//! to this module is a policy value (timeouts, status handling, redirects, proxy) and a spacing
//! key. Nothing here knows a provider's wire format.

pub(crate) mod breaker;
pub(crate) mod client;
pub(crate) mod fixtures;
pub(crate) mod rate;

pub(crate) use crate::sync::lock_unpoisoned;
