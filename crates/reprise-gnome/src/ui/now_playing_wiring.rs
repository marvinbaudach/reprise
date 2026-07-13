//! Constructs and wires the Now-Playing full view (Task 8) into both the
//! controller and the window's navigation shell — split out of
//! `player_controller.rs` purely to keep that file under the project's
//! file-size limit and to give the Now-Playing fan-out one dedicated home
//! (same rationale, and same sibling-module shape, as `mpris_mirror.rs`/
//! `playback_faults.rs`/`queue_transport.rs`).
//!
//! Two halves:
//!
//! - **Controller-facing** (`impl PlayerController`, `wire_now_playing_
//!   controls`): the `sync_*` methods are the ONE place that feeds both
//!   `PlayerBar` and `NowPlayingView` from a single state update — the
//!   controller's every existing bar-facing call site was changed to call
//!   these instead of `self.bar.set_*` directly (see `player_controller.rs`,
//!   `mpris_mirror.rs`, `player_controller_wiring.rs`), so the two widgets
//!   can never drift to two different playback/seek states (the same
//!   discipline as the MPRIS mirror). `wire_now_playing_controls` connects
//!   the page's transport signals to the exact same controller actions the
//!   bar uses (`toggle_pause`/`seek`/`previous`/`next`/queue mutations) —
//!   same shape as `player_controller_wiring.rs`'s `wire_bar_controls`.
//! - **Window-facing** (`build_content_nav`/`wire_bar_expand`/`arm_smoke_
//!   nowplaying`): builds the `adw::NavigationView` the shell's content page
//!   becomes, and wires the bar's click-to-expand callback and the headless
//!   smoke hook. Called from `window::build`.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::IsA;
use libadwaita as adw;

use crate::ui::player_controller::PlayerController;
use reprise_core::cover::ThumbnailSize;
use reprise_core::media_integration::MprisState;
use reprise_core::playback::PlaybackState;
use reprise_core::queue::Repeat;

fn cover_path_to_uri(path: &Path) -> Option<String> {
    match glib::filename_to_uri(path, None) {
        Ok(uri) => Some(uri.to_string()),
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "could not build MPRIS cover URI");
            None
        }
    }
}

fn set_art_url_for_current_track(mirror: &mut MprisState, track_id: i64, art_url: String) -> bool {
    if mirror.track_id != Some(track_id) {
        return false;
    }
    mirror.art_url = Some(art_url);
    true
}

impl PlayerController {
    /// Invalidates and reloads the displayed cover when a successful tag
    /// edit touched the currently playing path. Playback itself is left
    /// untouched.
    pub(super) fn refresh_edited_cover(&self, edited_paths: &[PathBuf]) {
        let current_path = self
            .now_playing
            .borrow()
            .as_ref()
            .map(|track| track.path.clone());
        let Some(current_path) = current_path else {
            return;
        };
        if !edited_paths
            .iter()
            .any(|path| path == std::path::Path::new(&current_path))
        {
            return;
        }
        self.cover_loader.invalidate_paths(edited_paths);
        self.sync_cover(&current_path);
    }

    /// Feeds Bar, Compact, and Now Playing metadata from one call. Compact
    /// additionally receives the optional year used by Card; the other
    /// surfaces retain their existing metadata set.
    pub(super) fn sync_track(&self, title: &str, artist: &str, album: &str, year: Option<i32>) {
        self.bar.set_track(title, artist);
        self.compact_player.set_track(title, artist, album, year);
        self.now_playing_view.set_track(title, artist, album);
    }

    /// Clears the bar's AND the page's track display together — the
    /// `Stopped`/failure-path counterpart to `sync_track`.
    pub(super) fn sync_clear_track(&self) {
        self.bar.clear_track();
        self.compact_player.clear_track();
        self.now_playing_view.clear_track();
    }

    /// Loads `path`'s cover into both widgets through the ONE shared
    /// `CoverLoader` instance (`self.cover_loader`) — same source, same
    /// on-disk cache, just two sizes and two independent generation tokens
    /// (`bar_cover_generation`/`now_playing_cover_generation`) so a stale
    /// in-flight load for either widget can never clobber a newer one (see
    /// `cover_loader.rs`). This is the "no second cache" half of the single
    /// state path the design rule requires.
    pub(super) fn sync_cover(&self, path: &str) {
        let bar_generation = self.bar_cover_generation.get().wrapping_add(1);
        self.bar_cover_generation.set(bar_generation);
        self.cover_loader.load_into(
            self.bar.cover_image(),
            path,
            ThumbnailSize::Bar,
            bar_generation,
            &self.bar_cover_generation,
        );

        let compact_generation = self.compact_cover_generation.get().wrapping_add(1);
        self.compact_cover_generation.set(compact_generation);
        self.compact_player.set_cover_placeholder();
        self.cover_loader.load_into(
            self.compact_player.cover_image(),
            path,
            ThumbnailSize::Bar,
            compact_generation,
            &self.compact_cover_generation,
        );

        let full_generation = self.now_playing_cover_generation.get().wrapping_add(1);
        self.now_playing_cover_generation.set(full_generation);
        let track_id = self.now_playing.borrow().as_ref().map(|track| track.id);
        if let Some(track_id) = track_id {
            let now_playing = self.now_playing.clone();
            let mpris_state = self.mpris_state.clone();
            self.cover_loader.load_into_with_path(
                self.now_playing_view.cover_image(),
                path,
                ThumbnailSize::Full,
                full_generation,
                &self.now_playing_cover_generation,
                move |cover_path| {
                    let Some(art_url) = cover_path_to_uri(&cover_path) else {
                        return;
                    };
                    {
                        let mut current = now_playing.borrow_mut();
                        let Some(track) = current.as_mut().filter(|track| track.id == track_id)
                        else {
                            return;
                        };
                        track.art_url = Some(art_url.clone());
                    }
                    let mut mirror = mpris_state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    set_art_url_for_current_track(&mut mirror, track_id, art_url);
                },
            );
        } else {
            self.cover_loader.load_into(
                self.now_playing_view.cover_image(),
                path,
                ThumbnailSize::Full,
                full_generation,
                &self.now_playing_cover_generation,
            );
        }
    }

    pub(super) fn sync_state(&self, state: PlaybackState) {
        self.bar.set_state(state);
        self.compact_player.set_state(state);
        self.now_playing_view.set_state(state);
    }

    pub(super) fn sync_position(&self, position_ms: i64, duration_ms: i64) {
        self.bar.set_position(position_ms, duration_ms);
        self.compact_player.set_position(position_ms, duration_ms);
        self.now_playing_view.set_position(position_ms, duration_ms);
    }

    pub(super) fn sync_transport_enabled(&self, enabled: bool) {
        self.bar.set_transport_enabled(enabled);
        self.compact_player.set_transport_enabled(enabled);
        self.now_playing_view.set_transport_enabled(enabled);
    }

    /// Sets the shuffle indicator on both widgets — called from whichever
    /// widget's own click originated the change, and from MPRIS's `Shuffle`
    /// write (`mpris_mirror.rs`'s `mpris_set_shuffle`). Each widget's own
    /// `updating_shuffle` guard makes re-setting the originating widget's
    /// indicator a harmless no-op, so callers never need to know which
    /// widget (if any) was the origin.
    pub(super) fn sync_shuffle_indicator(&self, active: bool) {
        self.bar.set_shuffle_indicator(active);
        self.compact_player.set_shuffle_indicator(active);
        self.now_playing_view.set_shuffle_indicator(active);
    }

    /// Same shape as `sync_shuffle_indicator`, for the repeat button.
    pub(super) fn sync_repeat_indicator(&self, repeat: Repeat) {
        self.bar.set_repeat_indicator(repeat);
        self.compact_player.set_repeat_indicator(repeat);
        self.now_playing_view.set_repeat_indicator(repeat);
    }

    pub(super) fn sync_volume_indicator(&self, volume: f64) {
        self.bar.set_volume_indicator(volume);
        self.compact_player.set_volume_indicator(volume);
    }
}

/// Wires the Now-Playing page's transport signals to the exact same
/// controller actions `player_controller_wiring.rs`'s `wire_bar_controls`
/// wires the bar's to — one code path per action, shared by both widgets
/// (DRY), so pressing play/pause/seek/previous/next/shuffle/repeat on
/// either widget has identical effect. Each closure holds a `Weak`
/// controller reference for the same reason `wire_bar_controls`'s do: the
/// page is owned *by* the controller, so a strong reference here would be a
/// leak-guaranteeing `Rc` cycle.
pub(super) fn wire_now_playing_controls(controller: &Rc<PlayerController>) {
    let weak = Rc::downgrade(controller);
    controller.now_playing_view.connect_play_pause(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.toggle_pause();
    });

    let weak = Rc::downgrade(controller);
    controller
        .now_playing_view
        .connect_seek(move |position_ms| {
            let Some(controller) = weak.upgrade() else {
                return;
            };
            controller.seek(position_ms);
        });

    let weak = Rc::downgrade(controller);
    controller.now_playing_view.connect_previous(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.previous();
    });

    let weak = Rc::downgrade(controller);
    controller.now_playing_view.connect_next(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.next();
    });

    let weak = Rc::downgrade(controller);
    controller
        .now_playing_view
        .connect_shuffle_toggled(move |active| {
            let Some(controller) = weak.upgrade() else {
                return;
            };
            controller.queue.borrow_mut().set_shuffle(active);
            let is_shuffled = controller.queue.borrow().is_shuffled();
            controller.sync_shuffle_indicator(is_shuffled);
            controller.update_mpris_shuffle(is_shuffled);
            tracing::debug!(is_shuffled, "shuffle toggled (now-playing page)");
        });

    let weak = Rc::downgrade(controller);
    controller.now_playing_view.connect_repeat_clicked(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        // Same explicit-block borrow shape as `wire_bar_controls`'s repeat
        // handler — see `player_controller.rs`'s `## Queue borrow
        // discipline` doc section.
        let next_repeat = {
            let mut queue = controller.queue.borrow_mut();
            let next_repeat = crate::ui::player_controller_wiring::cycle_repeat(queue.repeat());
            queue.set_repeat(next_repeat);
            next_repeat
        };
        controller.sync_repeat_indicator(next_repeat);
        controller.update_mpris_repeat(next_repeat);
    });
}

/// Builds the content `adw::NavigationView`: the library page (wrapping the
/// existing toast overlay) as the static root, plus the Now-Playing page
/// (if the player is available) as a second static page — added via
/// `NavigationView::add`, not `push`, so `wire_bar_expand`/`arm_smoke_
/// nowplaying` pushing it later doesn't destroy/reconstruct it, and popping
/// it back off leaves it alive for the next bar click (see `AdwNavigation
/// View`'s doc: a page added this way is "pushed automatically" only for
/// the FIRST page added — the library page here — every later `add`ed page
/// stays off the visible stack until explicitly `push`ed).
pub(super) fn build_content_nav(
    library_content: &impl IsA<gtk4::Widget>,
    now_playing_page: Option<&adw::NavigationPage>,
    app_name: &str,
) -> adw::NavigationView {
    let library_page = adw::NavigationPage::builder()
        .title(app_name)
        .child(library_content)
        .build();
    let nav = adw::NavigationView::new();
    nav.add(&library_page);
    if let Some(page) = now_playing_page {
        nav.add(page);
    }
    nav
}

/// Wires the bar's cover/track-info click to push the Now-Playing page onto
/// `nav`. A no-op if the player is unavailable — same player-unavailable
/// degradation every other bar-driven feature in `window.rs` uses.
pub(super) fn wire_bar_expand(player: Option<&Rc<PlayerController>>, nav: &adw::NavigationView) {
    let Some(player) = player else { return };
    let nav = nav.clone();
    let page = player.now_playing_widget().clone();
    player.set_on_expand(move || nav.push(&page));
}

/// Headless verification hook for Task 8: `REPRISE_SMOKE_NOWPLAYING=1`
/// pushes the Now-Playing page (deferred via `glib::idle_add_local_once`,
/// mirroring `player_controller_wiring.rs`'s `arm_smoke_repeat` convention)
/// and logs so a smoke run can grep for it.
const SMOKE_NOWPLAYING_ENV_VAR: &str = "REPRISE_SMOKE_NOWPLAYING";

pub(super) fn arm_smoke_nowplaying(
    player: Option<&Rc<PlayerController>>,
    nav: &adw::NavigationView,
) {
    if std::env::var(SMOKE_NOWPLAYING_ENV_VAR).is_err() {
        return;
    }
    let Some(player) = player else {
        tracing::warn!("{SMOKE_NOWPLAYING_ENV_VAR} set but no player available; skipping");
        return;
    };
    let nav = nav.clone();
    let page = player.now_playing_widget().clone();
    glib::idle_add_local_once(move || {
        nav.push(&page);
        tracing::info!("smoke: opened now-playing view");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_path_to_uri_encodes_special_characters() {
        let uri = cover_path_to_uri(Path::new("/tmp/Reprise Cover ä.png")).unwrap();
        assert_eq!(uri, "file:///tmp/Reprise%20Cover%20%C3%A4.png");
    }

    #[test]
    fn stale_cover_result_cannot_replace_current_mpris_art() {
        let mut mirror = MprisState {
            track_id: Some(2),
            art_url: Some("file:///current.png".into()),
            ..MprisState::default()
        };

        assert!(!set_art_url_for_current_track(
            &mut mirror,
            1,
            "file:///stale.png".into(),
        ));
        assert_eq!(mirror.art_url.as_deref(), Some("file:///current.png"));
    }

    #[test]
    fn current_cover_result_updates_mpris_art() {
        let mut mirror = MprisState {
            track_id: Some(2),
            ..MprisState::default()
        };

        assert!(set_art_url_for_current_track(
            &mut mirror,
            2,
            "file:///cover.png".into(),
        ));
        assert_eq!(mirror.art_url.as_deref(), Some("file:///cover.png"));
    }
}
