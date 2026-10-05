---
slug: refactor-wave-2026-10-w3a
worktree: /home/marvin/Projects/reprise-refactor-wave-2026-10-w3a
branch: feature/refactor-wave-2026-10-w3a
phase: shipped
codex_session:
created: 2026-10-05
---
# Refactor wave 2026-10, wave 3 — strand A: `CoreError`, the additive slice

Mother plan: `docs/plans/refactor-wave-2026-10-w3.md`, and the "Standing rules for every strand" in
`docs/plans/refactor-wave-2026-10.md` bind this strand. Consolidation packages 3.1 and 3.2
(`docs/plans/consolidation-plan.md` §5) and finding A2 (`docs/plans/architecture-consolidation.md`
§2.3) are the origin. Where they describe the end state (every core signature converted), this plan
deliberately stops earlier: it is the **additive slice** the grill fixed on 2026-10-04.

This plan is your only channel. When the code disagrees with a table below, the code wins: keep what
the code does and say so in your final message.

## Purpose

`reprise_core::CoreError` exists; every core facade that `reprise-cli` or `reprise-mcp` calls returns
`Result<_, CoreError>` instead of `Result<_, rusqlite::Error>`; both crates drop `rusqlite` from
`[dependencies]`, never name it in `src/`, and `scripts/check-architecture.sh` keeps it that way.
Everything else — the ~700 internal `rusqlite::Error` signatures, `DbError`, `ScanError` and the other
domain enums that embed `rusqlite::Error` — stays as it is.

**Behaviour-preserving means:** every error message text printed by the CLI or returned by the MCP
server stays byte-identical, every exit code and retry decision stays the same, no SQL changes, no
schema change, no signature outside `reprise-core`/`reprise-cli`/`reprise-mcp` changes.

## Evidence (origin/dev @ 0322ae01df, 2026-10-05)

- `crates/reprise-cli/Cargo.toml`: `rusqlite = { version = "0.40", features = ["bundled"] }` at line 48
  (`[dependencies]`, comment lines 39-44) and line 75 (`[dev-dependencies]`). Features: `default`,
  `worker` (`dep:reprise-stems`, `reprise-stems/ort`, `dep:signal-hook`), `mpris` (`dep:zbus`,
  `dep:reprise-runtime-protocol`).
- `crates/reprise-mcp/Cargo.toml`: `rusqlite` at line 28 (`[dependencies]`, comment 23-26) and line 68
  (`[dev-dependencies]`, next to `reprise-core = { path = "../reprise-core", features = ["test-fixtures"] }`).
  Features: `default`, `mpris`.
- `crates/reprise-core/Cargo.toml`: `thiserror = "2"` (line 16); features `default = []`,
  `test-fixtures = []`. Nothing named `CoreError`, `LibraryError` or `EngineError` exists in core.
- **The headless sources name `rusqlite` at these places** (every one must go):

  | File:line | What | Becomes |
  | --- | --- | --- |
  | `crates/reprise-cli/src/error.rs:98-102` | `impl From<rusqlite::Error> for CliError { Self::Database(error.to_string()) }` | `impl From<CoreError> for CliError`, same body |
  | `crates/reprise-cli/src/retry.rs:63-72` | `pub fn rusqlite_is_busy(error: &rusqlite::Error) -> bool` — `SqliteFailure` with `ErrorCode::DatabaseBusy \|\| DatabaseLocked` | deleted; callers pass `CoreError::is_busy` |
  | `crates/reprise-cli/src/retry.rs:76-84` | `scan_is_busy`: `ScanError::Sqlite(inner) => rusqlite_is_busy(inner)`, `ScanError::Db(DbError::Sqlite(inner)) => rusqlite_is_busy(inner)` | both arms call `reprise_core::library::stats::is_database_busy(inner)` (public, exists, same predicate) |
  | `crates/reprise-cli/src/retry.rs:170-190` (`#[cfg(test)]`) | `rusqlite_busy_and_locked_are_retryable`, `rusqlite_other_failures_are_not_retryable` construct `rusqlite::Error::SqliteFailure(ffi::Error::new(SQLITE_BUSY \| SQLITE_LOCKED \| SQLITE_READONLY))` and `QueryReturnedNoRows` | moved to core as the classification tests (A1); deleted here |
  | `crates/reprise-cli/src/retry.rs:63,75` | doc comments that say "rusqlite" | reworded; the gate greps the bare word |
  | `crates/reprise-cli/src/commands/playlist.rs:13,20-21` | `fn retry_write<T>(op: impl FnMut() -> Result<T, rusqlite::Error>) -> Result<T, CliError> { with_retry(op, rusqlite_is_busy).map_err(CliError::from) }` | `Result<T, CoreError>`, predicate `CoreError::is_busy` |
  | `crates/reprise-cli/src/commands/worker.rs:43,51-52,279,303` (`cfg(feature = "worker")`) | `retrying<T>(..)` same shape; two more `rusqlite_is_busy` predicates | same change |
  | `crates/reprise-cli/src/commands/instrumental.rs:23,166,302` | `rusqlite_is_busy` as predicate | `CoreError::is_busy` |
  | `crates/reprise-mcp/src/capability.rs:50,55,60,65,70,76,81,100,109,118,123,128` | twelve `-> Result<bool, rusqlite::Error>` wrappers around `settings::get_bool` (76/81/128 are `cfg(feature = "mpris")`) | `-> Result<bool, CoreError>` |
  | `crates/reprise-mcp/src/startup.rs:22` | `StartupError::Query(rusqlite::Error)` | `Query(CoreError)`; `main.rs:74` prints it with `{error}` — Display text unchanged |
  | `crates/reprise-mcp/src/data.rs:35` | `DataError::Db(rusqlite::Error)` | `Db(CoreError)`; the 71 `.map_err(DataError::Db)` sites compile unchanged |
  | `crates/reprise-mcp/src/data.rs:409-423` | `map_create_error(error: rusqlite::Error)` using `is_constraint_violation` (`ErrorCode::ConstraintViolation`) → `DataError::InvalidInput("one or more track ids do not exist in the library")` else `DataError::Db(error)` | `map_create_error(error: CoreError)` using `error.is_conflict()`; `is_constraint_violation` deleted |
  | `crates/reprise-mcp/src/doctor_actions.rs:525` | `DoctorError::Database(error) => DataError::Db(error)` (binds a `rusqlite::Error` without naming it) | `DataError::Db(error.into())` |

  Integration tests under `crates/reprise-cli/tests/**` and `crates/reprise-mcp/tests/**` open raw
  connections and seed with SQL (`INSERT INTO tracks`, `concert_events`, `library_doctor_*`,
  `tag_write_jobs`, `new_releases`, `BEGIN IMMEDIATE` holds, `pragma_update(user_version)`). Core offers
  no seeding API for any of that (checked: no `fixture`/`test_support`/`test-fixtures` DB helper).
- **Busy predicate that already exists in core:** `crates/reprise-core/src/library/stats.rs:227`
  `pub fn is_database_busy(error: &rusqlite::Error) -> bool` (`DatabaseBusy | DatabaseLocked`), used by
  `reprise-android-ffi` (`play_recorder.rs:319`, `play_recorder_writer.rs:55`), which has no rusqlite
  dependency. A private copy sits at `crates/reprise-core/src/events/mod.rs:260`.
- **`Db`** (`crates/reprise-core/src/db_handle.rs`): `open_migrated/open_in_memory/open_ready` return
  `Result<Self, DbError>`; `conn()` is `pub(crate)`. Neither headless crate reaches a `Connection`.
  `DbError` (`db.rs:27-37`): `Sqlite(#[from] rusqlite::Error)`, `Io`, `SchemaTooNew`, `SchemaNotReady`.
  The CLI maps `SchemaTooNew` to exit 5 and everything else to `CliError::Database(to_string())`
  (exit 6); mcp maps it to `StartupError::SchemaTooNew` (exit 3). **`DbError` does not change.**
- **Core error enums that embed `rusqlite::Error`** (all via a `#[from]` variant; none changes shape):
  `DbError`, `ScanError` (`library/scanner_types.rs:11`: `Db(DbError)` and `Sqlite(rusqlite::Error)`),
  `PromotionError` (`ai_promotion.rs:112`), `SweepError` (`ai_staging.rs:202`), `DoctorError`
  (`library/library_doctor/types.rs:328`), `PipelineError` (`podcasts/pipeline.rs:271`), `CleanupError`
  (`podcasts/downloads.rs:338`), `ConcertError`, `SessionError`, `StartupTaskError`,
  `RhythmboxImportError`, `SyncTargetError`, `DeviceSettingsError`, `QueueError`, `TagWriteJobError`.
- `reprise-gnome` still depends on `rusqlite` directly (`crates/reprise-gnome/Cargo.toml:29`); 47 of its
  production lines mention `rusqlite::Error`; `scripts/check-frontend-thinness.sh` pins a `rusqlite`
  budget of exactly 114 lines matching `rusqlite::|use rusqlite|params!|\.prepare\(|\.query_row\(|Connection`
  in `crates/reprise-gnome/src` production code. `reprise-android-ffi`, `reprise-view`,
  `reprise-platform-linux` and `reprise-stems` declare no `rusqlite`.
- `scripts/check-architecture.sh:388-405` already bans SQL text in `crates/reprise-cli/src` and
  `crates/reprise-mcp/src` and exempts `tests/`; its comment (lines 390-393) says the surfaces "hold a
  rusqlite Connection only to open the migrated database and to read busy/lock error codes".

## Decisions (fixed — do not re-open)

1. **Shape.** `CoreError` is a `#[non_exhaustive]` enum with three variants today — `Busy`, `Conflict`,
   `Storage` — each wrapping a `StorageError` whose field is private, so the SQLite error type is never
   part of a public pattern. Display is **transparent**: `CoreError::from(e).to_string() == e.to_string()`
   for every `rusqlite::Error`. That is what keeps CLI stderr and MCP messages byte-identical. The
   consolidation plan's `NotFound`/`Invalid`/`Backend(String)` are not added now: no converted facade
   produces them, and the slice is additive.
2. **Classification lives in one place.** `From<rusqlite::Error> for CoreError` maps
   `SqliteFailure` with `ErrorCode::DatabaseBusy | DatabaseLocked` to `Busy` (exactly today's
   `rusqlite_is_busy` and `library::stats::is_database_busy`), `ErrorCode::ConstraintViolation` to
   `Conflict` (exactly today's mcp `is_constraint_violation`), everything else to `Storage`.
   `library::stats::is_database_busy` keeps its name and doc and delegates to the same predicate.
3. **`?` keeps working in both directions.** A transitional `impl From<CoreError> for rusqlite::Error`
   unwraps the inner error (lossless for the three variants). This is what keeps the hundreds of
   internal callers — and every `reprise-gnome`/`reprise-android-ffi` caller that propagates with `?` —
   compiling without edits. It is documented as transitional and leaves with the last internal
   `rusqlite::Error` signature in a later wave. Only tail-position returns (`fn f() -> Result<_, rusqlite::Error> { facade() }`)
   need a one-line adapter.
4. **Only Table A converts.** Facades that return a domain enum embedding `rusqlite::Error` (Table B:
   `Db::open_*`, `scan_folder`, `ai_promotion::*`, `StagingStore::sweep_orphans`, `LibraryDoctor::*`,
   `podcasts::pipeline::*`) do not change; cli/mcp never name `rusqlite` through them.
5. **Tests keep `rusqlite` as a dev-dependency.** The integration tests arrange fixtures in SQL that no
   core API can produce (fake-path tracks with fixed ids, forged `user_version`, held write
   transactions, doctor proposal rows). Moving them behind `test-fixtures` is its own task. The gate
   therefore bans `rusqlite` in `[dependencies]` (via `cargo tree -e normal --depth 1`) and as a word
   anywhere under `src/`, and leaves `tests/` alone — the same cut the SQL gate already makes.
6. **No signature changes outside core/cli/mcp.** A GTK, Android or view call site gets at most a
   one-line adapter (`Ok(..?)`); retyping those functions is a later wave.

## Owns

This list is a starting point, not a fence. A file that has to change to keep a call site compiling
may be added. Stop only if the contract itself turns out wrong.

- New: `crates/reprise-core/src/error.rs`, `crates/reprise-core/src/error_tests.rs`
- `crates/reprise-core/src/lib.rs` — the new `pub mod error;` and `pub use error::CoreError;` lines only.
  Strand B replaces the `mod sources_http;` line of the same file; keep your lines away from it.
- `crates/reprise-core/src/library/stats.rs` — `is_database_busy` becomes a delegate
- The Table A facade files: `library/settings_api.rs`, `queries/{surface_browse,library_views,stats,track_view,maintenance,mod}.rs`,
  `library/{playlists_api,playlist_delete}.rs`, `events/mod.rs`, `concerts/{config,query}.rs`,
  `artist_news_history.rs`, `artist_news_refresh.rs`, `ai_jobs.rs`, `ai_jobs/query.rs`,
  `podcasts/{config,store,query,downloads}.rs`, `radio/{config,station}.rs`, `online_sources.rs`,
  `modules.rs`, `library/library_doctor/preferences.rs` (all under `crates/reprise-core/src/`)
- Any core file whose domain error enum needs `impl From<CoreError>` (the fifteen listed above) or
  whose internal caller needs an `Ok(..?)` adapter
- `crates/reprise-cli/Cargo.toml`, `crates/reprise-cli/src/{error,retry}.rs`,
  `crates/reprise-cli/src/commands/{playlist,instrumental,worker}.rs`
- `crates/reprise-mcp/Cargo.toml`, `crates/reprise-mcp/src/{capability,startup,data,doctor_actions}.rs`
- One-line adapters in `reprise-gnome`, `reprise-android-ffi`, `reprise-view`, `reprise-platform-linux`
  where a converted facade is returned in tail position. Known: `crates/reprise-gnome/src/ui/podcasts/add_dialog_subscription.rs:48-54`
  (`subscribe(..) -> Result<i64, rusqlite::Error> { podcasts::store::add_or_restore_with_baseline(..) }`).
  Strand C owns that directory; this strand lands first and C rebases, so the adapter is yours to add.
- `scripts/check-architecture.sh` — the headless block only (lines 388-405 and the new block right
  after it). Strand B edits lines 240-270 of the same file. Touch nothing else in it.
- `scripts/check-frontend-thinness.sh` — the `[rusqlite]=114` number only, and only if the measured
  count changes (see A6).

Not owned: `DbError`, `ScanError` and the other domain enums' variants; the SQL text of any facade;
`reprise-gnome` function signatures; `scripts/check-architecture.sh` outside the headless block.

## The new type — exact shape

`crates/reprise-core/src/error.rs`:

```rust
//! The engine's error type for callers outside the engine. Today it classifies storage failures;
//! facades convert to it at the boundary, internals keep their own error types.

/// A storage failure, with the SQLite error behind it. The field is private on purpose: callers
/// read `Display` or walk `source()`, they never match the backend's type.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct StorageError(#[source] rusqlite::Error);

/// What a core facade hands out when the library store fails.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CoreError {
    /// `SQLITE_BUSY` or `SQLITE_LOCKED`: another writer holds the store; the caller may retry.
    #[error(transparent)]
    Busy(StorageError),
    /// A constraint rejected the write (foreign key, uniqueness, NOT NULL).
    #[error(transparent)]
    Conflict(StorageError),
    /// Every other storage failure.
    #[error(transparent)]
    Storage(StorageError),
}

impl CoreError {
    #[must_use] pub fn is_busy(&self) -> bool;       // matches!(self, Self::Busy(_))
    #[must_use] pub fn is_conflict(&self) -> bool;   // matches!(self, Self::Conflict(_))
}

/// Shared predicate: `SqliteFailure` whose code is `DatabaseBusy` or `DatabaseLocked`.
pub(crate) fn sqlite_error_is_busy(error: &rusqlite::Error) -> bool;
/// Shared predicate: `SqliteFailure` whose code is `ConstraintViolation`.
pub(crate) fn sqlite_error_is_conflict(error: &rusqlite::Error) -> bool;

impl From<rusqlite::Error> for CoreError { /* busy → Busy, conflict → Conflict, else Storage */ }

/// Transitional: lets every internal `Result<_, rusqlite::Error>` and every frontend `?` keep
/// compiling while the internal signatures are still converted. Lossless for all current variants.
/// Delete it together with the last internal `rusqlite::Error` signature.
impl From<CoreError> for rusqlite::Error { /* unwrap the StorageError of any variant */ }
```

`lib.rs` gains `pub mod error;` (alphabetical among the `pub mod` lines) and `pub use error::CoreError;`.
`library::stats::is_database_busy` keeps its signature, name and doc comment and becomes
`crate::error::sqlite_error_is_busy(error)`. The private `is_busy` in `events/mod.rs:260` may do the
same; optional.

`Display` is the SQLite message for all three variants (pinned by test). `Debug` differs from the raw
`rusqlite::Error` debug output — grep cli/mcp for `{:?}`, `{e:?}`, `{error:?}` on error values before
A4/A5 and report any that reach a user or a test assertion.

## Table A — the facades to convert

Every row is `pub fn …(db: &Db, …) -> Result<X, rusqlite::Error>` today and becomes
`-> Result<X, CoreError>`. Bodies do not change beyond what `?` needs. Call sites in cli/mcp are found
by the compiler once the `From<rusqlite::Error>` impls leave those crates (A4/A5); the inventory
behind this table listed them all and found no `&rusqlite::Connection` parameter anywhere.

| Core path | Definition | X |
| --- | --- | --- |
| `library::settings::get_bool` | `library/settings_api.rs:38` | `bool` |
| `library::settings::set_bool` | `library/settings_api.rs:43` | `()` |
| `library::settings::set_setting` | `library/settings_api.rs:33` | `()` |
| `library::settings::get_library_root` | `library/settings_api.rs:48` | `Option<String>` |
| `library::settings::set_library_root` | `library/settings_api.rs:53` | `()` |
| `queries::query_library_metadata_text_search` | `queries/surface_browse.rs:175` | `TrackWindow` |
| `queries::query_all_artists` | `queries/library_views.rs:200` | `Vec<ArtistSummary>` |
| `queries::query_all_albums` | `queries/library_views.rs:192` | `Vec<AlbumSummary>` |
| `queries::query_artist_count` | `queries/library_views.rs:411` | `i64` |
| `queries::query_album_count` | `queries/library_views.rs:387` | `i64` |
| `queries::query_library_stats` | `queries/stats.rs:30` | `LibraryStats` |
| `queries::query_track_window` | `queries/track_view.rs:101` | `Vec<Track>` |
| `queries::query_track_count` | `queries/track_view.rs:112` | `i64` |
| `queries::query_track_summary` | `queries/maintenance.rs:43` | `Option<TrackSummary>` |
| `queries::filter_present` | `queries/maintenance.rs:108` | `Vec<i64>` |
| `queries::track_source_path` | `queries/mod.rs:227` | `Option<PathBuf>` |
| `library::playlists::list` | `library/playlists_api.rs:19` | `Vec<PlaylistSummary>` |
| `library::playlists::get` | `library/playlists_api.rs:24` | `Option<PlaylistSummary>` |
| `library::playlists::create` | `library/playlists_api.rs:9` | `i64` |
| `library::playlists::create_with_tracks` | `library/playlists_api.rs:39` | `i64` |
| `library::playlists::rename` | `library/playlists_api.rs:14` | `usize` |
| `library::playlists::delete` | `library/playlist_delete.rs:14` (re-exported by `library/playlists.rs:13`) | `bool` |
| `library::playlists::add_tracks` | `library/playlists_api.rs:34` | `u32` |
| `library::playlists::track_ids` | `library/playlists_api.rs:29` | `Vec<i64>` |
| `events::read_since` | `events/mod.rs:125` | `Vec<Change>` |
| `concerts::config::credentials` | `concerts/config.rs:56` | `Credentials` |
| `concerts::config::location` | `concerts/config.rs:83` | `Option<AppLocation>` |
| `concerts::config::window_days` | `concerts/config.rs:90` | `i64` |
| `concerts::config::persisted_filter` | `concerts/config.rs:101` | `ConcertFilter` |
| `concerts::config::similar_config` | `concerts/config.rs:126` | `SimilarConfig` |
| `concerts::query_cached_events` | `concerts/query.rs:46` | `Vec<CachedConcertEvent>` |
| `concerts::query_events` | `concerts/query.rs:86` | `Vec<ConcertRow>` |
| `concerts::latest_fetch_at` | `concerts/query.rs:184` | `Option<i64>` |
| `artist_news_history::query_complete_history` | `artist_news_history.rs:137` | `Vec<ReleaseHistoryRecord>` |
| `artist_news::latest_fetched_at` | `artist_news_refresh.rs:54` (re-exported by `artist_news.rs:97-98`) | `Option<i64>` |
| `ai_jobs::enqueue_instrumental_batch` | `ai_jobs.rs:248` | `BatchOutcome` |
| `ai_jobs::claim_next` | `ai_jobs.rs:382` | `Option<ClaimedJob>` |
| `ai_jobs::heartbeat` | `ai_jobs.rs:448` | `HeartbeatOutcome` |
| `ai_jobs::set_progress` | `ai_jobs.rs:480` | `bool` |
| `ai_jobs::mark_failed` | `ai_jobs.rs:511` | `bool` |
| `ai_jobs::mark_cancelled` | `ai_jobs.rs:533` | `bool` |
| `ai_jobs::request_cancel` | `ai_jobs.rs:594` | `CancelOutcome` |
| `ai_jobs::discard_staged` | `ai_jobs.rs:676` | `bool` |
| `ai_jobs::get_job` | `ai_jobs/query.rs:10` (re-exported by `ai_jobs.rs:45`) | `Option<AiJob>` |
| `ai_jobs::list_jobs_in_batch` | `ai_jobs/query.rs:25` | `Vec<AiJob>` |
| `ai_jobs::list_active_jobs` | `ai_jobs/query.rs:39` | `Vec<AiJob>` |
| `ai_jobs::batch_progress` | `ai_jobs/query.rs:66` | `BatchProgress` |
| `podcasts::config::load` | `podcasts/config.rs:175` | `PodcastConfig` |
| `podcasts::config::source_network_allowed` | `podcasts/config.rs:311` | `bool` |
| `radio::config::load` | `radio/config.rs:27` | `RadioConfig` |
| `podcasts::store::subscription` | `podcasts/store.rs:123` | `Option<SubscriptionRow>` |
| `podcasts::store::episode` | `podcasts/store.rs:337` | `Option<EpisodeRow>` |
| `podcasts::store::add_or_restore` | `podcasts/store.rs:43` | `i64` |
| `podcasts::store::add_or_restore_with_baseline` | `podcasts/store.rs:52` | `i64` |
| `podcasts::store::upsert_episode` | `podcasts/store.rs:267` | `Option<UpsertResult>` |
| `podcasts::store::tombstone_episode` | `podcasts/store.rs:511` | `bool` |
| `podcasts::store::commit_remove_episode` | `podcasts/store.rs:531` | `Option<String>` |
| `podcasts::store::tombstone_subscription` | `podcasts/store.rs:565` | `()` |
| `podcasts::store::commit_remove_subscription` | `podcasts/store.rs:583` | `()` |
| `podcasts::store::update_subscription_details` | `podcasts/store.rs:169` | `bool` |
| `podcasts::store::update_fetch_success` | `podcasts/store.rs:360` | `()` |
| `podcasts::store::active_subscriptions` | `podcasts/store.rs:103` | `Vec<SubscriptionRow>` |
| `podcasts::query::list_episodes` | `podcasts/query.rs:19` | `Vec<EpisodeRow>` |
| `podcasts::query::episodes_for_subscription` | `podcasts/query.rs:37` | `Vec<EpisodeRow>` |
| `podcasts::downloads::set_downloaded_file` | `podcasts/downloads.rs:45` | `()` |
| `radio::station::list` | `radio/station.rs:77` | `Vec<StationRow>` |
| `radio::station::get` | `radio/station.rs:92` | `Option<StationRow>` |
| `radio::station::add_or_restore` | `radio/station.rs:21` | `i64` |
| `radio::station::update` | `radio/station.rs:177` | `bool` |
| `radio::station::tombstone` | `radio/station.rs:115` | `bool` |
| `radio::station::commit_remove` | `radio/station.rs:133` | `bool` |
| `online_sources::is_enabled` | `online_sources.rs:40` | `bool` |
| `online_sources::set_enabled` | `online_sources.rs:45` | `()` |
| `online_sources::network_allowed` | `online_sources.rs:166` | `bool` |
| `modules::is_enabled` | `modules.rs:202` | `bool` |
| `modules::set_enabled` | `modules.rs:214` | `()` |
| `library_doctor::remote_suggestion_preference` | `library/library_doctor/preferences.rs:63` | `RemoteSuggestionPreference` |

If the compiler reveals a facade cli/mcp call that is missing here, convert it too and list it. If a
listed facade turns out not to be called by cli/mcp, convert it anyway (the table was measured against
`src/` and `tests/` of both crates) and say so.

**Table B — called, embeds `rusqlite::Error`, does not change:** `Db::{open_migrated,open_in_memory,open_ready}`
(`DbError`), `library::scanner::scan_folder` (`ScanError`), `ai_promotion::{promote,complete_render_with_publish}`
(`PromotionError`), `ai_staging::StagingStore::sweep_orphans` (`SweepError`),
`LibraryDoctor::{scan,last_complete_scan,apply_auto_tier,apply_review_plan,revert_last_cleanup,finalize_incomplete_writes}`
(`DoctorError`), `podcasts::pipeline::{download_episode,refresh_to_root}` (`PipelineError`).

## Tasks (in order, one commit each)

**A1 — the classification tests first (red: `crate::error` does not exist yet).** Create
`crates/reprise-core/src/error_tests.rs`, declared from `error.rs` the way the neighbouring sibling
test files are declared (look at how `db_tests.rs` is wired from `db.rs`; follow that form). Tests:

- `busy_and_locked_fold_into_busy`: `SqliteFailure(ffi::Error::new(SQLITE_BUSY), None)` and
  `(SQLITE_LOCKED)` convert to a `CoreError` with `is_busy()`; `is_conflict()` is false. (This is the
  moved CLI test `rusqlite_busy_and_locked_are_retryable`.)
- `constraint_violation_folds_into_conflict`: `SQLITE_CONSTRAINT` converts to `is_conflict()`, not busy.
- `read_only_and_no_rows_fold_into_storage`: `SQLITE_READONLY` and `rusqlite::Error::QueryReturnedNoRows`
  are neither busy nor conflict. (Moved CLI test `rusqlite_other_failures_are_not_retryable`.)
- `display_is_the_sqlite_message`: for each error above, `CoreError::from(e.clone_like()).to_string() == e.to_string()`
  — `rusqlite::Error` is not `Clone`, so construct each error twice.
- `source_is_the_sqlite_error`: `std::error::Error::source(&core_error)` is `Some`, and its
  `to_string()` equals the SQLite message.
- `round_trip_through_the_backend_type_is_lossless`: `rusqlite::Error::from(CoreError::from(e)).to_string() == e.to_string()`
  and the busy classification is identical before and after.
- `is_database_busy_and_core_error_agree`: for the five constructed errors,
  `library::stats::is_database_busy(&e) == CoreError::from(e).is_busy()`.
- `core_error_is_send_and_sync`: `fn assert<T: Send + Sync>() {} assert::<CoreError>();` (mcp moves
  `DataError` across `tokio::task::spawn_blocking`).

**A2 — the type.** Write `error.rs` as specified. Add the `lib.rs` lines. Make
`library::stats::is_database_busy` delegate. `cargo test -p reprise-core error_` goes green.

**A3 — convert Table A.** Work file by file; one commit per module group (settings + queries;
playlists + events; concerts + artist news; ai_jobs; podcasts + radio; online_sources + modules +
doctor preferences). In each file:

- Change the return type of every listed facade to `Result<X, CoreError>`.
- Add the import. Prefer folding it into an existing `use crate::…` line (`use crate::{db::Db, CoreError};`)
  so the file does not grow; four Table A files sit within 80 lines of the cap
  (`podcasts/store.rs` 738, `queries/maintenance.rs` 740, `queries/library_views.rs` 722, `ai_jobs.rs` 711).
- The body stays. An internal call ending the function in tail position (`internal_fn(db, …)`)
  becomes `Ok(internal_fn(db, …)?)`; a `?` inside the body already converts via `From<rusqlite::Error>`.
  Do not restructure anything else.
- Then `cargo check -p reprise-core --all-targets`. Internal callers that break are of two kinds:
  - a function returning `Result<_, rusqlite::Error>` that returns the converted facade in tail
    position: `Ok(facade(..)?)`;
  - a function returning a domain enum (`ScanError`, `DoctorError`, …) that calls the converted facade
    with `?`: add `impl From<CoreError> for ThatEnum { fn from(error: CoreError) -> Self { Self::<its rusqlite variant>(error.into()) } }`
    next to the enum. Add these only where the compiler asks; list every one in the final message.
- Do not change any `_in(conn, …)`/`_conn` internal helper; they stay on `rusqlite::Error`.

**A4 — `reprise-cli`.** Apply the evidence table rows for cli. `retry_write`/`retrying` take
`impl FnMut() -> Result<T, CoreError>`; every `with_retry(op, rusqlite_is_busy)` becomes
`with_retry(op, CoreError::is_busy)`; `scan_is_busy` uses `reprise_core::library::stats::is_database_busy`
in both arms. Delete the two predicate tests (moved in A1); keep every other test in `retry.rs`. If
`retry.rs` is left with no test of `with_retry` itself, add `with_retry_stops_at_the_first_non_busy_error`
using a local two-variant stub enum and a closure predicate — no `CoreError` needs to be constructed
outside core. Reword the two doc comments at `retry.rs:63,75` (and any other comment under `src/`)
so the word `rusqlite` no longer appears: the gate greps the bare word. Remove line 48 from
`Cargo.toml` and rewrite the comment above it: core's `CoreError` is the facade error type now;
`rusqlite` remains a dev-dependency because the integration tests arrange fixtures in SQL. Then
`cargo check -p reprise-cli --all-targets --all-features` must pass with no `rusqlite` in `src/`.

**A5 — `reprise-mcp`.** Apply the evidence table rows for mcp. `map_create_error` keeps the exact
`InvalidInput` text. `doctor_actions.rs:525` becomes `DataError::Db(error.into())`. Remove line 28 from
`Cargo.toml` and rewrite the comment at 23-26 (the dev-dependency comment at 64-66 stays true). Then
`cargo check -p reprise-mcp --all-targets --all-features` passes with no `rusqlite` in `src/`.

**A6 — the other crates.** `cargo check --workspace --all-targets --all-features`. Every remaining
error is a tail-position return of a converted facade in `reprise-gnome`, `reprise-android-ffi`,
`reprise-view` or `reprise-platform-linux`. Fix each with `Ok(<call>?)` on that line; if clippy's
`needless_question_mark` objects (it should not, the error types differ), use `.map_err(Into::into)`.
Never change a signature there. Known site: `crates/reprise-gnome/src/ui/podcasts/add_dialog_subscription.rs:48-54`.
Then run `scripts/check-frontend-thinness.sh`: the `rusqlite` count is expected to stay at 114 (your
adapters add no matching token). If it reports "down to N", lower `[rusqlite]=114` to `N` in the same
commit and name the lines that changed in the final message. If it reports growth, you added a
`rusqlite::` mention in gnome — remove it.

**A7 — the gate.** In `scripts/check-architecture.sh`, headless block only:

- Rewrite the comment at lines 390-393: the surfaces no longer name `rusqlite`; core's `CoreError` is
  what the facades hand out, busy/conflict classification happens in core; test fixtures under `tests/`
  may still use SQL and `rusqlite` directly.
- After the existing loop (line 405) add:

```bash
# The headless surfaces never name rusqlite: reprise_core::CoreError is the error type the facades
# hand out, and busy/conflict classification happens in core. `rusqlite` stays a dev-dependency only,
# because the integration tests under tests/ arrange their fixtures in SQL.
for surface in reprise-cli reprise-mcp; do
  direct_deps=$(run_dependency_probe "$surface direct dependencies" \
    -p "$surface" --all-features -e normal --depth 1 --prefix none --target all) || exit 1
  if printf '%s\n' "$direct_deps" | rg --quiet '^rusqlite '; then
    echo "$surface must not depend on rusqlite; return reprise_core::CoreError from the core facade instead" >&2
    exit 1
  fi
  if rg --quiet -w 'rusqlite' "crates/$surface/src" --glob '*.rs'; then
    echo "$surface sources must not name rusqlite; the core facades return CoreError" >&2
    rg -n -w 'rusqlite' "crates/$surface/src" --glob '*.rs' >&2
    exit 1
  fi
done
```

- Keep `scripts/check-shell.sh` (shellcheck) green. `scripts/tests/qa-linters.sh:214-223` pins other
  patterns in this script, none in this block; run it anyway. **Never cite a `docs/plans/…` path from
  the script or from code**: the "Documentation references from code" gate fails once the plan is
  deleted on landing, and wave plans are deleted on landing.
- Prove the gate in both directions once, locally: temporarily add `use rusqlite::Connection;` to a cli
  source file, run the script, see it fail with the second message, revert.

## Known traps

- **Display must stay the SQLite text.** `cli/src/main.rs` prints `CliError::Database(String)`,
  `mcp/src/main.rs:74` prints `StartupError::Query(error)` with `{error}`; the CLI integration tests
  compare stderr. `#[error(transparent)]` over `StorageError`'s `#[error("{0}")]` is what guarantees
  identity — do not add prefixes like "database error:".
- **`#[non_exhaustive]`** forces a wildcard arm in any `match` on `CoreError` outside core. Inside cli/mcp
  use `is_busy()`/`is_conflict()`, never a `match`.
- **Both `From` impls are required.** Deleting the transitional `From<CoreError> for rusqlite::Error`
  turns hundreds of internal `?` sites red; it is not this strand's job.
- **`?` into a domain enum.** `ScanError`, `DoctorError`, `PipelineError` etc. have `From<rusqlite::Error>`
  but not `From<CoreError>`; the compiler tells you which ones need the impl (A3).
- **Features.** `worker.rs` is `cfg(feature = "worker")`, three `capability.rs` functions are
  `cfg(feature = "mpris")`, `data_tests.rs` is `cfg(all(test, feature = "mpris"))`. Only
  `cargo clippy --all-targets --workspace --all-features -- -D warnings` compiles them all. If the
  `worker` feature cannot build in your sandbox (it pulls `ort`), run `-p reprise-cli --features mpris`
  and `--features worker` separately and report the exact failure instead of skipping it.
- **CI clippy is 1.99, local is 1.97.** Do not delete an existing `#[expect]`/`#[allow]` in a touched
  file as "unfulfilled" without checking that it is unfulfilled under both. Every new suppression
  needs `reason = "…"` (`allow_attributes_without_reason` is a workspace lint).
- **`significant_drop_in_scrutinee`** is on; irrelevant here unless you touch a lock — you should not.
- **`busy_retry` integration test** (`crates/reprise-cli/tests/busy_retry.rs`) is the net for the busy
  classification and is known to flake under CI load (exit 6 instead of 0). A local failure means a
  control arm on `dev` first, then compare.
- **mcp robustness test** (`crates/reprise-mcp/tests/robustness.rs:105-126`) holds `BEGIN IMMEDIATE`
  through a raw dev-dependency connection and expects an error, not a hang. It keeps using `rusqlite`;
  that is allowed under `tests/`.
- **File sizes.** Every touched file ends below 800 lines; the near-cap files are listed in A3. mcp
  `source_actions.rs` is 776 lines and should need no edit (`.map_err(DataError::Db)` is unchanged).
  `window.rs` is not touched.
- **Thinness scanner.** `scripts/check-frontend-thinness.sh` counts gnome production lines; `Ok(..?)`
  adapters are neutral; a closure annotated `|e: rusqlite::Error|` that you retype lowers the count —
  then lower the budget (A6).
- **English everywhere**, focused commits, no agent attribution lines.

## Verification

Run from the worktree root, in this order; every command must pass:

```
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo clippy --all-targets --workspace --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo test -p reprise-core
cargo test -p reprise-cli -p reprise-mcp
cargo test -p reprise-cli --features mpris        # the mpris-gated code paths
cargo test -p reprise-mcp --features mpris
cargo test -p reprise-android-ffi                 # only if A6 touched it
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # must print nothing
cargo tree -p reprise-cli -e normal --depth 1 --prefix none | grep '^rusqlite'   # must print nothing
cargo tree -p reprise-mcp -e normal --depth 1 --prefix none | grep '^rusqlite'   # must print nothing
rg -n -w 'rusqlite' crates/reprise-cli/src crates/reprise-mcp/src                # must print nothing
scripts/check-architecture.sh
scripts/check-frontend-thinness.sh
scripts/check-shell.sh
scripts/tests/qa-linters.sh
```

For `reprise-gnome` run filtered tests only, for the modules whose files A6 touched (for example
`cargo test -p reprise-gnome add_dialog_subscription`); never the unfiltered suite. The orchestrator
runs `scripts/check-merge-readiness.sh` after the code phase. Report: the number of converted
facades, every `impl From<CoreError>` added, every adapter outside core/cli/mcp with file:line, and
the thinness count.
