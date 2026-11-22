//! Privacy-safe, platform-neutral diagnostic report model and renderer.

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
mod redact;
mod redact_assignments;
mod redact_composite;
mod render;

pub use field_values::{render_field, RecordedField, RecordedValue, REDACTED};
pub use log_tail::{read_log_tail, tail_of, TAIL_MAX_BYTES, TAIL_MAX_LINES};
pub use log_template::{LogTemplate, TemplateError, HOLE};
pub use log_templates::{is_own_file, reduced_message, templates_for_file};
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
mod redact_assignment_tests;
#[cfg(test)]
mod redact_composite_tests;
#[cfg(test)]
mod redact_path_tests;
#[cfg(test)]
mod tests;
