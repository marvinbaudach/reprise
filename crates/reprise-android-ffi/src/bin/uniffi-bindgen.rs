//! Bindings generator entry point (spike only). UniFFI's `--library` mode
//! reads the compiled `.so` and emits Kotlin from the proc-macro metadata, so
//! there is no `.udl` file to keep in sync with the Rust source.

fn main() {
    uniffi::uniffi_bindgen_main();
}
