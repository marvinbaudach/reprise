//! Every structured field the code emits is classified, exactly once.

use std::collections::BTreeSet;
use std::path::Path;

use super::field_policy::{KEEP_FIELDS, REDACT_FIELDS};
use super::field_sweep::{distinct_keys, workspace_sites};

/// The sweep of origin/dev that motivated the policy found 397 keys. The
/// scanner finds more, because the sweep missed dotted keys and counted
/// format arguments, and the two together are not a tolerance: a drop below
/// this number means the scanner stopped seeing part of the code.
const SWEEP_FLOOR: usize = 397;

#[test]
fn every_emitted_field_key_is_in_exactly_one_list() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let sites = workspace_sites(&crates).expect("the workspace sources are readable");
    let emitted = distinct_keys(&sites);
    let kept: BTreeSet<&str> = KEEP_FIELDS.iter().copied().collect();
    let redacted: BTreeSet<&str> = REDACT_FIELDS.iter().copied().collect();

    let mut problems = Vec::new();
    if emitted.len() < SWEEP_FLOOR {
        problems.push(format!(
            "the scanner found {} keys, fewer than the {SWEEP_FLOOR} of the original sweep",
            emitted.len()
        ));
    }
    for key in &emitted {
        let site = sites
            .iter()
            .find(|site| site.key == *key)
            .map(|site| format!("{}:{}", site.file, site.line))
            .unwrap_or_default();
        match (kept.contains(key), redacted.contains(key)) {
            (false, false) => problems.push(format!(
                "`{key}` is emitted (first at {site}) but classified nowhere. Add it to \
                 KEEP_FIELDS in crates/reprise-core/src/diagnostics/field_policy.rs only if every \
                 place that emits it carries a number, a bool, a duration, a fieldless enum, a \
                 static label or an internal id; otherwise add it to REDACT_FIELDS."
            )),
            (true, true) => problems.push(format!(
                "`{key}` is in both KEEP_FIELDS and REDACT_FIELDS; keep it in exactly one."
            )),
            _ => {}
        }
    }
    for key in kept.iter().chain(&redacted) {
        if !emitted.contains(key) {
            problems.push(format!(
                "`{key}` is listed in field_policy.rs but no tracing macro emits it any more; \
                 remove it."
            ));
        }
    }

    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}
