//! The scanner behind the classification check reads each field form.

use super::field_sweep::{sweep_source, FileSweep};

fn keys(source: &str) -> Vec<String> {
    sweep_source("fixture.rs", source)
        .expect("the fixture is readable")
        .sites
        .into_iter()
        .map(|site| site.key)
        .collect()
}

fn sweep(source: &str) -> FileSweep {
    sweep_source("fixture.rs", source).expect("the fixture is readable")
}

#[test]
fn reads_assignment_and_shorthand_forms() {
    let found = keys(
        r#"
        fn f() {
            tracing::info!(a = 1, b = %x, c = ?y, d, %e, ?g, "message {}", h = 3);
        }
        "#,
    );

    assert_eq!(found, ["a", "b", "c", "d", "e", "g"]);
}

#[test]
fn reads_dotted_and_quoted_keys() {
    let found = keys(
        r#"
        fn f() {
            info!(restored.browse = 1, ?restored.sort, "net.peer" = 2, "m");
        }
        "#,
    );

    assert_eq!(found, ["restored.browse", "restored.sort", "net.peer"]);
}

#[test]
fn reads_every_level_the_bare_form_and_the_event_macro() {
    let found = keys(
        r#"
        fn f() {
            trace!(a);
            debug!(b = 1);
            warn!(c = 2, "m");
            error!(d, "m");
            tracing::event!(tracing::Level::INFO, e = 5, "m");
            ::tracing::error!(f, "m");
        }
        "#,
    );

    assert_eq!(found, ["a", "b", "c", "d", "e", "f"]);
}

#[test]
fn skips_options_and_format_arguments_after_the_message() {
    let found = keys(
        r#"
        fn f() {
            tracing::warn!(target: "x::y", parent: None, a = 1, "m {} {}", positional, named = 2);
            warn!("Current track slot {} not found", track_slot);
        }
        "#,
    );

    assert_eq!(found, ["a"]);
}

#[test]
fn reads_fields_whose_values_nest_brackets_commas_and_strings() {
    let found = keys(
        r#"
        fn f() {
            info!(
                a = foo(1, "x, y", [2, 3]).map(|v| (v, 1)),
                b = if c { "}" } else { ")" },
                d = 'x',
                "m"
            );
        }
        "#,
    );

    assert_eq!(found, ["a", "b", "d"]);
}

#[test]
fn ignores_strings_comments_other_macros_and_foreign_paths() {
    let found = keys(
        r##"
        // info!(commented = 1);
        /* warn!(block = 1); /* nested */ */
        fn f() {
            let _ = "info!(quoted = 1)";
            let _ = r#"warn!(raw = 1)"#;
            println!("a = {}", a);
            log::info!(foreign = 1);
            format!("{}", error);
        }
        "##,
    );

    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn skips_test_only_items_and_reports_test_only_modules() {
    let swept = sweep(
        r#"
        fn f() { info!(kept = 1); }

        #[cfg(test)]
        mod tests { fn t() { info!(hidden = 1); } }

        #[cfg(all(test, debug_assertions))]
        fn only_in_debug_tests() { info!(hidden_too = 1); }

        #[cfg(any(test, feature = "x"))]
        fn sometimes() { info!(sometimes = 1); }

        #[cfg(not(test))]
        fn never_in_tests() { info!(not_test = 1); }

        #[cfg(test)]
        mod helper;

        #[cfg(test)]
        #[path = "elsewhere_tests.rs"]
        mod moved;

        #[cfg(test)]
        use a::{b, c};
        fn g() { info!(after = 1); }

        struct S {
            kept_field: u8,
            #[cfg(test)]
            test_field: gtk::Widget,
            other: u8,
        }
        fn h() { info!(after_field = 1); }
        "#,
    );

    let found: Vec<&str> = swept.sites.iter().map(|site| site.key.as_str()).collect();
    assert_eq!(
        found,
        ["kept", "sometimes", "not_test", "after", "after_field"]
    );
    assert_eq!(
        swept.test_modules,
        [
            ("helper".to_owned(), None),
            ("moved".to_owned(), Some("elsewhere_tests.rs".to_owned())),
        ]
    );
}

#[test]
fn reports_the_line_of_the_macro() {
    let swept = sweep("fn f() {\n\n    info!(a = 1);\n}\n");

    assert_eq!(swept.sites[0].line, 3);
}

#[test]
fn a_part_it_cannot_read_is_an_error_not_a_silent_miss() {
    let error = sweep_source("fixture.rs", "fn f() { info!(a.b(), \"m\"); }")
        .expect_err("a call is not a field");

    assert!(error.contains("fixture.rs:1"), "{error}");
}
