---
slug: file-open-mini-player
worktree: /home/marvin/Projects/reprise-file-open-mini-player
branch: feature/file-open-mini-player
phase: shipped
codex_session:
created: 2026-08-05
---
# Mini player on a file-open cold start

When Reprise is started *by* opening an audio file through the desktop file
association, and at least one of those files resolves to a track that is already
in the library, the window opens as the mini player (Compact Mode) instead of
the full library.

## Behaviour contract

The mini player opens only when **all four** hold:

1. Reprise was not already running (cold start — the process that builds the
   first window).
2. The process was started through `open`, not `activate`.
3. At least one opened file resolves to a library track.
4. No playlist file (`.m3u` / `.m3u8`) is part of the same request.

Everything else keeps today's behaviour:

| Case | Result |
|---|---|
| Nothing resolves to a library track | Library, today's toasts unchanged |
| A playlist file is in the request (even mixed with audio) | Library — import feedback and the sidebar belong there |
| Reprise is already running | Unchanged. The forwarded `open` never switches mode |
| Very first launch (first-run wizard) | Library. The wizard wins over the intent |
| Persisted mode is already Compact | Nothing special, it stays Compact |
| Playback backend unavailable (no `PlayerController`) | Library — the intent cannot be honoured without a player |

**The mode is not persisted.** `ui.window_view_mode` must be left untouched by
this path: a double-click in the file manager must never silently rewrite the
user's start preference. `MinimalView::apply_initial` already only *applies* the
transition (persistence lives in the toggle path around
`minimal_view.rs:209`) — keep it that way and pin it with a test.

Pressing Ctrl+M in such a session switches back to the library and persists that
choice exactly like any other deliberate toggle. No new behaviour needed.

## Why the decision is made before the window is built

`ui::window::build` presents the window itself (`window.rs:614`) and returns the
`FileOpenHandler`; `main.rs`'s `connect_open` only calls `handler.open(files)`
afterwards. Switching the mode there would show the full library window first and
then collapse it into the mini player — visible flicker on the very first
impression. So the file paths are resolved against the database *before*
`ensure_window` runs (the `Rc<Db>` already exists at that point) and the outcome
is passed into `build` as an explicit intent.

## Tasks

### Task 1 — a window-free open-request resolution

In `crates/reprise-gnome/src/ui/file_open.rs`:

- Add `pub(crate) struct OpenRequest { audio_ids: Vec<i64>, unresolved: Vec<PathBuf>, playlists: Vec<PathBuf>, unsupported: usize }`.
- Add `pub(crate) fn resolve_open_request(db: &Db, files: &[gio::File]) -> OpenRequest`.
  It performs exactly the classification and resolution that `FileOpenHandler::open`
  does today (the `file.path()` / `is_file()` / `classify_path` loop plus
  `resolve_audio_ids`) and touches no widgets. It must keep the existing
  guarantee that opening never inserts library rows.
- Add `pub(crate) enum StartupOpenIntent { Library, CompactPlayback }` with
  `Library` as the `Default`, and a method on `OpenRequest`:

  ```rust
  pub(crate) fn startup_intent(&self) -> StartupOpenIntent
  ```

  which returns `CompactPlayback` only when `!audio_ids.is_empty() &&
  playlists.is_empty()`, otherwise `Library`.

Tests (pure, no display): a single resolvable audio file yields
`CompactPlayback`; an audio file mixed with an `.m3u` yields `Library`; only
unresolvable audio yields `Library`; a playlist alone yields `Library`.

### Task 2 — `FileOpenHandler::open` consumes the resolved request

Rework `FileOpenHandler::open` so it takes the already-resolved `OpenRequest`
instead of resolving a second time — the same decision must not be made twice in
two places, or the two copies will drift. Keep a thin `open(&self, files:
&[gio::File])` entry point for the `activate`-side/any other caller if one is
needed, implemented as `self.open_request(resolve_open_request(&self.conn,
files))`. The playlist import, playback start and all three toasts stay exactly
as they are today.

### Task 3 — thread the intent through to the mode decision

- `main.rs`: in `connect_open`, resolve the request first, derive the intent,
  and hand it to `ensure_window(...)` → `ui::window::build(app, conn, db_path,
  intent)`. Then call the handler with the already-resolved request. When
  `ensure_window` finds an existing window (the app was already running), the
  intent is dropped on the floor — this is the "already running" row of the
  contract. `connect_activate` passes `StartupOpenIntent::Library`.
- `ui::window::build` gains the intent parameter.
- `compact_mode_controls::initial_transition(db, first_run, intent)` and
  `minimal_view::startup_transition(persisted_mode, persisted_layout, first_run,
  intent)` gain it too. Precedence, in this order:

  ```
  first-run wizard  >  file-open intent  >  persisted mode
  ```

Table-driven unit tests for `startup_transition` covering that precedence:
wizard + `CompactPlayback` → Library; `CompactPlayback` + persisted Library →
Compact; `Library` intent + persisted Compact → Compact; `Library` intent +
persisted Library → Library. `layout` passes through untouched in every case.

### Task 4 — no player, no mini player

`initial_view` is computed at `window.rs:84` but its only consumer is
`build_mode` at `window.rs:507`, while `player` is built at `window.rs:168`.
Move the `initial_transition` call below the player construction and degrade the
intent to `Library` when `player.is_none()`, so a broken playback backend can
never strand the user in the smallest possible window. This happens before
`window.present()`, so nothing flickers.

Do **not** apply that degradation to a *persisted* Compact mode — only to the
intent — so this task changes no existing behaviour.

### Task 5 — the UX rule

Add **MINI-6** to section AD of `docs/ux-rules.md`, in the voice and format of
the surrounding MINI-1..5 rules: a song opened through the desktop file
association that starts Reprise opens the mini player; a request with nothing
playable, or with a playlist in it, opens the library; the mode is not
persisted; an already-running Reprise never switches mode. Mark it `[active]
[gtk]`.

## Verification

- `cargo test -p reprise-gnome` and `cargo test -p reprise-core` for the pure
  logic (this is where the value of this change is proven).
- `cargo clippy --workspace --all-targets` clean, `cargo fmt --check` clean.
- Display-backed tests cannot run in this sandbox — that is expected, not a
  failure. Likewise, tests that write to `~/.cache` may fail with
  `Os { code: 30, kind: ReadOnlyFilesystem }` under the sandbox; both classes are
  pre-existing environment limits and must not be "fixed" by changing test code.
  Every *other* failure is a real failure and must be resolved.

## Out of scope (deliberately)

No preference toggle, no offer to import unknown files, no change to behaviour
while the app is already running, no change to the compact layout itself.
