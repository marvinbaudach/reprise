//! Stub binary for `reprise-mcp`. Package B (MCP v1) builds the real
//! stdio server — resources (`reprise://library/summary`,
//! `reprise://playlists`) and tools (`music_search_tracks`,
//! `music_create_playlist`) over `reprise-core`, with stderr-only logging and
//! a protocol-clean stdout. It exists now so the workspace member compiles and
//! `cargo test --workspace` covers the MIT-frontend → `reprise-core` seam.

// Locks in the dependency edge `scripts/check-architecture.sh` and package I
// govern; package B replaces this with the real server.
use reprise_core as _;

fn main() {}
