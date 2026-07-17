//! Callback type aliases for `track_list.rs`'s `Shared` seams — split out
//! of the surface file (UI-orchestrator size rule). Each alias's doc
//! comment explains the seam; the fields on `Shared` and the setters on
//! `TrackList` reference these by name.

use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::models::Track;
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

/// Callback invoked on row activation (double-click/Enter on a row, or the
/// `REPRISE_SMOKE_ACTIVATE` hook). Provided by `window::build`, which routes
/// it to the player — the track list itself stays free of any playback
/// knowledge. Alongside the activated row's `Track` (for logging/fallback,
/// see the `None` player branch in `window::build`), it also carries the
/// full queue this activation should start: `ids` is every track id in the
/// activated row's current sort/filter view (via `queue_ids_for_activation`)
/// and `start_index` is the activated row's position within that list —
/// together, exactly `PlayerController::play_from_view`'s parameters.
pub type OnActivate = Box<dyn Fn(&Track, Vec<i64>, usize, ViewSource)>;

/// Callback invoked at the end of every `reload()` — see the `Shared::
/// on_reload` doc comment for what each parameter carries and why
/// `window.rs` needs all four.
pub(in crate::ui) type OnReload = Box<dyn Fn(&ViewSource, usize, &str, &BrowseFilter)>;

/// Context-menu "Play" action callback — see the `Shared::on_play_selected`
/// doc comment.
pub(in crate::ui) type OnPlaySelected = Rc<dyn Fn(Vec<i64>, usize, ViewSource)>;
/// Context-menu "Add to queue" action callback — see the `Shared::on_queue_
/// selected` doc comment.
pub(in crate::ui) type OnQueueSelected = Rc<dyn Fn(Vec<i64>)>;
pub(in crate::ui) type OnQueueActivate = Rc<dyn Fn(super::queue_row_mapping::QueueRow)>;
pub(in crate::ui) type OnQueueRemove = Rc<dyn Fn(&[super::queue_row_mapping::QueueRow]) -> usize>;
/// Queue drag-reorder callback — see the `Shared::on_queue_reorder` doc
/// comment. Returns whether the move actually happened (`false` for a
/// degraded no-op, e.g. no player wired — see `Shared::on_queue_reorder`'s
/// doc comment), which `ui::track_list_dnd`'s drop handler propagates as its
/// own result rather than reporting success just because a callback was
/// present (Stage 3 Task 6 review finding #3).
pub(in crate::ui) type OnQueueReorder =
    Rc<dyn Fn(super::queue_row_mapping::QueueReorderOp) -> bool>;
/// Sidebar drag-and-drop "add to playlist" callback — see the `Shared::on_
/// sidebar_playlist_drop` doc comment.
pub(in crate::ui) type OnSidebarPlaylistDrop = Rc<dyn Fn(i64, &str, &[i64]) -> bool>;
/// "Remove from library" callback — see the `Shared::on_library_mutated` doc
/// comment. Takes the ids actually deleted (Stage-3 close-out).
pub(in crate::ui) type OnLibraryMutated = Rc<dyn Fn(&[i64])>;
/// Successful tag-edit callback. Paths let the player invalidate only the
/// currently displayed cover while the window refreshes sidebar metadata.
pub(in crate::ui) type OnTagsMutated = Rc<dyn Fn(&[PathBuf])>;
pub(in crate::ui) type OnSelectionChanged = Rc<dyn Fn(crate::ui::info_panel_state::PanelContext)>;
