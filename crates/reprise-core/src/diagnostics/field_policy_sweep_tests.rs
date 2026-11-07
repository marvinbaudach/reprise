//! Every structured field the code emits is classified, exactly once.

use std::collections::BTreeSet;
use std::path::Path;

use super::field_policy::{KEEP_FIELDS, REDACT_FIELDS};
use super::field_sweep::{distinct_keys, workspace_sites, FieldSite};

/// The key `EventFields` routes into the message slot without asking the
/// policy: a field called `message` is indistinguishable from the text of the
/// event itself.
const MESSAGE_KEY: &str = "message";

/// The one crate whose events never reach `EventFields`: it logs to logcat
/// and never calls the redactor, so a field called `message` is harmless
/// there. Everywhere else it is refused.
const MESSAGE_EXEMPT_PREFIX: &str = "reprise-android-ffi/";

/// What is wrong with the classification of `emitted`, one sentence per
/// problem; empty when every key is in exactly one list and every listed key
/// is still emitted.
fn classification_problems(emitted: &[FieldSite], kept: &[&str], redacted: &[&str]) -> Vec<String> {
    let keys = distinct_keys(emitted);
    let kept: BTreeSet<&str> = kept.iter().copied().collect();
    let redacted: BTreeSet<&str> = redacted.iter().copied().collect();
    let first_site = |key: &str| {
        emitted
            .iter()
            .find(|site| site.key == key)
            .map(|site| format!("{}:{}", site.file, site.line))
            .unwrap_or_default()
    };

    let mut problems = Vec::new();
    for site in emitted
        .iter()
        .filter(|site| site.key == MESSAGE_KEY && !site.file.starts_with(MESSAGE_EXEMPT_PREFIX))
    {
        problems.push(format!(
            "`message` is used as a field key at {}:{}. A field called `message` goes into the \
             log message slot unchecked and bypasses the key policy, so the value is not \
             redacted by key. Pick another key (for example `failure_message`) and classify \
             that one.",
            site.file, site.line
        ));
    }
    for key in &keys {
        match (kept.contains(key), redacted.contains(key)) {
            (false, false) => problems.push(format!(
                "`{key}` is emitted (first at {}) but classified nowhere. Add it to KEEP_FIELDS \
                 in crates/reprise-core/src/diagnostics/field_policy.rs only if every place that \
                 emits it carries a number, a bool, a duration, a fieldless enum, a static label \
                 or an internal id; otherwise add it to REDACT_FIELDS.",
                first_site(key)
            )),
            (true, true) => problems.push(format!(
                "`{key}` is in both KEEP_FIELDS and REDACT_FIELDS; keep it in exactly one."
            )),
            _ => {}
        }
    }
    for (list, name) in [(&kept, "KEEP_FIELDS"), (&redacted, "REDACT_FIELDS")] {
        for key in list {
            if !keys.contains(key) {
                problems.push(format!(
                    "`{key}` is listed in {name} in field_policy.rs but no tracing macro emits \
                     it any more; remove it."
                ));
            }
        }
    }
    problems
}

#[test]
fn every_emitted_field_key_is_in_exactly_one_list() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let sweep = workspace_sites(&crates).expect("the workspace sources are readable");
    // A scanner that finds nothing would pass the check below vacuously.
    assert!(sweep.files > 0, "the scan read no source file");
    assert!(!sweep.sites.is_empty(), "the scan found no field site");

    let problems = classification_problems(&sweep.sites, KEEP_FIELDS, REDACT_FIELDS);

    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

fn site(key: &str, file: &str, line: usize) -> FieldSite {
    FieldSite {
        key: key.to_owned(),
        file: file.to_owned(),
        line,
    }
}

#[test]
fn a_correctly_classified_set_has_no_problems() {
    let emitted = [
        site("count", "a/src/x.rs", 3),
        site("path", "a/src/y.rs", 9),
    ];

    assert!(classification_problems(&emitted, &["count"], &["path"]).is_empty());
}

#[test]
fn an_unclassified_key_is_named_with_its_first_site_and_both_lists() {
    let emitted = [
        site("fresh_key", "core/src/a.rs", 12),
        site("fresh_key", "core/src/b.rs", 40),
    ];

    let problems = classification_problems(&emitted, &[], &[]);

    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("`fresh_key`"), "{}", problems[0]);
    assert!(problems[0].contains("core/src/a.rs:12"), "{}", problems[0]);
    assert!(!problems[0].contains("core/src/b.rs"), "{}", problems[0]);
    assert!(problems[0].contains("KEEP_FIELDS"), "{}", problems[0]);
    assert!(problems[0].contains("REDACT_FIELDS"), "{}", problems[0]);
}

#[test]
fn a_key_in_both_lists_is_named() {
    let emitted = [site("twice", "core/src/a.rs", 1)];

    let problems = classification_problems(&emitted, &["twice"], &["twice"]);

    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("`twice`"), "{}", problems[0]);
    assert!(problems[0].contains("both"), "{}", problems[0]);
}

#[test]
fn a_listed_key_nobody_emits_is_stale_and_names_its_list() {
    let emitted = [site("live", "core/src/a.rs", 1)];

    let problems = classification_problems(&emitted, &["live", "old_kept"], &["old_redacted"]);

    assert_eq!(problems.len(), 2, "{problems:?}");
    let kept = problems.iter().find(|p| p.contains("`old_kept`"));
    let redacted = problems.iter().find(|p| p.contains("`old_redacted`"));
    assert!(kept.is_some_and(|p| p.contains("KEEP_FIELDS") && !p.contains("REDACT_FIELDS")));
    assert!(redacted.is_some_and(|p| p.contains("REDACT_FIELDS")));
}

#[test]
fn a_field_called_message_is_refused_with_every_site_even_when_it_is_listed() {
    let emitted = [
        site("message", "gnome/src/ui/x.rs", 77),
        site("message", "core/src/y.rs", 5),
    ];

    for redacted in [vec![], vec!["message"]] {
        let problems = classification_problems(&emitted, &[], &redacted);

        for place in ["gnome/src/ui/x.rs:77", "core/src/y.rs:5"] {
            assert!(
                problems
                    .iter()
                    .any(|p| p.contains(place) && p.contains("Pick another key")),
                "{place}: {problems:?}"
            );
        }
    }
}

#[test]
fn a_field_called_message_in_the_android_crate_is_allowed_when_classified() {
    let emitted = [site("message", "reprise-android-ffi/src/a.rs", 1)];

    assert!(classification_problems(&emitted, &[], &["message"]).is_empty());
}
