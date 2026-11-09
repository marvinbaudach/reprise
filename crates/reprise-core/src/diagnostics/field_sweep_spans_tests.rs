//! The field sweep reads span fields too: `span!`, the `*_span!` macros and
//! `#[instrument(fields(…))]`.

use super::field_sweep::sweep_source;

fn keys(source: &str) -> Vec<String> {
    sweep_source("fixture.rs", source)
        .expect("the fixture is readable")
        .sites
        .into_iter()
        .map(|site| site.key)
        .collect()
}

#[test]
fn a_span_macro_after_its_name_reads_fields_like_an_event() {
    let found = keys(
        r#"
        fn f() {
            tracing::info_span!("startup measurement", measurement);
            trace_span!("n", a = 1, ?b, %c, "net.peer" = 2);
            tracing::debug_span!(target: "x", parent: None, "n", d = 1);
            warn_span!("n", restored.browse = 1);
            ::tracing::error_span!("n", e);
            info_span!("no fields");
        }
        "#,
    );

    assert_eq!(
        found,
        [
            "measurement",
            "a",
            "b",
            "c",
            "net.peer",
            "d",
            "restored.browse",
            "e"
        ]
    );
}

#[test]
fn the_span_macro_takes_a_level_and_a_name_before_its_fields() {
    let found = keys(
        r#"
        fn f() {
            tracing::span!(tracing::Level::INFO, "n", a = 1, b);
            span!(LEVEL, "n", c = 2);
            span!(target: "t", Level::DEBUG, "n", d = ?x);
            span!(Level::TRACE, "only a name");
        }
        "#,
    );

    assert_eq!(found, ["a", "b", "c", "d"]);
}

#[test]
fn a_span_name_held_in_a_constant_is_not_taken_for_a_field() {
    let found = keys("fn f() { info_span!(NAME, a = 1); span!(LEVEL, NAME, b = 2); }");

    assert_eq!(found, ["a", "b"]);
}

#[test]
fn span_fields_behind_a_test_cfg_are_not_counted() {
    let found = keys(
        r#"
        #[cfg(test)]
        fn t() { info_span!("n", hidden = 1); }
        fn kept() { info_span!("n", shown = 1); }
        "#,
    );

    assert_eq!(found, ["shown"]);
}

#[test]
fn instrument_fields_are_read_when_every_argument_is_skipped() {
    let found = keys(
        r#"
        #[instrument(skip_all, fields(a = 1, b = %c, d, restored.x = 2))]
        fn one(path: &Path) {}

        #[tracing::instrument(skip_all, level = "debug", name = "n", target = "t", fields(e = 1))]
        fn two() {}

        #[instrument(skip_all)]
        fn three() {}

        #[derive(Debug)]
        #[instrument(fields(f = 1), skip_all)]
        fn four() {}
        "#,
    );

    assert_eq!(found, ["a", "b", "d", "restored.x", "e", "f"]);
}

#[test]
fn instrument_in_a_test_only_item_is_not_counted() {
    let found = keys(
        r#"
        #[cfg(test)]
        mod t {
            #[instrument(skip_all, fields(hidden = 1))]
            fn f() {}
        }
        "#,
    );

    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_instrument_the_sweep_cannot_vouch_for_is_an_error_with_its_line() {
    for (source, reason) in [
        ("#[instrument]\nfn f(path: &Path) {}", "skip_all"),
        (
            "#[instrument(fields(a = 1))]\nfn f(path: &Path) {}",
            "skip_all",
        ),
        (
            "#[instrument(skip(a), fields(b = 1))]\nfn f(a: u8) {}",
            "skip_all",
        ),
        ("#[instrument(skip_all, ret)]\nfn f() {}", "ret"),
        ("#[instrument(skip_all, err)]\nfn f() {}", "err"),
        (
            "#[cfg_attr(feature = \"x\", instrument(skip_all))]\nfn f() {}",
            "cfg_attr",
        ),
        ("#[other::instrument(skip_all)]\nfn f() {}", "instrument"),
    ] {
        let error = sweep_source("fixture.rs", source).expect_err(source);

        assert!(error.contains("fixture.rs:1"), "{source}: {error}");
        assert!(error.contains(reason), "{source}: {error}");
        assert!(error.contains("instrument"), "{source}: {error}");
    }
}

#[test]
fn an_unreadable_span_field_is_an_error_like_an_event_field() {
    let error = sweep_source("fixture.rs", "fn f() { info_span!(\"n\", a.b()); }")
        .expect_err("a call is not a field");

    assert!(error.contains("fixture.rs:1"), "{error}");
    assert!(error.contains("in parentheses"), "{error}");
}
