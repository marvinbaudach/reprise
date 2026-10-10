//! Bindings generator entry point (spike only). UniFFI's `--library` mode
//! reads the compiled `.so` and emits Kotlin from the proc-macro metadata, so
//! there is no `.udl` file to keep in sync with the Rust source.

// Production code must not panic on bad input or broken invariants: every
// fallible step returns an error instead. A genuine, documented invariant is
// suppressed at its site with `#[expect(clippy::<lint>, reason = "…")]`, and
// scripts/check-crate-layers.sh counts those suppressions against a budget.
// Test builds are exempt; test doubles legitimately use unreachable!/expect.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable
    )
)]

fn main() {
    uniffi::uniffi_bindgen_main();
}
