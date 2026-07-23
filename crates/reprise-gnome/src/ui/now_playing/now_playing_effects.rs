//! Track-content rendering and the crossfade animation for `NowPlayingPanel`,
//! split out of `now_playing.rs` to keep that file under the 800-line cap. These
//! stay `NowPlayingPanel` methods — a child-module `impl super::NowPlayingPanel`
//! block reaches the panel's (and `PanelWidgets`') private fields as a
//! descendant — and are exposed `pub(super)` so `now_playing.rs` can still call
//! them.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AnimationExt;
use reprise_core::cover::ThumbnailSize;

use crate::ui::now_playing::cover_loader::CoverLoader;
use crate::ui::now_playing::panel_state::*;

impl super::NowPlayingPanel {
    pub(super) fn render_track(&self) {
        let track = self.loaded_track.borrow().clone();
        let presentation = panel_presentation(track.as_ref(), self.playback_state.get());
        self.widgets.title.set_label(&presentation.title);
        let (artist, album) = track.as_ref().map_or(("", ""), |track| {
            (track.artist.as_str(), track.album.as_str())
        });
        self.widgets.artist.set_label(artist);
        self.widgets.album.set_label(album);
        self.widgets.artist.set_visible(!artist.trim().is_empty());
        self.widgets.album.set_visible(!album.trim().is_empty());
        if presentation.idle {
            self.widgets.stage.add_css_class("reprise-now-playing-idle");
        } else {
            self.widgets
                .stage
                .remove_css_class("reprise-now-playing-idle");
        }
        let generation = self.cover_generation.get().wrapping_add(1);
        self.cover_generation.set(generation);
        CoverLoader::set_placeholder(&self.widgets.cover);
        // Revert the visualizer's cover-derived accent up front, same reason
        // `PlayerController::sync_cover`'s `reset_cover_accent` does for the
        // bar: without this, a track with no (or slow-to-decode) cover would
        // leave the previous track's accent lingering in the engine.
        self.widgets.visualizer.set_cover(None);
        if let Some(track) = track {
            let visualizer = self.widgets.visualizer.clone();
            let cover_widget = self.widgets.cover.clone();
            self.cover_loader.load_into_with_path(
                &self.widgets.cover,
                &track.path,
                ThumbnailSize::Full,
                generation,
                &self.cover_generation,
                move |resolved_path| {
                    // `load_target` already set `cover_widget`'s paintable to
                    // the decoded texture before invoking this callback (same
                    // generation), so reuse it instead of decoding the
                    // full-resolution file a second time. Only fall back to a
                    // fresh decode if the paintable isn't a texture for some
                    // reason (e.g. still showing the placeholder icon).
                    let texture = cover_widget
                        .paintable()
                        .and_downcast::<gtk4::gdk::Texture>()
                        .or_else(|| gtk4::gdk::Texture::from_filename(&resolved_path).ok());
                    visualizer.set_cover(texture.as_ref());
                },
            );
        }
    }

    pub(super) fn animate_track_change(self: &Rc<Self>) {
        self.cancel_track_animation();
        let generation = self.track_animation_generation.get().wrapping_add(1);
        self.track_animation_generation.set(generation);
        let target = adw::CallbackAnimationTarget::new({
            let content = self.widgets.track_content.clone();
            move |value| content.set_opacity(value)
        });
        let fade_out = crate::ui::motion::timed(
            &self.widgets.track_content,
            self.widgets.track_content.opacity(),
            0.0,
            crate::ui::motion::STANDARD,
            target,
        );
        fade_out.set_duration(crate::ui::motion::half(crate::ui::motion::STANDARD));
        let panel = Rc::downgrade(self);
        fade_out.connect_done(move |_| {
            let Some(panel) = panel.upgrade() else {
                return;
            };
            if panel.track_animation_generation.get() != generation {
                return;
            }
            panel.render_track();
            let target = adw::CallbackAnimationTarget::new({
                let content = panel.widgets.track_content.clone();
                move |value| content.set_opacity(value)
            });
            let fade_in = crate::ui::motion::timed(
                &panel.widgets.track_content,
                0.0,
                1.0,
                crate::ui::motion::STANDARD,
                target,
            );
            fade_in.set_duration(crate::ui::motion::half(crate::ui::motion::STANDARD));
            let panel_for_done = Rc::downgrade(&panel);
            fade_in.connect_done(move |_| {
                let Some(panel) = panel_for_done.upgrade() else {
                    return;
                };
                if panel.track_animation_generation.get() == generation {
                    panel.track_animation.borrow_mut().take();
                    panel.widgets.track_content.set_opacity(1.0);
                }
            });
            *panel.track_animation.borrow_mut() = Some(fade_in.clone());
            fade_in.play();
        });
        *self.track_animation.borrow_mut() = Some(fade_out.clone());
        fade_out.play();
    }

    pub(super) fn cancel_track_animation(&self) {
        self.track_animation_generation
            .set(self.track_animation_generation.get().wrapping_add(1));
        if let Some(animation) = self.track_animation.borrow_mut().take() {
            animation.pause();
        }
    }

    #[cfg(test)]
    pub(super) fn has_track_animation(&self) -> bool {
        self.track_animation.borrow().is_some()
    }
}
