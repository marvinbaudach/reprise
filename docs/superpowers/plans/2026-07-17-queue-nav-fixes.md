# Queue & Navigation Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Returning to a view finds it exactly as left (NAV-5), cover/title in the player bar jump to the playing track's home (NAV-9, Ctrl+L, back-stack), and the Queue view shows the full playback truth: Now Playing + Play Next + Up Next · from <source> (QUE-1..5).

**Architecture:** Three independent mechanisms share two new pieces of state. (1) A session-scoped `ViewSource → SavedViewState` map inside `TrackList::Shared`, saved/restored at the single `set_source_and_reload` choke point. (2) A playback *origin* (`ViewSource` + resolved label) threaded through `play_from_view` and stored beside the hidden `Queue` snapshot; it powers both the Queue view's "Up Next · from X" section and NAV-9's jump target. (3) The Queue view stops rendering `up_next` only and instead renders a composite id list (current + play-next + snapshot-rest) with `gtk::SectionModel` sections on the existing windowed `TrackListModel`; every queue-row interaction remaps view positions to section-local operations.

**Tech Stack:** Rust, gtk4-rs 0.11 (GTK 4.22: `ColumnView::set_header_factory` + `SectionModelImpl`), libadwaita, rusqlite. No new dependencies.

## Global Constraints

- Gates before EVERY commit: `cargo fmt --check` · `cargo clippy --all-targets --workspace -- -D warnings` · `cargo test --workspace` · `cargo audit` (only accepted advisory RUSTSEC-2024-0436).
- reprise-core stays dependency-pure (no gtk4/gstreamer/zbus) — verify with `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` after core changes.
- Files < 800 lines; extract sibling modules instead of trimming docs.
- RefCell discipline: never hold a `borrow()` across a GTK/callback call (see building-gtk4-rust-apps skill).
- All user-visible copy via `strings.rs` `N_!` constants.
- One commit per task, no attribution footer, no push.
- ux-rules.md status flips ([geplant] → [aktiv]) happen IN the implementation commit of the rule.

---

### Task 1: UX rulebook `docs/ux-rules.md`

**Files:**
- Create: `docs/ux-rules.md`

New rules NAV-9 / QUE-1..QUE-5 verbatim from the task, status `[geplant]`. Referenced pre-existing rules reconstructed compactly (NAV-2 history stack `[geplant]` — nothing exists today; NAV-3 artist click `[aktiv]`; NAV-5 view-state preservation `[geplant]` — currently violated; PLAY-1/2/3 snapshot semantics `[aktiv]` — implemented by `play_from_view`/`queue_ids_for_activation`; FB-5 empty states `[aktiv]`). Header notes the file is the canonical UX contract and that acceptance tests reference rule ids.

- [ ] Write the file; commit `docs: add UX rulebook (NAV/PLAY/QUE/FB) with planned queue+nav rules`

### Task 2: NAV-5 — session view-state memory

**Files:**
- Modify: `crates/reprise-core/src/view_source.rs:18` — add `Hash` to derives.
- Create: `crates/reprise-gnome/src/ui/track_list/view_state_memory.rs`
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list.rs` (Shared field), `track_list_builder.rs` (init), `track_list_reload.rs::set_source_and_reload` (save+restore), `mod.rs` (module decl)

**Interfaces:**
- Produces: `pub(in crate::ui) struct SavedViewState { pub scroll_value: f64, pub selected_ids: Vec<i64> }`; `Shared.view_state_memory: RefCell<HashMap<ViewSource, SavedViewState>>`; free fns `capture(column_view, selection, model) -> SavedViewState`, `positions_to_select(saved: &SavedViewState, current_ids: &[i64]) -> Vec<u32>`, `clamped_scroll(saved: f64, upper: f64, page: f64) -> f64`.

- [ ] **Failing tests** (in `view_state_memory.rs` tests mod): `positions_to_select_maps_surviving_ids_only` (saved ids `[7,9,11]`, current `[9,42,7]` → `[0, 2]`), `clamped_scroll_clamps_to_content` (saved 900.0, upper 500, page 200 → 300.0; saved 100 → 100), `clamped_scroll_zero_when_list_fits` (upper 100, page 200 → 0.0).
- [ ] Run → red. Implement pure fns → green.
- [ ] Wire in `set_source_and_reload`: before `*shared.source.borrow_mut() = source`, capture old source state into the map (skip when old == new). After `reload(shared)`, look up new source: select positions immediately (`selection.unselect_all()` then `select_item(pos, false)` per position), scroll via `glib::idle_add_local_once` reading the vadjustment and setting the clamped value. Plain `reload()` (filter/sort/browse) does NOT save/restore — source switches only.
- [ ] Gates. Commit `feat: preserve per-source scroll and selection within a session (NAV-5)` — flip NAV-5 → `[aktiv]` in the same commit.

### Task 3: Playback origin threading

**Files:**
- Modify: `crates/reprise-core/src/queue/snapshot.rs` — `QueueSnapshot` gains `#[serde(default)] pub origin: Option<String>` (opaque serialized ViewSource key) — actually store as two fields: `origin_kind: Option<String>`, `origin_label: Option<String>`, serde-defaulted for backward compat.
- Modify: `crates/reprise-core/src/library/session.rs` — `SessionState` passthrough (snapshot already embedded; `normalize` keeps unknown origins as None).
- Modify: `crates/reprise-gnome/src/ui/playback/player_controller.rs` — `play_from_view(&self, ids, start_index, origin: PlayOrigin)`; new field `play_origin: RefCell<Option<PlayOrigin>>`.
- Create: `crates/reprise-gnome/src/ui/playback/play_origin.rs` — `pub(in crate::ui) struct PlayOrigin { pub source: ViewSource, pub label: String }` + `resolve_label(conn, &ViewSource) -> String` (Library→strings::SIDEBAR_MUSIC text "Music", Playlist/Smart→name via `playlists` queries with id-gone fallback to "Music"/Library, Album→album, Artist→name, Missing/ImportErrors→their sidebar labels, Queue→unreachable, falls back to Library) + `origin_from_session(kind, label) -> Option<PlayOrigin>` / `session_fields(&PlayOrigin)` round-trip helpers.
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list.rs:99` — `pub type OnActivate = Box<dyn Fn(&Track, Vec<i64>, usize, ViewSource)>`; `track_list_activation.rs::activate_track` passes `shared.source.borrow().clone()`.
- Modify all `play_from_view` callers: `window/window.rs:226` (pass activation source), `window/window_action_wiring.rs:126,143,157,220,235` (album hero → `ViewSource::Album{..}`, artist hero → `ViewSource::Artist(..)`, shuffle-all/play-all → `ViewSource::Library`), plus session restore (`session_restore.rs`) restoring `play_origin` from snapshot fields.

**Interfaces produced:** `PlayerController::play_origin() -> Option<PlayOrigin>` (clone-out), `play_from_view(ids, start_index, origin: PlayOrigin)`.

- [ ] **Failing tests:** core round-trip `queue_snapshot_origin_roundtrips_and_defaults_absent` (serde json without fields → None); gnome `resolve_label` unit tests with in-memory DB (playlist name, deleted playlist falls back), `origin_from_session` round-trip.
- [ ] Red → implement → green. Gates (incl. core purity grep). Commit `feat: track playback origin (source + label) through play_from_view and session`.

### Task 4: QUE-1/2/4/5 — composite Queue view with sections

**Files:**
- Modify: `crates/reprise-core/src/queue.rs` — `pub fn remaining_after_current(&self) -> Vec<i64>` (`order[pos+1..] → ids`; empty when pos None/at end) + `pub fn remaining_len(&self) -> usize`.
- Modify: `crates/reprise-gnome/src/ui/playback/queue_transport.rs` — `pub(in crate::ui) fn queue_view_sections(&self) -> QueueViewSections` where `pub struct QueueViewSections { pub now_playing: Option<i64>, pub play_next: Vec<i64>, pub up_next_rest: Vec<i64>, pub origin_label: Option<String> }` (now_playing = `current_track` id; play_next = `up_next.ids()`; up_next_rest = `queue.remaining_after_current()`; label from `play_origin`). `pub fn queue_pending_len(&self) -> usize` = play_next + up_next_rest lengths. Fire `notify_queue_changed` additionally from `play_from_view`, advance paths (unconditional, not only on up_next shrink), and the current-track-changed wiring reloads the queue view (`window.rs` `set_on_current_track_changed` closure already exists in `current_track_selection::wire` — add `track_list.reload_queue_if_visible()` + sidebar refresh there via existing on-queue-changed callback instead: simplest is calling the stored on_queue_changed hook from `notify_current_track_changed` path in the controller).
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list.rs` — `queue_ids_provider` type becomes `Box<dyn Fn() -> QueueViewModel>` with `pub struct QueueViewModel { pub ids: Vec<i64>, pub sections: Vec<QueueSection> }`, `pub struct QueueSection { pub start: u32, pub len: u32, pub kind: QueueSectionKind }`, `enum QueueSectionKind { NowPlaying, PlayNext, UpNext { source_label: String } }`; Shared gains `queue_sections: RefCell<Vec<QueueSection>>`.
- Modify: `track_list_model.rs` — implement `gtk::SectionModel` on the subclass (`type Interfaces = (gio::ListModel, gtk4::SectionModel)`; `impl SectionModelImpl { fn section(&self, position) -> (u32, u32) }` reading a `RefCell<Vec<(u32,u32)>>` set via `set_sections(&self, ranges)`, full-range fallback when empty; call `sections_changed(0, n_items)` after query swaps).
- Create: `crates/reprise-gnome/src/ui/track_list/queue_sections.rs` — pure builders: `compose(now_playing: Option<i64>, play_next: &[i64], up_next_rest: &[i64], origin_label: Option<&str>) -> QueueViewModel` and `section_ranges(model: &QueueViewModel) -> Vec<(u32,u32)>`; header factory `wire_queue_header_factory(column_view, shared)` rendering `gtk::ListHeader` titles: "Now Playing" / "Play Next" / "Up Next · from {label}" (all `N_!` strings, the last via freemarker-style `{}` replace like existing count strings); factory active only for Queue source (`set_header_factory(None)` otherwise — mirror `artist_master.rs:347-356`).
- Modify: `track_list_reload.rs::reload` — Queue branch consumes `QueueViewModel`, stores sections in Shared + model, keeps windowed `query_track_window_queue` on the composite ids.
- Modify: `track_list_columns.rs::empty_state_for` — `Queue` → new `EmptyState::EmptyQueue`; strings `EMPTY_QUEUE_TITLE = N_!("Nothing queued")`, `EMPTY_QUEUE_DESCRIPTION = N_!("Play something")` (FB-5: one next step). Existing test at :780-795 updated.
- Modify: `window.rs:202-207` — `queue_len_provider` → `controller.queue_pending_len()`.

**QUE-2 guarantee:** no playback-logic change — `next_target` already pops `up_next` FIFO then walks the snapshot; the composite view mirrors exactly that order. Add a core test pinning it.

- [ ] **Failing tests:** core `remaining_after_current_returns_shuffle_order_tail` (seed 5 ids, jump pos, incl. shuffled case via `set_shuffle` determinism — assert it equals `ids_in_order()[pos+1..]`), `remaining_after_current_empty_at_end_or_unseeded`; gnome `queue_sections::compose` cases: full three sections, no play_next (2 sections), nothing playing but pending play_next (QUE-1 says sections exist only while playing — when now_playing None and lists empty → empty model; when None but play_next non-empty → PlayNext + UpNext sections still shown), ranges math; `next_target` order pin test in `up_next_transport.rs` tests.
- [ ] Red → implement → green. Headless smoke: activate track → switch source queue (REPRISE_SMOKE_SOURCE=queue) → log shows composite count = 1 + rest.
- [ ] Gates. Commit `feat: queue view shows now playing, play next, and up next from playback origin (QUE-1/QUE-2/QUE-4/QUE-5)` — flip those four rules → `[aktiv]`.

### Task 5: QUE-3 — queue interactions on the composite view + "Play next" action

**Files:**
- Modify: `crates/reprise-core/src/up_next.rs` — `pub fn insert(&mut self, index: usize, id: i64)` (clamped), `pub fn clear(&mut self)`, `pub fn prepend(&mut self, ids: &[i64])` (cap-aware).
- Modify: `crates/reprise-core/src/queue.rs` — `pub fn remove_order_positions(&mut self, positions: &[usize]) -> bool` (single-occurrence removal by order index; advance-forward semantics mirroring `remove_ids` when current removed) + `pub fn jump_to_order_position(&mut self, position: usize) -> Option<i64>`.
- Create: `crates/reprise-gnome/src/ui/track_list/queue_row_mapping.rs` — pure remapping: `pub enum QueueRow { NowPlaying, PlayNext(usize), UpNext(usize) }`, `pub fn classify(view_pos: u32, sections: &[QueueSection]) -> Option<QueueRow>`, `pub fn reorder_op(from: u32, to: u32, sections) -> Option<QueueReorderOp>` where `enum QueueReorderOp { WithinPlayNext { from, to }, PromoteUpNext { order_pos, insert_at } }` (drops within the UpNext section → None = reject).
- Modify: `crates/reprise-gnome/src/ui/playback/queue_transport.rs` — `remove_queue_rows(&self, rows: &[QueueRow]) -> usize` (play-next → `up_next.remove_positions`; up-next → `queue.remove_order_positions`; now-playing → snapshot remove + advance), `reorder_queue_rows(&self, op: QueueReorderOp) -> bool` (promote = `queue.remove_order_positions([p])` → `up_next.insert(at, id)`), `jump_to_queue_row(&self, row: QueueRow)` (PlayNext → existing `play_up_next_at`; UpNext → `jump_to_order_position` + `play_track_id`; NowPlaying → restart current), `play_next(&self, ids)` → `up_next.prepend`, `clear_play_next(&self)` → `up_next.clear` — each fires `notify_queue_changed`.
- Modify: `track_list_dnd.rs::handle_queue_reorder_drop` + `window_action_wiring.rs:175-201` — route through the new remap (callbacks now take view positions and the stored `shared.queue_sections`).
- Modify: `track_list_queue_menu.rs` — remove/jump route through `classify`; add `ACTION_PLAY_NEXT = "play-next"`, label `CONTEXT_MENU_PLAY_NEXT = N_!("Play next")` shown for non-queue sources above "Add to queue", wired like `on_queue_selected` to `controller.play_next(ids)`.
- Modify: `queue_sections.rs` header factory — the Play Next header carries a flat `gtk::Button` "Clear" (`QUEUE_CLEAR_PLAY_NEXT = N_!("Clear")`, real Button per gtk4 skill) → `clear_play_next`.

- [ ] **Failing tests:** `queue_row_mapping` classification + reorder-op table tests (incl. reject within-UpNext, promote at boundary); core `remove_order_positions_removes_single_occurrence_and_advances_when_current`, `jump_to_order_position_moves_playhead`, `up_next insert/prepend/clear` tests.
- [ ] Red → implement → green. Gates. Commit `feat: queue interactions — reorder, promote, remove, jump, clear, play-next (QUE-3)` — flip QUE-3 → `[aktiv]`.

### Task 6: NAV-9 jump + NAV-2 back-stack

**Files:**
- Create: `crates/reprise-gnome/src/ui/nav_history.rs` — `pub(in crate::ui) struct NavHistory { stack: RefCell<Vec<ViewSource>>, navigating_back: Cell<bool> }` with `push(&self, source: ViewSource)` (dedup consecutive), `pop(&self) -> Option<ViewSource>`, `suppressed(&self) -> bool` + RAII-free guard helpers; capped at 50 entries.
- Modify: `crates/reprise-gnome/src/ui/sidebar/sidebar.rs` — `pub(in crate::ui) fn select_source(&self, source: &ViewSource) -> bool` (find row by source, `listbox.select_row` so routing fires; false when the row is gone → caller falls back to Library).
- Modify: `crates/reprise-gnome/src/ui/window/library_shell.rs::wire_source_routing` — push the previous source onto NavHistory on every route (skip when `navigating_back`).
- Modify: `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs` — cover click rewired from panel-toggle to jump; new `win.jump-to-now-playing` action (accel `<Control>l`) and `win.nav-back` action (accel `<Alt>Left`); jump: read `player.play_origin()` (fallback Library), push current source, `sidebar.select_source(&origin.source)` (fallback Library row), then `controller.notify_restored_current_track()` (existing select+center — centers mid-viewport, satisfying the middle-third requirement).
- Modify: `current_track_selection.rs:148-152` — title click routes to the same jump closure (controller `set_on_title_click` wiring moves to `window_runtime_wiring.rs` beside cover).
- Modify: `ui/help.rs` — add both shortcuts to the shortcuts window.

**NAV-3 untouched:** artist click keeps its Artists-tab target.

- [ ] **Failing tests:** `nav_history` unit tests (push/pop order, consecutive dedup, cap, back-suppression flag); `select_source` covered via smoke.
- [ ] Red → implement → green. Gates. Commit `feat: jump to now playing from player bar cover/title with back history (NAV-9, NAV-2)` — flip NAV-9 and NAV-2 → `[aktiv]`.

### Task 7: Acceptance verification + ledger

- [ ] Headless E2E (isolated dbus/Xvfb/fakesink, pattern from `scripts/check-lyrics-smoke.sh`) covering the four acceptance scenarios via `REPRISE_SMOKE_*` hooks and log assertions: (1) activate → queue source shows `1 + rest` composite with "from Music"; (2) play-next two tracks → section order + advance order; (3) queue→music restores scroll/selection (log the restored values), cover-jump selects+centers, back returns to queue; (4) stop → empty StatusPage strings. Extend smoke hooks minimally where a scenario can't be driven (e.g. `REPRISE_SMOKE_JUMP=1`).
- [ ] Full gate battery + `scripts/check-architecture.sh`.
- [ ] Append ledger line to `.superpowers/sdd/progress.md`.

## Self-Review

- Spec coverage: NAV-5→Task 2, NAV-9/NAV-2/Ctrl+L→Task 6, QUE-1/2/4/5→Task 4, QUE-3+Play-next+Clear→Task 5, rules doc+flips→Tasks 1..6, acceptance→Task 7. Origin threading (prerequisite for QUE-1 label + NAV-9 target) →Task 3. ✔
- Type consistency: `QueueViewModel`/`QueueSection`/`QueueSectionKind` defined Task 4, consumed Task 5 mapping; `PlayOrigin` defined Task 3, consumed Tasks 4/6. `QueueRow`/`QueueReorderOp` defined+consumed Task 5. ✔
- Open risk, called out: `SectionModelImpl` + `ColumnView::set_header_factory` on the custom windowed model is the least-proven piece — Task 4 does it first behind a pure `queue_sections` module so a fallback (header-as-first-section-row) would only replace the factory wiring, not the composition logic.
