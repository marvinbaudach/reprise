//! Playback-state fan-out and navigation shell helpers — split out of
//! `player_controller.rs` purely to keep that file under the project's
//! file-size limit (same rationale, and same sibling-module shape, as
//! `mpris_mirror.rs`/`playback_faults.rs`/`queue_transport.rs`).
//!
//! The `sync_*` methods are the ONE place that feeds the `PlayerBar` and
//! compact player from a single state update — the controller's every
//! bar-facing call site calls these instead of `self.bar.set_*` directly
//! (see `player_controller.rs`, `mpris_mirror.rs`,
//! `player_controller_wiring.rs`).
//!
//! `build_content_nav` builds the `adw::NavigationView` the shell's content
//! page becomes. Called from `library_shell::build`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::IsA;
use libadwaita as adw;

use crate::ui::player_controller::PlayerController;
use crate::ui::style::cover_accent::Rgb;
use reprise_core::cover::ThumbnailSize;
use reprise_core::media_integration::MprisState;
use reprise_core::playback::PlaybackState;
use reprise_core::queue::Repeat;
use reprise_core::waveform::STORED_PEAK_COUNT;

fn cover_path_to_uri(path: &Path) -> Option<String> {
    match glib::filename_to_uri(path, None) {
        Ok(uri) => Some(uri.to_string()),
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "could not build MPRIS cover URI");
            None
        }
    }
}

/// Off-main cover-accent extraction: decode the cover, derive its dominant
/// accent, and cross-fade to it (generation-guarded so a rapid track change
/// can't apply a stale album accent). A non-colorful cover cross-fades to the
/// theme fallback. The previous accent is read from (and written back to)
/// `last_accent_cell`; `widget` is required for the animation target.
fn apply_cover_accent(
    generation_cell: &Rc<std::cell::Cell<u64>>,
    last_accent_cell: &Rc<RefCell<Option<Rgb>>>,
    cover_path: &Path,
    widget: impl IsA<gtk4::Widget> + Clone + 'static,
) {
    let generation = generation_cell.get().wrapping_add(1);
    generation_cell.set(generation);
    let generation_cell = generation_cell.clone();
    let last_accent_cell = last_accent_cell.clone();
    let cover_path = cover_path.to_path_buf();
    let (sender, receiver) = async_channel::bounded(1);
    if std::thread::Builder::new()
        .name("reprise-cover-accent".to_string())
        .spawn(move || {
            let _ = sender.send_blocking(crate::ui::style::cover_accent::accent_from_cover_file(
                &cover_path,
            ));
        })
        .is_err()
    {
        return;
    }
    glib::spawn_future_local(async move {
        if let Ok(new_color) = receiver.recv().await {
            if generation_cell.get() == generation {
                let old_color = *last_accent_cell.borrow();
                *last_accent_cell.borrow_mut() = new_color;
                crate::ui::style::cover_accent::cross_fade_accent(old_color, new_color, &widget);
            }
        }
    });
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
    pub(in crate::ui) fn refresh_edited_cover(&self, edited_paths: &[PathBuf]) {
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

    /// Re-reads title/artist/album from DB for the currently playing track
    /// when that track was just edited. Updates the player bar and MPRIS
    /// metadata in place without interrupting playback.
    pub(in crate::ui) fn refresh_edited_metadata(&self, edited_ids: &[i64]) {
        let current_id = self.current_track.get().map(|(id, _)| id);
        let Some(id) = current_id else {
            return;
        };
        if !edited_ids.contains(&id) {
            return;
        }
        let summary = {
            let conn = self.conn.borrow();
            reprise_core::queries::query_track_summary(&conn, id)
        };
        let Ok(Some(summary)) = summary else {
            return;
        };
        let title = summary.title;
        let artist = summary.artist;
        let album = summary.album;
        let year = summary.year;
        // Update bar + compact player
        self.sync_track(&title, &artist, &album, year);
        // Update MPRIS now_playing cache
        if let Some(np) = self.now_playing.borrow_mut().as_mut() {
            np.title = title.clone();
            np.artist = artist.clone();
            np.album = album.clone();
        }
        // Update MPRIS mirror state (will trigger PropertiesChanged via poll diff)
        {
            let mut state = self
                .mpris_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.title = title;
            state.artist = artist;
            state.album = album;
        }
    }

    /// Feeds Bar, Compact, and Now Playing metadata from one call.
    pub(in crate::ui) fn sync_track(
        &self,
        title: &str,
        artist: &str,
        _album: &str,
        _year: Option<i32>,
    ) {
        self.bar.set_track(title, artist);
        self.compact_player.set_track(title, artist);
    }

    /// Clears Bar, Compact, and Lyrics together — the `Stopped`/failure-path
    /// counterpart to `sync_track`.
    pub(in crate::ui) fn sync_clear_track(&self) {
        self.bar.clear_track();
        self.compact_player.clear_track();
        self.sync_lyrics_track(None);
        self.reset_cover_accent();
    }

    /// Reverts the cover-derived accent to the theme fallback AND bumps the
    /// generation, so an accent extraction still in flight for the previous
    /// track can't re-apply its (now stale) album accent afterwards. This is
    /// the clear-path counterpart of `apply_cover_accent`'s own bump — without
    /// it, a Stop or a switch to a coverless track would leave the previous
    /// album's hue tinting the waveform and play button.
    fn reset_cover_accent(&self) {
        let generation = self.cover_accent_generation.get().wrapping_add(1);
        self.cover_accent_generation.set(generation);
        let old_color = *self.cover_accent_last.borrow();
        *self.cover_accent_last.borrow_mut() = None;
        crate::ui::style::cover_accent::cross_fade_accent(old_color, None, self.bar.widget());
    }

    /// Loads `path`'s cover into the bar and compact player through the shared
    /// `CoverLoader` instance. The bar's cover load also carries the MPRIS
    /// art_url callback and cover-accent extraction (previously on the
    /// now-playing page's full-size load).
    pub(in crate::ui) fn sync_cover(&self, path: &str) {
        if let Some(track_id) = self.now_playing.borrow().as_ref().map(|t| t.id) {
            self.sync_waveform(track_id, path);
        }
        // Revert to the theme fallback up front: if this track has no usable
        // cover, the loader's `on_loaded` never fires and `apply_cover_accent`
        // is never reached, so without this reset the previous album's accent
        // would linger. A track that *does* have a cover re-applies its accent
        // once decoding completes.
        self.reset_cover_accent();
        let bar_generation = self.bar_cover_generation.get().wrapping_add(1);
        self.bar_cover_generation.set(bar_generation);
        let track_id = self.now_playing.borrow().as_ref().map(|track| track.id);
        if let Some(track_id) = track_id {
            let now_playing = self.now_playing.clone();
            let mpris_state = self.mpris_state.clone();
            let cover_accent_generation = self.cover_accent_generation.clone();
            let cover_accent_last = self.cover_accent_last.clone();
            let bar_widget = self.bar.widget().clone();
            self.cover_loader.load_into_with_path(
                self.bar.cover_image(),
                path,
                ThumbnailSize::Bar,
                bar_generation,
                &self.bar_cover_generation,
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
                    apply_cover_accent(
                        &cover_accent_generation,
                        &cover_accent_last,
                        &cover_path,
                        bar_widget.clone(),
                    );
                },
            );
        } else {
            self.cover_loader.load_into(
                self.bar.cover_image(),
                path,
                ThumbnailSize::Bar,
                bar_generation,
                &self.bar_cover_generation,
            );
        }

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
    }

    /// Loads pre-computed waveform peaks from the DB off-main and hands them
    /// to the player bar. If no peaks exist yet, extracts them on demand and
    /// stores them in the DB so subsequent plays are instant.
    pub(in crate::ui) fn sync_waveform(&self, track_id: i64, path: &str) {
        let generation = self.waveform_generation.get().wrapping_add(1);
        self.waveform_generation.set(generation);
        let waveform_generation = self.waveform_generation.clone();
        let waveform_backend = self.waveform_backend.clone();
        let waveform = self.bar.waveform_handle();
        let db_path = reprise_core::db::default_path();
        let track_path = std::path::PathBuf::from(path);
        let (sender, receiver) = async_channel::bounded(1);
        if std::thread::Builder::new()
            .name("reprise-waveform".to_string())
            .spawn(move || {
                let peaks = reprise_core::db::open_migrated(Some(&db_path))
                    .ok()
                    .and_then(|conn| {
                        // Try DB first.
                        if let Some(cached) = reprise_core::db::get_waveform_peaks(&conn, track_id)
                            .ok()
                            .flatten()
                        {
                            return Some(cached);
                        }
                        // Not cached — extract now and store for next time.
                        let peaks = waveform_backend
                            .extract_peaks(&track_path, STORED_PEAK_COUNT)
                            .ok()?;
                        reprise_core::db::set_waveform_peaks(&conn, track_id, &peaks).ok();
                        Some(peaks)
                    });
                let _ = sender.send_blocking(peaks);
            })
            .is_err()
        {
            return;
        }
        glib::spawn_future_local(async move {
            if let Ok(Some(peaks)) = receiver.recv().await {
                if waveform_generation.get() == generation {
                    waveform.set_peaks(peaks);
                }
            }
        });
    }

    pub(in crate::ui) fn sync_state(&self, state: PlaybackState) {
        self.bar.set_state(state);
        self.compact_player.set_state(state);
        // Fan the same state out to the track list's now-playing equaliser
        // (freeze on pause, drop the marker on stop). Cloned-out before the
        // call inside `notify_playback_state_changed`, per RefCell discipline.
        self.notify_playback_state_changed(state);
    }

    pub(in crate::ui) fn sync_position(&self, position_ms: i64, duration_ms: i64) {
        self.bar.set_position(position_ms, duration_ms);
        self.compact_player.set_position(position_ms, duration_ms);
        self.sync_lyrics_position(position_ms);
    }

    pub(in crate::ui) fn sync_transport_enabled(&self, enabled: bool) {
        self.bar.set_transport_enabled(enabled);
        self.compact_player.set_transport_enabled(enabled);
    }

    /// Sets the shuffle indicator on both widgets — called from whichever
    /// widget's own click originated the change, and from MPRIS's `Shuffle`
    /// write (`mpris_mirror.rs`'s `mpris_set_shuffle`). Each widget's own
    /// `updating_shuffle` guard makes re-setting the originating widget's
    /// indicator a harmless no-op, so callers never need to know which
    /// widget (if any) was the origin.
    pub(in crate::ui) fn sync_shuffle_indicator(&self, active: bool) {
        self.bar.set_shuffle_indicator(active);
        // Shuffle changed the play order, so the upcoming track changed too:
        // re-feed the gapless next. Every shuffle path funnels through here.
        self.feed_next();
    }

    /// Same shape as `sync_shuffle_indicator`, for the repeat button.
    pub(in crate::ui) fn sync_repeat_indicator(&self, repeat: Repeat) {
        self.bar.set_repeat_indicator(repeat);
        // Repeat mode changes what plays next (All wraps, One suppresses the
        // gapless pre-feed): re-feed. Every repeat path funnels through here.
        self.feed_next();
    }

    pub(in crate::ui) fn sync_volume_indicator(&self, volume: f64) {
        self.bar.set_volume_indicator(volume);
        self.compact_player.set_volume_indicator(volume);
    }
}

/// Builds the content `adw::NavigationView`: the library page (wrapping the
/// existing toast overlay) as the static root.
pub(in crate::ui) fn build_content_nav(
    library_content: &impl IsA<gtk4::Widget>,
    app_name: &str,
) -> adw::NavigationView {
    let library_page = adw::NavigationPage::builder()
        .title(app_name)
        .child(library_content)
        .build();
    let nav = adw::NavigationView::new();
    nav.add(&library_page);
    nav
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
