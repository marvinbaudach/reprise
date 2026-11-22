//! Privacy-safe, platform-neutral diagnostic report model and renderer.

mod crash_reduce;
mod field_policy;
#[cfg(test)]
mod field_sweep;
#[cfg(test)]
mod field_sweep_cfg;
#[cfg(test)]
mod field_sweep_lex;
#[cfg(test)]
mod field_sweep_literals;
#[cfg(test)]
mod field_sweep_spans;
mod field_values;
mod log_tail;
mod log_template;
mod log_templates;
mod model;
mod panic_phrases;
mod redact_composite;
mod render;
mod safe_event;

pub use crash_reduce::{reduce_backtrace, reduce_location, reduce_message, PanicPayload};
pub use field_policy::is_safe_structured_field;
pub use field_values::{render_field, RecordedField, RecordedValue, REDACTED};
pub use log_tail::{read_log_tail, tail_of, TAIL_MAX_BYTES, TAIL_MAX_LINES};
pub use log_template::{LogTemplate, TemplateError, HOLE};
pub use log_templates::{is_own_file, reduced_message, templates_for_file};
pub use model::{
    DiagnosticEvent, DiagnosticFacts, DiagnosticLevel, DiagnosticLog, PackageKind,
    DIAGNOSTIC_EVENT_CAPACITY,
};
pub use render::render_report;
pub use safe_event::{safe_event, Callsite, LogLevel, RecordedEvent, SafeEvent};

#[cfg(test)]
mod allowlist_acceptance_tests;
#[cfg(test)]
mod allowlist_fuzz_tests;
#[cfg(test)]
mod crash_reduce_tests;
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
mod field_sweep_literals_tests;
#[cfg(test)]
mod field_sweep_spans_tests;
#[cfg(test)]
mod field_sweep_tests;
#[cfg(test)]
mod field_values_tests;
#[cfg(test)]
mod log_tail_tests;
#[cfg(test)]
mod log_template_tests;
#[cfg(test)]
mod log_templates_tests;
#[cfg(test)]
mod panic_phrases_tests;
#[cfg(test)]
mod safe_event_tests;
#[cfg(test)]
mod tests;
