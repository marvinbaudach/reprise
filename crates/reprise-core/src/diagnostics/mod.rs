//! Privacy-safe, platform-neutral diagnostic report model and renderer.

mod model;
mod redact;
mod render;

pub use model::{
    DiagnosticEvent, DiagnosticFacts, DiagnosticLevel, DiagnosticLog, PackageKind,
    RedactionContext, DIAGNOSTIC_EVENT_CAPACITY,
};
pub use redact::redact_log_message;
pub use render::render_report;

#[cfg(test)]
mod tests;
