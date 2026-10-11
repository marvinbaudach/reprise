//! GP-4: production code of the GTK frontend does not call `unwrap`, `expect`,
//! `panic!`, `todo!`, `unimplemented!` or `unreachable!`; clippy enforces it
//! from the crate root. GTK-free: it reads the root's source, so it also runs
//! where no display is available.

const CRATE_ROOT: &str = include_str!("main.rs");

const SIX_LINTS: [&str; 6] = [
    "clippy::expect_used",
    "clippy::panic",
    "clippy::todo",
    "clippy::unimplemented",
    "clippy::unreachable",
    "clippy::unwrap_used",
];

/// The text of the crate-root `#![cfg_attr(…)]` attribute, if there is one.
fn root_attribute() -> Option<&'static str> {
    let start = CRATE_ROOT.find("#![cfg_attr(")?;
    let rest = &CRATE_ROOT[start..];
    let end = rest.find(")]")?;
    Some(&rest[..end + ")]".len()])
}

#[test]
fn gp_4_the_crate_root_denies_exactly_the_six_panic_lints_outside_tests() {
    let attribute = root_attribute().expect("main.rs carries the never-crash attribute");
    let compact = attribute.split_whitespace().collect::<String>();
    assert!(
        compact.starts_with("#![cfg_attr(not(test),deny("),
        "the lints must be denied for non-test builds only: {attribute}"
    );

    let mut named = attribute
        .match_indices("clippy::")
        .map(|(at, _)| {
            attribute[at..]
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
                .next()
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    named.sort_unstable();
    assert_eq!(
        named, SIX_LINTS,
        "the attribute must name exactly the six never-crash lints"
    );
}
