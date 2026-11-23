//! The generator reads each message literal, and the table renders from them.

use super::field_sweep::{sweep_source, workspace_sites};
use super::field_sweep_literals::table_text;

/// The reduced form of every template the fixture yields, in source order.
fn reduced(source: &str) -> Vec<String> {
    sweep_source("reprise-core/src/fixture.rs", source)
        .expect("the fixture is readable")
        .templates
        .iter()
        .map(|site| site.template.reduced())
        .collect()
}

fn error_of(source: &str) -> String {
    sweep_source("reprise-core/src/fixture.rs", source).expect_err("the fixture is refused")
}

#[test]
fn a_plain_message_is_the_template() {
    assert_eq!(
        reduced(r#"fn f() { tracing::warn!("could not rebuild the sidebar"); }"#),
        ["could not rebuild the sidebar"]
    );
}

#[test]
fn holes_become_placeholders_and_doubled_braces_stay_literal() {
    assert_eq!(
        reduced(r#"fn f() { info!("track {id} of {} {{kept}}", total); }"#),
        ["track … of … {kept}"]
    );
}

#[test]
fn fields_before_the_message_do_not_hide_it() {
    assert_eq!(
        reduced(r#"fn f() { error!(path = %p, count = 3, ?state, "failed {}", 1); }"#),
        ["failed …"]
    );
}

#[test]
fn a_target_option_and_an_event_level_are_skipped() {
    assert_eq!(
        reduced(
            r#"fn f() {
                tracing::event!(target: "reprise::x", tracing::Level::INFO, "first");
                event!(Level::WARN, a = 1, "second");
            }"#
        ),
        ["first", "second"]
    );
}

#[test]
fn a_multi_line_call_with_a_continued_message_is_read_whole() {
    let source = "fn f() {\n    warn!(\n        error = %e,\n        \"could not open \\\n         the file {}\",\n        1,\n    );\n}\n";

    assert_eq!(reduced(source), ["could not open the file …"]);
}

#[test]
fn a_raw_string_message_keeps_its_backslashes() {
    assert_eq!(
        reduced(r##"fn f() { debug!(r#"pattern \d+ "x""#); }"##),
        [r#"pattern \d+ "x""#]
    );
}

#[test]
fn escapes_in_a_message_are_unescaped() {
    assert_eq!(
        reduced(r#"fn f() { info!("tab\there\nnext \u{e9} \x41 \"q\" \\"); }"#),
        ["tab\there\nnext é A \"q\" \\"]
    );
}

#[test]
fn spans_and_events_without_a_message_have_no_template() {
    assert_eq!(
        reduced(
            r#"fn f() {
                let _s = tracing::info_span!("opening", a = 1);
                trace!(a);
            }"#
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_test_only_item_has_no_template() {
    let source = r#"
        fn f() { info!("kept"); }
        #[cfg(test)]
        mod tests { fn g() { info!("dropped"); } }
        #[cfg(test)]
        fn h() { invariant_broken!("dropped too"); }
    "#;

    assert_eq!(reduced(source), ["kept"]);
}

#[test]
fn an_invariant_site_is_filed_with_its_prefix() {
    assert_eq!(
        reduced(r#"fn f() { invariant_broken!("row {} has no model", 7); }"#),
        ["invariant broken: row … has no model"]
    );
}

#[test]
fn a_multi_line_invariant_call_is_read() {
    let source = "fn f() {\n    x => invariant_broken!(\n        \"queued offline for {}\",\n        id,\n    ),\n}\n";

    assert_eq!(reduced(source), ["invariant broken: queued offline for …"]);
}

#[test]
fn the_macro_definition_and_a_mention_are_not_invariant_sites() {
    let source = r#"
        macro_rules! invariant_broken { ($($m:tt)+) => {{}}; }
        use crate::invariant_broken;
        fn f() { let _ = stringify!(invariant_broken); }
    "#;

    assert_eq!(reduced(source), Vec::<String>::new());
}

#[test]
fn the_tracing_call_inside_the_invariant_macro_body_is_neither_a_template_nor_a_field() {
    // The body forwards the caller's literal with `concat!`, so its message is
    // not one string literal; the use sites carry the template instead.
    let source = r#"
        macro_rules! invariant_broken {
            ($fmt:literal $($rest:tt)*) => {{
                ::tracing::error!(concat!("invariant broken: ", $fmt) $($rest)*);
            }};
        }
        fn f() { invariant_broken!("row {} has no model", 7); tracing::warn!("after"); }
    "#;

    assert_eq!(
        reduced(source),
        ["invariant broken: row … has no model", "after"]
    );
}

#[test]
fn an_invariant_site_that_does_not_start_with_a_literal_is_refused() {
    let error = error_of("fn f() {\n invariant_broken!(message);\n}\n");

    assert!(error.contains("reprise-core/src/fixture.rs:2"), "{error}");
    assert!(error.contains("invariant_broken!"), "{error}");
}

#[test]
fn a_malformed_format_string_is_refused_with_its_site() {
    let unclosed = error_of("fn f() {\n\n info!(\"open {\");\n}\n");
    let stray = error_of("fn f() { warn!(\"close }\"); }");
    let invariant = error_of("fn f() { invariant_broken!(\"bad }\"); }");

    assert!(
        unclosed.contains("reprise-core/src/fixture.rs:3"),
        "{unclosed}"
    );
    assert!(unclosed.contains("never closed"), "{unclosed}");
    assert!(stray.contains("reprise-core/src/fixture.rs:1"), "{stray}");
    assert!(invariant.contains("bad }"), "{invariant}");
}

#[test]
fn a_message_that_is_not_one_literal_is_refused() {
    let error = error_of("fn f() { info!(\"a\" \"b\"); }");

    assert!(error.contains("one string literal"), "{error}");
}

fn sites_of(file: &str, source: &str) -> Vec<super::field_sweep_literals::TemplateSite> {
    sweep_source(file, source)
        .expect("the fixture is readable")
        .templates
}

#[test]
fn the_table_is_sorted_distinct_and_keyed_by_the_workspace_path() {
    let mut sites = sites_of(
        "reprise-gnome/src/b.rs",
        r#"fn f() { info!("zeta {}", 1); info!("alpha"); info!("zeta {x}"); }"#,
    );
    sites.extend(sites_of(
        "reprise-core/src/a.rs",
        r#"fn f() { warn!("middle"); }"#,
    ));

    assert_eq!(
        table_text(&sites),
        "crates/reprise-core/src/a.rs\tmiddle\n\
         crates/reprise-gnome/src/b.rs\talpha\n\
         crates/reprise-gnome/src/b.rs\tzeta {}\n"
    );
}

#[test]
fn the_table_escapes_backslash_tab_newline_and_carriage_return_and_keeps_braces_literal() {
    let sites = sites_of(
        "reprise-core/src/a.rs",
        r#"fn f() { info!("a\\b\tc\nd\re {{x}} {}", 1); }"#,
    );

    assert_eq!(
        table_text(&sites),
        "crates/reprise-core/src/a.rs\ta\\\\b\\tc\\nd\\re {{x}} {}\n"
    );
}

#[test]
fn the_table_covers_only_the_crates_whose_events_reach_the_desktop() {
    let mut sites = sites_of("reprise-cli/src/main.rs", r#"fn f() { info!("cli"); }"#);
    sites.extend(sites_of(
        "reprise-android-ffi/src/lib.rs",
        r#"fn f() { info!("android"); }"#,
    ));
    sites.extend(sites_of(
        "reprise-core-extra/src/lib.rs",
        r#"fn f() { info!("lookalike"); }"#,
    ));
    sites.extend(sites_of(
        "reprise-view/src/lib.rs",
        r#"fn f() { info!("view"); }"#,
    ));
    sites.extend(sites_of(
        "reprise-platform-linux/src/lib.rs",
        r#"fn f() { info!("linux"); }"#,
    ));

    assert_eq!(
        table_text(&sites),
        "crates/reprise-platform-linux/src/lib.rs\tlinux\n\
         crates/reprise-view/src/lib.rs\tview\n"
    );
}

#[test]
fn the_workspace_sweep_collects_templates_from_production_files_only() {
    let crates = tempfile::tempdir().expect("a scratch directory can be created");
    let source = crates.path().join("reprise-core/src");
    std::fs::create_dir_all(source.join("tests")).expect("the crate directory can be created");
    std::fs::write(
        source.join("lib.rs"),
        "mod loud;\n#[cfg(test)]\nmod quiet;\nfn f() { info!(\"in lib\"); }\n",
    )
    .expect("the source can be written");
    std::fs::write(
        source.join("loud.rs"),
        "fn g() { warn!(\"in loud {}\", 1); }\n",
    )
    .expect("the source can be written");
    std::fs::write(source.join("quiet.rs"), "fn h() { warn!(\"in quiet\"); }\n")
        .expect("the source can be written");
    std::fs::write(
        source.join("lib_tests.rs"),
        "fn t() { warn!(\"in tests\"); }\n",
    )
    .expect("the source can be written");

    let sweep = workspace_sites(crates.path()).expect("the scratch tree is readable");

    assert_eq!(
        table_text(&sweep.templates),
        "crates/reprise-core/src/lib.rs\tin lib\n\
         crates/reprise-core/src/loud.rs\tin loud {}\n"
    );
}
