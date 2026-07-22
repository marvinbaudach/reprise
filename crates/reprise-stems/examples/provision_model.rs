//! Provisions the pinned htdemucs weights through the **production** path —
//! [`reprise_stems::provision::ensure_weights`] (SHA-256 checksum, atomic write,
//! licence notice beside the file) fed by the real streaming HTTP fetcher — into
//! a target model directory, printing download progress.
//!
//! ```sh
//! # Into the default <XDG data>/reprise/models:
//! cargo run -p reprise-stems --example provision_model --features ort --release
//! # Into an explicit directory (e.g. a scratch XDG for a smoke test):
//! cargo run -p reprise-stems --example provision_model --features ort --release -- /path/to/models
//! ```
//!
//! A present, checksum-valid model is not re-downloaded — the run just verifies
//! it and (re)writes the licence notice, so it is safe to re-run. Requires the
//! `ort` feature for the real `ureq` fetcher.

use std::cell::Cell;
use std::io::Write as _;
use std::path::PathBuf;

use reprise_stems::model::HTDEMUCS_FP32;
use reprise_stems::provision::{
    default_model_dir, ensure_weights, http_fetcher_with_progress, license_path,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model_dir: PathBuf = match std::env::args().nth(1) {
        Some(dir) => PathBuf::from(dir),
        None => default_model_dir()?,
    };
    println!(
        "Provisioning {} ({} bytes) into {}",
        HTDEMUCS_FP32.model_id,
        HTDEMUCS_FP32.size_bytes,
        model_dir.display()
    );

    // Interior mutability so the fetcher stays an `Fn` (ensure_weights' contract)
    // while throttling the printout to one line per whole percent.
    let last_percent = Cell::new(u64::MAX);
    let fetch = |url: &str| -> Result<Vec<u8>, String> {
        http_fetcher_with_progress(url, &mut |read, total| {
            if let Some(total) = total.filter(|total| *total > 0) {
                let percent = read * 100 / total;
                if percent != last_percent.get() {
                    last_percent.set(percent);
                    print!("\rDownloading… {percent}% ({read}/{total} bytes)");
                    let _ = std::io::stdout().flush();
                }
            }
        })
    };

    let path = ensure_weights(&model_dir, &HTDEMUCS_FP32, &fetch)?;
    println!("\nModel ready:     {}", path.display());
    println!(
        "Licence notice:  {}",
        license_path(&model_dir, &HTDEMUCS_FP32).display()
    );
    Ok(())
}
