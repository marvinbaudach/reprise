//! The crash section's message, location and backtrace grammars.

use super::crash_reduce::{reduce_backtrace, reduce_location, reduce_message, PanicPayload};
use super::log_templates::templates_for_file;

/// A source file of the app that logs an `invariant_broken!` template with a
/// hole, standing for "the panic location's file has templates".
const FILE_WITH_HOLES: &str = "crates/reprise-gnome/src/ui/motion_reveal.rs";

fn formatted(text: &str) -> PanicPayload {
    PanicPayload::Formatted(text.to_owned())
}

// ---- message ----

#[test]
fn a_static_payload_is_kept_as_compiled() {
    let payload = PanicPayload::Static("the queue is empty");

    assert_eq!(reduce_message(&payload, None), "the queue is empty");
}

#[test]
fn a_static_payload_is_one_line_and_at_most_a_kibibyte() {
    let payload = PanicPayload::Static("first\nsecond\r\nthird");
    assert_eq!(reduce_message(&payload, None), "first\\nsecond\\r\\nthird");

    let long: &'static str = Box::leak("é".repeat(2_000).into_boxed_str());
    let shown = reduce_message(&PanicPayload::Static(long), None);
    assert!(shown.len() <= 1024 && !shown.is_empty(), "{}", shown.len());
}

#[test]
fn a_formatted_payload_that_matches_nothing_is_a_placeholder() {
    let payload = formatted("could not open /srv/zqxshare/Zqx Album/01.flac");

    assert_eq!(reduce_message(&payload, None), "…");
    assert_eq!(
        reduce_message(&payload, Some("crates/reprise-core/src/absent.rs")),
        "…"
    );
}

#[test]
fn a_formatted_payload_may_not_forge_a_phrase_with_text_around_it() {
    let payload = formatted("zqx before called `Option::unwrap()` on a `None` value");

    assert_eq!(reduce_message(&payload, None), "…");
}

#[test]
fn a_standard_phrase_hides_the_values_inside_it() {
    let payload =
        formatted("called `Result::unwrap()` on an `Err` value: \"/home/zqxuser/Zqx.flac\"");

    assert_eq!(
        reduce_message(&payload, None),
        "called `Result::unwrap()` on an `Err` value: …"
    );
}

#[test]
fn a_template_of_the_panic_location_file_stands_in_for_its_message() {
    let template = templates_for_file(FILE_WITH_HOLES)
        .iter()
        .find(|template| template.hole_count() > 0)
        .expect("the table has a template with a hole for this file");
    let rendered = template.canonical().replace("{}", "Zqx value");

    let shown = reduce_message(&formatted(&rendered), Some(FILE_WITH_HOLES));

    assert_eq!(shown, template.reduced());
    assert!(!shown.to_lowercase().contains("zqx"), "{shown}");
    assert_eq!(reduce_message(&formatted(&rendered), None), "…");
}

#[test]
fn a_payload_of_another_type_has_a_fixed_text() {
    let error = std::panic::catch_unwind(|| std::panic::panic_any(5_u32)).unwrap_err();
    let payload = PanicPayload::of(error.as_ref());

    assert_eq!(payload, PanicPayload::Other);
    assert_eq!(reduce_message(&payload, None), "non-string panic payload");
}

#[test]
fn the_payload_kind_follows_how_std_created_it() {
    let from_literal = std::panic::catch_unwind(|| std::panic::panic_any("static")).unwrap_err();
    let from_string =
        std::panic::catch_unwind(|| std::panic::panic_any(String::from("owned"))).unwrap_err();

    assert_eq!(
        PanicPayload::of(from_literal.as_ref()),
        PanicPayload::Static("static")
    );
    assert_eq!(
        PanicPayload::of(from_string.as_ref()),
        PanicPayload::Formatted("owned".to_owned())
    );
}

// ---- location ----

#[test]
fn a_workspace_relative_location_is_kept() {
    assert_eq!(
        reduce_location("crates/reprise-gnome/src/ui/window/window.rs:101:17"),
        "crates/reprise-gnome/src/ui/window/window.rs:101:17"
    );
}

#[test]
fn the_location_of_this_very_file_is_kept() {
    let here = format!("{}:{}:{}", file!(), line!(), column!());

    assert_eq!(reduce_location(&here), here);
}

#[test]
fn a_location_in_the_standard_library_is_kept() {
    let location =
        "/rustc/8bab26f4f6df3b0d4d4e3a3c7e1bb1c0cd5e5d00/library/core/src/option.rs:2184:21";
    assert_eq!(reduce_location(location), location);

    let with_parent = "/rustc/8bab26f4f6df3b0d4d4e3a3c7e1bb1c0cd5e5d00/library/std/src/../../backtrace/src/backtrace/libunwind.rs:117:9";
    assert_eq!(reduce_location(with_parent), with_parent);
}

#[test]
fn a_dependency_source_path_is_shortened_to_its_crate_and_version() {
    for (path, shown) in [
        (
            "/home/zqxuser/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/zbus-5.1.0/src/connection.rs:12:3",
            "…/zbus-5.1.0/src/connection.rs:12:3",
        ),
        (
            "/run/build/zqxbuild/cargo/vendor/md-5-0.10.6/src/lib.rs:9:1",
            "…/md-5-0.10.6/src/lib.rs:9:1",
        ),
        (
            "/home/zqxuser/vendor/windows-sys-0.52.0/src/core/mod.rs:3:3",
            "…/windows-sys-0.52.0/src/core/mod.rs:3:3",
        ),
        (
            "/srv/vendor/serde-1.0.200-beta.1+build.5/src/de/mod.rs:5:5",
            "…/serde-1.0.200-beta.1+build.5/src/de/mod.rs:5:5",
        ),
    ] {
        assert_eq!(reduce_location(path), shown, "{path}");
        assert!(!shown.contains("zqx"));
    }
}

#[test]
fn every_other_location_path_is_a_placeholder() {
    for location in [
        "/home/zqxuser/Music/Zqx Album/01.flac:1:1",
        "/srv/zqxshare/Zqx Album/01.flac:1:1",
        "/home/zqxuser/Projects/reprise/crates/reprise-core/src/state_root.rs:1:1",
        "./src/main.rs:1:1",
        "src/main.rs:1:1",
        "crates/Zqx-Crate/src/lib.rs:1:1",
        "crates/reprise-core/src/Zqx.rs:1:1",
        "crates/reprise-core/src/zqx dir/lib.rs:1:1",
        "crates/reprise-core/src/../../../etc/zqx.rs:1:1",
        "crates/reprise-core/src/lib.txt:1:1",
        "crates/reprise-core/lib.rs:1:1",
        "/rustc/zzzz/library/core/src/option.rs:1:1",
        "/rustc/8bab26f4f6df/src/option.rs:1:1",
        "/rustc/8bab26f4f6df/library/core/src/Zqx.rs:1:1",
        "/home/zqxuser/zqx-1.0/lib/src.rs:1:1",
        "/home/zqxuser/zqx-1.0/src/Zqx File.rs:1:1",
        "/home/zqxuser/zqx-one/src/lib.rs:1:1",
    ] {
        assert_eq!(reduce_location(location), "…:1:1", "{location}");
    }
}

#[test]
fn a_location_without_numbers_is_a_placeholder() {
    for location in [
        "crates/reprise-core/src/lib.rs",
        "crates/reprise-core/src/lib.rs:x:y",
        "",
        ":1:2",
    ] {
        let shown = reduce_location(location);
        assert!(
            shown == "…" || shown.starts_with("…:"),
            "{location} -> {shown}"
        );
    }
    assert_eq!(reduce_location("crates/reprise-core/src/lib.rs"), "…");
}

#[test]
fn a_location_with_one_number_keeps_it() {
    assert_eq!(
        reduce_location("crates/reprise-core/src/lib.rs:12"),
        "crates/reprise-core/src/lib.rs:12"
    );
}

// ---- backtrace ----

#[test]
fn a_frame_and_its_location_are_kept_and_normalised() {
    let backtrace = "   0: reprise::ui::window::build::{{closure}}\n             at crates/reprise-gnome/src/ui/window/window.rs:101:17\n  12: <alloc::boxed::Box<F,A> as core::ops::function::FnOnce<Args>>::call_once\n";

    assert_eq!(
        reduce_backtrace(backtrace),
        "   0: reprise::ui::window::build::{{closure}}\n             at crates/reprise-gnome/src/ui/window/window.rs:101:17\n  12: <alloc::boxed::Box<F,A> as core::ops::function::FnOnce<Args>>::call_once"
    );
}

#[test]
fn a_full_format_frame_reduces_to_its_symbol_and_drops_the_address() {
    let backtrace = "   0:     0x55d4c3a1b2c4 - std::rt::lang_start\n                               at /rustc/8bab26f4f6df3b0d4d4e3a3c7e1bb1c0cd5e5d00/library/std/src/rt.rs:206:18\n  12:        0x7f0a1c0291ca - <unknown>\n";

    let shown = reduce_backtrace(backtrace);

    assert_eq!(
        shown,
        "   0: std::rt::lang_start\n             at /rustc/8bab26f4f6df3b0d4d4e3a3c7e1bb1c0cd5e5d00/library/std/src/rt.rs:206:18\n  12: <unknown>"
    );
    assert!(!shown.contains("0x"), "{shown}");
}

#[test]
fn a_full_format_frame_whose_address_is_not_hexadecimal_is_a_placeholder() {
    for line in [
        "   0:     0xzqxuser - reprise::main",
        "   0:     0x - reprise::main",
        "   0:     0x55d4c3a1b2c4 reprise::main",
    ] {
        assert_eq!(reduce_backtrace(line), "…", "{line:?}");
    }
}

#[test]
fn a_short_format_frame_is_unchanged_by_the_address_rule() {
    assert_eq!(
        reduce_backtrace("  7: core::ops::function::FnOnce::call_once"),
        "   7: core::ops::function::FnOnce::call_once"
    );
}

#[test]
fn a_location_line_goes_through_the_location_grammar() {
    let backtrace = "   3: reprise::main\n             at /home/zqxuser/Music/Zqx Album/main.rs:12:5\n   4: std::rt::lang_start\n             at /rustc/8bab26f4f6df3b0d4d4e3a3c7e1bb1c0cd5e5d00/library/std/src/rt.rs:206:18";

    let shown = reduce_backtrace(backtrace);

    assert!(shown.contains("             at …:12:5"), "{shown}");
    assert!(shown.contains("library/std/src/rt.rs:206:18"));
    assert!(!shown.to_lowercase().contains("zqx"), "{shown}");
}

#[test]
fn a_line_outside_the_grammar_becomes_a_placeholder() {
    let backtrace = "0: reprise::main\nthe user opened /srv/zqxshare/Zqx Album/01.flac\n 1: zqx\\path/$HOME\n 2: reprise::main\n x: reprise::main\n   at\n";

    let shown = reduce_backtrace(backtrace);

    assert_eq!(
        shown.lines().collect::<Vec<_>>(),
        [
            "   0: reprise::main",
            "…",
            "…",
            "   2: reprise::main",
            "…",
            "…"
        ]
    );
    assert!(!shown.to_lowercase().contains("zqx"));
}

#[test]
fn a_frame_whose_symbol_is_not_a_symbol_is_a_placeholder() {
    for line in [
        "1: name with /slash",
        "1: has\ttab",
        "1: Zqx$value",
        "1: é",
        "1: ",
    ] {
        assert_eq!(reduce_backtrace(line), "…", "{line:?}");
    }
    assert_eq!(reduce_backtrace(&format!("1: {}", "a".repeat(600))), "…");
}

#[test]
fn blank_lines_of_a_backtrace_are_dropped() {
    assert_eq!(reduce_backtrace("\n  \n0: a::b\n\n"), "   0: a::b");
    assert_eq!(reduce_backtrace(""), "");
}

#[test]
fn a_real_backtrace_keeps_its_own_frame_and_leaks_no_directory() {
    let raw = std::backtrace::Backtrace::force_capture().to_string();

    let shown = reduce_backtrace(&raw);

    assert!(
        shown.contains("crash_reduce_tests::a_real_backtrace_keeps_its_own_frame"),
        "the test's own frame was lost:\n{shown}\n--- raw ---\n{raw}"
    );
    assert!(
        !shown.lines().any(|line| line == "…"),
        "a real frame was lost to the symbol grammar:\n{shown}\n--- raw ---\n{raw}"
    );
    for line in shown.lines() {
        assert!(
            line == "…" || line.trim_start().starts_with("at ") || line.split_once(": ").is_some(),
            "{line:?}"
        );
        assert!(!line.contains("/home/"), "{line}");
    }
}
