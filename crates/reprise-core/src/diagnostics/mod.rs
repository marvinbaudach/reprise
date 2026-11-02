//! Privacy-safe, platform-neutral diagnostic report model and renderer.

mod log_tail;
mod model;
mod redact;
mod render;

pub use log_tail::{read_log_tail, tail_of, TAIL_MAX_BYTES, TAIL_MAX_LINES};
pub use model::{
    DiagnosticEvent, DiagnosticFacts, DiagnosticLevel, DiagnosticLog, PackageKind,
    RedactionContext, DIAGNOSTIC_EVENT_CAPACITY,
};
pub use redact::{is_safe_structured_field, redact_log_message};
pub use render::render_report;

#[cfg(test)]
mod extended_tests;
#[cfg(test)]
mod log_tail_tests;
#[cfg(test)]
mod tests;
