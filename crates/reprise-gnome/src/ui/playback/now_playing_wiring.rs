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

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::IsA;
use libadwaita as adw;

use crate::ui::one_shot_task;
use crate::ui::player_controller::PlayerController;
use reprise_core::cover::ThumbnailSize;
use reprise_core::media_integration::MprisState;
use reprise_core::playback::{PlaybackError, PlaybackState, SpectrumFrame};
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
            let conn = &self.conn;
            reprise_core::queries::query_track_summary(conn, id)
        };
        let Ok(Some(summary)) = summary else {
            return;
        };
        let title = summary.title;
        let artist = summary.artist;
        let album = summary.album;
        let year = summary.year;
        // Update the player-owned cache before `sync_track` fans the snapshot
        // out to the bar, compact player, and right Now Playing panel.
        if let Some(np) = self.now_playing.borrow_mut().as_mut() {
            np.title = title.clone();
            np.artist = artist.clone();
            np.album = album.clone();
        }
        self.sync_track(&title, &artist, &album, year);
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
        let availability = crate::ui::playing_links::LinkAvailability {
            artist: !artist.trim().is_empty(),
            album: self.current_album_identity().is_some(),
        };
        let mode = self.playback_mode();
        self.bar.set_track(
            title,
            artist,
            crate::ui::playing_links::player_bar_labels(mode, availability),
        );
        self.compact_player.set_track(title, artist);
        self.notify_now_playing_panel_track_changed(crate::ui::playing_links::panel_labels(
            mode,
            availability,
        ));
    }

    /// Clears Bar, Compact, Lyrics, and the Now Playing panel together — the
    /// `Stopped`/failure-path counterpart to `sync_track`.
    pub(in crate::ui) fn sync_clear_track(&self) {
        self.bar.clear_track();
        self.compact_player.clear_track();
        self.sync_lyrics_track(None);
        self.notify_now_playing_panel_track_changed(crate::ui::playing_links::panel_labels(
            self.playback_mode(),
            crate::ui::playing_links::LinkAvailability {
                artist: false,
                album: false,
            },
        ));
    }

    pub(in crate::ui) fn set_on_now_playing_panel_track_changed(
        &self,
        callback: impl Fn(Option<super::player_controller::NowPlaying>, crate::ui::playing_links::LinkLabels)
            + 'static,
    ) {
        *self.now_playing_panel_track_changed.borrow_mut() = Some(Rc::new(callback));
        let track = self.now_playing.borrow().clone();
        let availability = crate::ui::playing_links::LinkAvailability {
            artist: track
                .as_ref()
                .is_some_and(|track| !track.artist.trim().is_empty()),
            album: track
                .as_ref()
                .is_some_and(|track| !track.album.trim().is_empty()),
        };
        self.notify_now_playing_panel_track_changed(crate::ui::playing_links::panel_labels(
            self.playback_mode(),
            availability,
        ));
    }

    fn notify_now_playing_panel_track_changed(&self, labels: crate::ui::playing_links::LinkLabels) {
        let track = self.now_playing.borrow().clone();
        let callback = self.now_playing_panel_track_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(track, labels);
        }
    }

    pub(in crate::ui) fn set_on_now_playing_panel_state_changed(
        &self,
        callback: impl Fn(PlaybackState) + 'static,
    ) {
        *self.now_playing_panel_state_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_song_visual_spectrum_changed(
        &self,
        callback: impl Fn(SpectrumFrame) + 'static,
    ) {
        *self.song_visual_spectrum_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_song_visuals_enabled(
        &self,
        enabled: bool,
    ) -> Result<(), PlaybackError> {
        if !enabled {
            self.sync_bass(0.0, 0.0);
        }
        self.player.set_spectrum_enabled(enabled)
    }

    /// The bass pair, fanned out to the bar's reactive layers. Same
    /// discipline as the other `sync_*`: one place feeds the bar.
    pub(in crate::ui) fn sync_bass(&self, kick: f32, pressure: f32) {
        self.bar.set_bass(f64::from(kick), f64::from(pressure));
        let callbacks = self.bass_changed.borrow().clone();
        for callback in callbacks {
            callback(kick, pressure);
        }
    }

    /// Loads `path`'s cover into the bar and compact player through the shared
    /// `CoverLoader` instance. The bar's cover load also carries the MPRIS
    /// art_url callback.
    pub(in crate::ui) fn sync_cover(&self, path: &str) {
        if let Some(track_id) = self.now_playing.borrow().as_ref().map(|t| t.id) {
            self.sync_waveform(track_id, path);
        }
        let bar_generation = self.bar_cover_generation.get().wrapping_add(1);
        self.bar_cover_generation.set(bar_generation);
        let track_id = self.now_playing.borrow().as_ref().map(|track| track.id);
        if let Some(track_id) = track_id {
            let now_playing = self.now_playing.clone();
            let mpris_state = self.mpris_state.clone();
            let bar_cover_target = self.bar.cover_image().clone();
            self.cover_loader.load_into_with_resolution(
                &bar_cover_target,
                path,
                ThumbnailSize::Bar,
                bar_generation,
                &self.bar_cover_generation,
                move |cover_path| {
                    let Some(cover_path) = cover_path else {
                        return;
                    };
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
        self.offer_colour_legend(track_id);
        let generation = self.waveform_generation.get().wrapping_add(1);
        self.waveform_generation.set(generation);
        let waveform_generation = self.waveform_generation.clone();
        let waveform_backend = self.waveform_backend.clone();
        let waveform = self.bar.waveform_handle();
        let compact_player = self.compact_player.clone();
        let db_path = reprise_core::db::default_path();
        let track_path = std::path::PathBuf::from(path);
        let Ok(receiver) = one_shot_task::spawn("reprise-waveform", move || {
            reprise_core::db::Db::open_migrated(Some(&db_path))
                .ok()
                .and_then(|db| {
                    let peaks = reprise_core::waveform_cache::peaks_for_playback(
                        &db,
                        track_id,
                        &track_path,
                        waveform_backend.as_ref(),
                    )?;
                    // Only a track whose peaks had to be decoded just now also
                    // got a spectrogram stored; one with cached peaks returns
                    // from the call above untouched. So this is `None` for
                    // everything the library analysis has not reached yet, and
                    // the bar draws in the plain accent until it has.
                    let centroid = reprise_core::waveform_cache::centroid_for_playback(
                        &db,
                        track_id,
                        peaks.len(),
                    );
                    Some((peaks, centroid))
                })
        }) else {
            return;
        };
        let colour_backend = self.waveform_backend.clone();
        let colour_path = std::path::PathBuf::from(path);
        glib::spawn_future_local(async move {
            if let Ok(Some((peaks, centroid))) = receiver.recv().await {
                if waveform_generation.get() != generation {
                    return;
                }
                let buckets = peaks.len();
                // Same peaks feed both players so the mini waveform (frame
                // 1e) shows the real shape + progress, not the skeleton.
                compact_player.set_analysis(peaks.clone(), centroid.clone());
                waveform.set_analysis(peaks.clone(), centroid.clone());
                if centroid.is_some() {
                    return;
                }
                // No curve yet: the shape is already on screen, so the decode
                // that produces the colour runs behind it and the bar is
                // recoloured when it lands. Nobody waits for this.
                let db_path = reprise_core::db::default_path();
                let Ok(receiver) = one_shot_task::spawn("reprise-waveform-colour", move || {
                    reprise_core::db::Db::open_migrated(Some(&db_path))
                        .ok()
                        .and_then(|db| {
                            reprise_core::waveform_cache::ensure_centroid_for_playback(
                                &db,
                                track_id,
                                &colour_path,
                                buckets,
                                colour_backend.as_ref(),
                            )
                        })
                }) else {
                    return;
                };
                if let Ok(Some(curve)) = receiver.recv().await {
                    if waveform_generation.get() == generation {
                        // Same bars, now with colour: `set_analysis` crossfades
                        // between them, so the bar warms into its colours
                        // instead of snapping.
                        compact_player.set_analysis(peaks.clone(), Some(curve.clone()));
                        waveform.set_analysis(peaks, Some(curve));
                    }
                }
            }
        });
    }

    /// Shows the colour-scale legend for the first few track changes and then
    /// stops. The count lives in settings, so it survives restarts; the bar
    /// itself decides whether this track is a new one and whether its bar even
    /// has a scale to explain.
    fn offer_colour_legend(&self, track_id: i64) {
        use reprise_core::library::settings;

        if !self.bar.colour_legend_due_for(track_id) {
            return;
        }
        let seen = settings::get_seek_legend_seen(&self.conn);
        if !crate::ui::player_bar::seek_legend::shows_on_track_change(seen) {
            return;
        }
        if let Err(error) = settings::set_seek_legend_seen(&self.conn, seen + 1) {
            // Failing to store the count would show the legend forever, which
            // is worse than not showing it now.
            tracing::warn!(%error, "could not record the colour legend showing");
            return;
        }
        self.bar.show_colour_legend();
    }

    pub(in crate::ui) fn sync_state(&self, state: PlaybackState) {
        self.bar.set_state(state);
        if state != PlaybackState::Playing {
            // The bar resets its own consumers in `set_state`; use the shared
            // fan-out as well so the realised running-row wash reaches rest.
            self.sync_bass(0.0, 0.0);
        }
        self.compact_player.set_state(state);
        self.sync_lyrics_state(state);
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

    pub(in crate::ui) fn sync_transport_enabled(&self, queue_has_tracks: bool) {
        let play_available = queue_has_tracks || self.library_has_tracks.get();
        self.bar
            .set_transport_enabled(queue_has_tracks, self.library_has_tracks.get());
        self.compact_player.set_transport_enabled(play_available);
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
pub(in crate::ui) const LIBRARY_CONTENT_TAG: &str = "library-content";

pub(in crate::ui) fn build_content_nav(
    library_content: &impl IsA<gtk4::Widget>,
    app_name: &str,
) -> adw::NavigationView {
    let library_page = adw::NavigationPage::builder()
        .title(app_name)
        .tag(LIBRARY_CONTENT_TAG)
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

    #[test]
    fn player_cover_loading_has_no_cover_color_pipeline() {
        let wiring = include_str!("now_playing_wiring.rs");
        let controller = include_str!("player_controller.rs");
        let style = include_str!("../style/mod.rs");
        for retired in [
            ["apply_cover", "_accent"].concat(),
            ["reset_cover", "_accent"].concat(),
            ["cover", "_accent", "_generation"].concat(),
            ["cover", "_accent", "_last"].concat(),
        ] {
            assert!(!wiring.contains(&retired), "wiring retained {retired}");
            assert!(
                !controller.contains(&retired),
                "controller retained {retired}"
            );
        }
        assert!(!style.contains(&["mod cover", "_accent;"].concat()));
    }
}
