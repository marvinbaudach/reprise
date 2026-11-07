//! The scanner behind the classification check reads each field form.

use std::path::{Path, PathBuf};

use super::field_sweep::{sweep_source, test_only_paths, FileSweep};

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

#[test]
fn an_identifier_called_level_inside_a_field_value_does_not_hide_the_field() {
    let found = keys("fn f() { event!(Level::INFO, a = ?Level::DEBUG, b = 2); }");

    assert_eq!(found, ["a", "b"]);
}

#[test]
fn a_level_held_in_a_constant_is_not_taken_for_a_field() {
    let found = keys(
        r#"
        fn f() {
            event!(LEVEL, a = 1);
            tracing::event!(target: "x", parent: id, LEVEL, b = 2, "m");
            event!(tracing::Level::WARN, c);
        }
        "#,
    );

    assert_eq!(found, ["a", "b", "c"]);
}

#[test]
fn an_aliased_tracing_import_is_an_error_naming_the_site() {
    for source in [
        "use tracing::info as i;",
        "fn f() {}\nuse tracing::{debug, warn as w};",
        "pub(crate) use tracing::{error as e};",
        "use tracing::event as ev;",
        "use tracing as t;",
        "use tracing::{self as t};",
    ] {
        let error = sweep_source("fixture.rs", source).expect_err(source);

        assert!(error.contains("fixture.rs:"), "{source}: {error}");
        assert!(error.contains("alias"), "{source}: {error}");
        assert!(error.contains("tracing"), "{source}: {error}");
    }
}

#[test]
fn imports_that_are_not_aliased_tracing_macros_are_fine() {
    let swept = sweep(
        r#"
        use std::fmt::Result as FmtResult;
        use tracing::field::Visit as V;
        use tracing::{info, Level as L, warn};
        #[cfg(test)]
        use tracing::debug as hidden_alias;
        fn f() { info!(a = 1); }
        "#,
    );

    assert_eq!(swept.sites.len(), 1);
}

#[test]
fn a_test_only_item_with_a_crate_visibility_and_generics_is_cut_whole() {
    let found = keys(
        r#"
        #[cfg(test)] pub(crate) fn a<A, B>() { info!(hidden_a = 1); }
        #[cfg(test)] pub(super) fn b<A, B>() { info!(hidden_b = 1); }
        #[cfg(test)] pub(in crate::x) fn c<A, B>() { info!(hidden_c = 1); }
        fn kept() { info!(kept = 1); }
        "#,
    );

    assert_eq!(found, ["kept"]);
}

#[test]
fn a_brace_group_inside_a_generic_argument_does_not_end_the_item() {
    let found = keys(
        r#"
        #[cfg(test)]
        fn t<T>() where T: Foo<{ 1 }> { info!(hidden = 1); }
        #[cfg(test)]
        impl<const N: usize> Bar<{ N }> for Baz { fn f() { info!(hidden_too = 1); } }
        #[cfg(test)]
        const fn c<T: Foo<{ 2 }>>() -> Option<T> { info!(hidden_const = 1); None }
        fn kept() { info!(kept = 1); }
        "#,
    );

    assert_eq!(found, ["kept"]);
}

#[test]
fn an_inner_test_cfg_makes_the_whole_file_test_only() {
    let swept = sweep("#![cfg(test)]\nmod helper;\nfn f() { info!(hidden = 1); }");

    assert!(swept.test_only);
    assert!(swept.sites.is_empty());
}

#[test]
fn other_inner_attributes_leave_the_file_in_production() {
    let swept = sweep(
        r#"
        #![cfg_attr(not(test), deny(clippy::unwrap_used))]
        #![allow(dead_code)]
        fn f() { info!(kept = 1); }
        "#,
    );

    assert!(!swept.test_only);
    assert_eq!(swept.sites.len(), 1);
}

#[test]
fn a_path_attribute_before_the_test_cfg_still_names_the_module() {
    let swept = sweep(
        r#"
        #[path = "before_tests.rs"]
        #[cfg(test)]
        mod before;

        #[path = "production.rs"]
        mod production;
        "#,
    );

    assert_eq!(
        swept.test_modules,
        [("before".to_owned(), Some("before_tests.rs".to_owned()))]
    );
}

#[test]
fn a_binary_root_resolves_its_modules_beside_itself() {
    let swept = FileSweep {
        test_modules: vec![("helper".to_owned(), None)],
        ..FileSweep::default()
    };

    let bin = test_only_paths(Path::new("/c/src/bin/tool.rs"), &swept);
    let nested = test_only_paths(Path::new("/c/src/ui/panel.rs"), &swept);

    let expect = |paths: &[PathBuf], suffix: &str| paths.contains(&PathBuf::from(suffix));
    assert!(expect(&bin, "/c/src/bin/helper.rs"), "{bin:?}");
    assert!(expect(&bin, "/c/src/bin/helper"), "{bin:?}");
    assert!(expect(&nested, "/c/src/ui/panel/helper.rs"), "{nested:?}");
}
