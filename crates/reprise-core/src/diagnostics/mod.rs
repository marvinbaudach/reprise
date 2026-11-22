//! Privacy-safe, platform-neutral diagnostic report model and renderer.

mod crash_message;
mod crash_section;
mod field_policy;
#[cfg(test)]
mod field_sweep;
#[cfg(test)]
mod field_sweep_cfg;
#[cfg(test)]
mod field_sweep_lex;
#[cfg(test)]
mod field_sweep_spans;
mod log_tail;
mod model;
mod redact;
mod redact_assignments;
mod redact_composite;
mod redact_filenames;
mod redact_line_prefix;
mod redact_markers;
mod redact_names;
mod redact_network;
mod redact_network_scan;
mod redact_normalise;
mod redact_path_relative;
mod redact_path_triggers;
mod redact_paths;
mod redact_secrets;
mod redact_slash_words;
mod render;

pub(crate) use crash_message::redact_panic_message;
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
mod field_policy_sweep_tests;
#[cfg(test)]
mod field_policy_tests;
#[cfg(test)]
mod field_sweep_files_tests;
#[cfg(test)]
mod field_sweep_lex_tests;
#[cfg(test)]
mod field_sweep_spans_tests;
#[cfg(test)]
mod field_sweep_tests;
#[cfg(test)]
mod log_tail_tests;
#[cfg(test)]
mod redact_assignment_tests;
#[cfg(test)]
mod redact_composite_tests;
#[cfg(test)]
mod redact_crash_message_tests;
#[cfg(test)]
mod redact_extent_tests;
#[cfg(test)]
mod redact_fixpoint_tests;
#[cfg(test)]
mod redact_leak_tests;
#[cfg(test)]
mod redact_path_tests;
#[cfg(test)]
mod redact_review_tests;
#[cfg(test)]
mod redact_round4_path_tests;
#[cfg(test)]
mod redact_round4_secret_tests;
#[cfg(test)]
mod redact_secret_network_tests;
#[cfg(test)]
mod render_crash_tests;
#[cfg(test)]
mod tests;
