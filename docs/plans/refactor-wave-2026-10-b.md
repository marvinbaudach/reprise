---
slug: refactor-wave-2026-10-b
worktree: /home/marvin/Projects/reprise-refactor-wave-2026-10-b
branch: feature/refactor-wave-2026-10-b
phase: shipped
codex_session:
created: 2026-10-04
---
# Refactor wave 2026-10 — strand B

Mother plan: `docs/plans/refactor-wave-2026-10.md`. Its "Standing rules for every strand" bind this strand.

## Strand B — persistence registries (db + settings) and the ownership record

**Purpose.**

- Adding a migration becomes one table line, and the supported version is derived from that
  table instead of being maintained separately.
- `settings.rs` splits along its own sibling pattern.
- AGENTS.md stops claiming dead ownerships.

**Owns:**

- `crates/reprise-core/src/db.rs`
- New `crates/reprise-core/src/db_schema_baseline.rs`
- New `crates/reprise-core/src/db_migrations.rs`, if needed
- `crates/reprise-core/src/lib.rs`, module-declaration lines only
- `crates/reprise-core/src/library/settings*.rs`. This glob means only files beside
  `library/settings.rs`; `device_sync/settings.rs` belongs to strand A.
- `AGENTS.md`, the three stale ownership sections only.

**B1 — test first (red).** Add `migration_registry_is_contiguous_and_ends_at_supported_version`.
It asserts:

- the registry holds versions 19..=N, with no gap and no duplicate;
- `N == SUPPORTED_SCHEMA_VERSION`.

The existing suites stay green unchanged: `db_tests.rs`, `db_migration_repair_tests.rs`,
`db_handle_tests.rs` and every `db_*_migration_tests.rs`.

**B2 — freeze the baseline.**

- **What moves.** The `SCHEMA_V1`..`SCHEMA_V18` consts, the inline v1–v18 steps from
  `migrate_with_cache_dirs`, and any helper only they use (`grandfather_network_features` if that
  holds) go into `db_schema_baseline.rs`. The entry point is
  `pub(crate) fn migrate_baseline(conn, existing_database, cover_cache, portrait_cache)`.
- **What stays identical.** Statement order, one `unchecked_transaction` per step, and
  `user_version` stamping.
- **`initial_version`.** It is still read in `db.rs` before the baseline runs, and the
  "newer than supported" refusal still happens first.
- **Rustdoc.** Move the doc comments that explain the per-step transaction design with the code.

**B3 — the registry.**

```rust
struct MigrationContext<'a> { existing_database: bool, cover_cache: &'a Path, portrait_cache: &'a Path }
struct Migration { version: i64, run: fn(&Connection, &MigrationContext<'_>) -> Result<(), rusqlite::Error> }
const MIGRATIONS: &[Migration] = &[
    Migration { version: 19, run: |c, _| crate::db_library_doctor::migrate_v19(c) },
    // … v20–v87, one line each; v50 passes the context fields through …
];
pub const SUPPORTED_SCHEMA_VERSION: i64 = MIGRATIONS[MIGRATIONS.len() - 1].version;
```

- **The runner.** It walks `MIGRATIONS` in order. Each `migrate_vN` keeps its own version check
  and transaction. The `migrate_vN` functions themselves are not edited.
- **Where it lives.** Put the table in `db_migrations.rs` if `db.rs` would otherwise stay large.
- **The constant.** `SUPPORTED_SCHEMA_VERSION` keeps its public path and value (87).

**B4 — split `library/settings.rs` (733 lines)** along the file's own
`#[path = "settings_x.rs"] mod x; pub use x::*;` pattern:

- `settings_layout.rs`: window mode, sidebar and panel visibility, compact layout, decorations.
- `settings_playback.rs`: gapless, crossfade, volume-key skip, replay gain, transition.
- `settings_auto_clean.rs`: auto-clean.

Rules for the split:

- Key constants move with their getters and setters.
- `migrate_v79` and `migrate_v80` stay reachable for B3's registry.
- Every public path `reprise_core::library::settings::X` still resolves. Compiling the workspace
  proves it.
- Target: `settings.rs` under 400 lines.

**B5 — AGENTS.md.** Turn the three stale sections into "Completed file ownership — …" sections,
in the style of the "Library Doctor fix round 3" record:

- one paragraph saying the plans were deleted on landing, no branch remains, checked
  2026-10-04;
- the tables kept as historical boundaries.

Change no other section.

**Verification (B):**

```
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test -p reprise-core
scripts/check-architecture.sh
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # must be empty
```
