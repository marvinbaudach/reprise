//! Cover, metadata links, and the shared track-change crossfade for PlayerBar.
//!
//! Kept beside `player_bar.rs` so cover behavior does not push the main surface
//! past the project's 800-line code-file cap.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita::prelude::AnimationExt;

use crate::ui::{cover_loader::CoverLoader, motion, strings};

use super::PlayerBar;

impl PlayerBar {
    /// The cover thumbnail widget — the controller feeds it through the shared
    /// `CoverLoader` after `set_track`.
    pub fn cover_image(&self) -> &gtk4::Image {
        &self.cover
    }

    /// Resets the cover and its lift when no track remains active.
    pub fn clear_cover(&self) {
        CoverLoader::set_placeholder(&self.cover);
        self.reset_cover_swell();
    }

    pub fn set_on_title_click<F: Fn() + 'static>(&self, f: F) {
        *self.on_title_click.borrow_mut() = Some(Rc::new(f));
    }

    /// Registers the GRID-5 callback for cover link activation.
    pub fn connect_cover_clicked<F: Fn() + 'static>(&self, f: F) {
        *self.on_cover_click.borrow_mut() = Some(Rc::new(f));
    }

    /// Registers a callback invoked when the user clicks the artist label.
    pub fn connect_artist_clicked<F: Fn() + 'static>(&self, f: F) {
        *self.on_artist_click.borrow_mut() = Some(Rc::new(f));
    }

    /// Shows `title`/`artist` and starts their shared 250 ms crossfade.
    pub fn set_track(&self, title: &str, artist: &str) {
        self.title_button
            .update_property(&[gtk4::accessible::Property::Label(title)]);
        self.artist_button
            .update_property(&[gtk4::accessible::Property::Label(artist)]);
        self.artist_button.set_sensitive(!artist.trim().is_empty());
        self.cover_button
            .update_property(&[gtk4::accessible::Property::Label(&strings::text(
                strings::REVEAL_PLAYING_ALBUM,
            ))]);
        self.animate_track_change(title, artist);
    }

    /// 250 ms opacity crossfade: fade out cover + labels, swap text, fade in.
    /// The cover and metadata share one transition.
    fn animate_track_change(&self, title: &str, artist: &str) {
        let generation = self.track_animation_generation.get().wrapping_add(1);
        self.track_animation_generation.set(generation);
        let title = title.to_string();
        let artist = artist.to_string();
        let title_label = self.title_label.clone();
        let artist_label = self.artist_label.clone();
        let cover = self.cover.clone();
        let animation_slot = self.current_track_animation.clone();
        let animation_generation = self.track_animation_generation.clone();

        let fade_out_target = libadwaita::CallbackAnimationTarget::new({
            let title_label = title_label.clone();
            let artist_label = artist_label.clone();
            let cover = cover.clone();
            move |value| {
                title_label.set_opacity(value);
                artist_label.set_opacity(value);
                cover.set_opacity(value);
            }
        });
        let fade_out = motion::timed(
            &self.title_label,
            1.0,
            0.0,
            motion::STANDARD,
            fade_out_target,
        );

        fade_out.connect_done({
            let title_label = title_label.clone();
            let artist_label = artist_label.clone();
            let cover = cover.clone();
            move |_| {
                title_label.set_text(&title);
                artist_label.set_text(&artist);

                if animation_generation.get() != generation {
                    title_label.set_opacity(1.0);
                    artist_label.set_opacity(1.0);
                    cover.set_opacity(1.0);
                    return;
                }

                let fade_in_target = libadwaita::CallbackAnimationTarget::new({
                    let title_label = title_label.clone();
                    let artist_label = artist_label.clone();
                    let cover = cover.clone();
                    move |value| {
                        title_label.set_opacity(value);
                        artist_label.set_opacity(value);
                        cover.set_opacity(value);
                    }
                });
                let fade_in =
                    motion::timed(&title_label, 0.0, 1.0, motion::STANDARD, fade_in_target);
                fade_in.set_duration(motion::half(motion::STANDARD));
                motion::replace_animation(&animation_slot, fade_in.clone());
                fade_in.play();
            }
        });
        fade_out.set_duration(motion::half(motion::STANDARD));
        motion::replace_animation(&self.current_track_animation, fade_out.clone());
        fade_out.play();
    }

    /// Clears the track labels back to empty when playback stops.
    pub fn clear_track(&self) {
        let generation = self.track_animation_generation.get().wrapping_add(1);
        self.track_animation_generation.set(generation);
        let previous = self.current_track_animation.borrow_mut().take();
        if let Some(previous) = previous {
            previous.skip();
        }
        self.title_label.set_text("");
        self.artist_label.set_text("");
        self.artist_button.set_sensitive(false);
        self.clear_cover();
    }
}
