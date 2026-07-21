//! Stub binary for `reprise-cli`. Package A (CLI v1) builds the real
//! clap-based surface — `playlist list|show|create|rename|delete`, `search`,
//! `library summary`, `scan`, `events tail`, `--json`, `--db` — on top of
//! `reprise-core`'s facades. It exists now so the workspace member compiles
//! and `cargo test --workspace` covers the MIT-frontend → `reprise-core` seam.

// Locks in the dependency edge `scripts/check-architecture.sh` and package I
// govern; package A replaces this with real facade calls.
use reprise_core as _;

fn main() {}
