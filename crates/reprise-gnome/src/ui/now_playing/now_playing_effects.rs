//! Track-content rendering and the cover transition for `NowPlayingPanel`,
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
use crate::ui::playback::external_media::ExternalMedia;
use crate::ui::style::tokens;

impl super::NowPlayingPanel {
    pub(super) fn render_track(&self) {
        self.render_track_with_cover_resolution(|_| {});
    }

    fn render_track_with_cover_resolution(
        &self,
        on_cover_resolved: impl Fn(Option<std::path::PathBuf>) + 'static,
    ) {
        let track = self.loaded_track.borrow().clone();
        let external = self.external_snapshot.borrow().clone();
        let presentation = panel_presentation_with_external(
            track.as_ref(),
            external.as_ref(),
            self.playback_state.get(),
        );
        self.widgets.title.set_label(&presentation.title);
        let (artist, album) = external.as_ref().map_or_else(
            || {
                track.as_ref().map_or(("", ""), |track| {
                    (track.artist.as_str(), track.album.as_str())
                })
            },
            |_| (presentation.subtitle.as_str(), ""),
        );
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
        if let Some(external) = external {
            self.widgets.cover_stack.set_visible_child_name("external");
            while let Some(child) = self.widgets.external_cover.first_child() {
                self.widgets.external_cover.remove(&child);
            }
            let fallback_icon = match external.media {
                ExternalMedia::Podcast { .. } => "audio-input-microphone-symbolic",
                ExternalMedia::Radio { .. } => "audio-volume-high-symbolic",
            };
            let images_allowed = reprise_core::online_sources::network_allowed(
                &self.conn,
                &reprise_core::modules::SOURCE_IMAGES_MODULE,
            )
            .unwrap_or(false);
            let source_image = crate::ui::podcasts::source_image::SourceImage::new(
                external.art_url.as_deref(),
                fallback_icon,
                tokens::NOW_PLAYING_COVER_SIZE,
                images_allowed,
            );
            source_image
                .widget()
                .add_css_class("reprise-now-playing-cover");
            self.widgets.external_cover.append(source_image.widget());
            self.widgets.visualizer.set_cover(None);
            self.widgets.bloom.set_cover(None, generation);
            self.widgets.shimmer.set_cover(None, generation);
            on_cover_resolved(None);
            return;
        }
        self.widgets.cover_stack.set_visible_child_name("track");
        CoverLoader::set_placeholder(&self.widgets.cover);
        // Revert the visualizer's cover-derived accent up front, same reason
        // `PlayerController::sync_cover`'s `reset_cover_accent` does for the
        // bar: without this, a track with no (or slow-to-decode) cover would
        // leave the previous track's accent lingering in the engine.
        self.widgets.visualizer.set_cover(None);
        self.widgets.bloom.set_cover(None, generation);
        self.widgets.shimmer.set_cover(None, generation);
        if let Some(track) = track {
            let visualizer = self.widgets.visualizer.clone();
            let bloom = self.widgets.bloom.clone();
            let shimmer = self.widgets.shimmer.clone();
            let cover_widget = self.widgets.cover.clone();
            self.cover_loader.load_into_with_resolution(
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
                    if let Some(resolved_path) = resolved_path.as_ref() {
                        let texture = cover_widget
                            .paintable()
                            .and_downcast::<gtk4::gdk::Texture>()
                            .or_else(|| gtk4::gdk::Texture::from_filename(resolved_path).ok());
                        visualizer.set_cover(texture.as_ref());
                        bloom.set_cover(texture.as_ref(), generation);
                        // Same texture, same generation: the shimmer's disc is
                        // the same blur the bloom lies on, only masked round
                        // and turning. Nothing is decoded twice.
                        shimmer.set_cover(texture.as_ref(), generation);
                    }
                    on_cover_resolved(resolved_path);
                },
            );
        } else {
            on_cover_resolved(None);
        }
    }

    pub(super) fn animate_cover_change(self: &Rc<Self>) {
        let visible_cover = if self.cover_transition_active.get() {
            &self.widgets.outgoing_cover
        } else {
            &self.widgets.cover
        };
        let outgoing_paintable = visible_cover.paintable();
        let outgoing_icon = visible_cover.icon_name();
        self.cancel_cover_animation();
        let generation = self.cover_animation_generation.get().wrapping_add(1);
        self.cover_animation_generation.set(generation);
        self.cover_transition_active.set(true);
        if let Some(paintable) = outgoing_paintable {
            self.widgets.outgoing_cover.set_paintable(Some(&paintable));
        } else {
            self.widgets
                .outgoing_cover
                .set_icon_name(outgoing_icon.as_deref());
        }
        self.widgets.outgoing_cover.set_visible(true);
        self.widgets.outgoing_cover.set_opacity(1.0);

        let panel = Rc::downgrade(self);
        self.render_track_with_cover_resolution(move |_| {
            let Some(panel) = panel.upgrade() else {
                return;
            };
            if panel.cover_animation_generation.get() == generation {
                panel.start_cover_fade(generation);
            }
        });
    }

    fn start_cover_fade(self: &Rc<Self>, generation: u64) {
        let target = adw::CallbackAnimationTarget::new({
            let cover = self.widgets.outgoing_cover.clone();
            move |value| cover.set_opacity(value)
        });
        let transition = crate::ui::motion::timed(
            &self.widgets.outgoing_cover,
            1.0,
            0.0,
            crate::ui::motion::STANDARD,
            target,
        );
        let panel = Rc::downgrade(self);
        transition.connect_done(move |_| {
            let Some(panel) = panel.upgrade() else {
                return;
            };
            if panel.cover_animation_generation.get() != generation {
                return;
            }
            panel.cover_animation.borrow_mut().take();
            panel.cover_transition_active.set(false);
            panel.widgets.outgoing_cover.set_opacity(0.0);
            panel.widgets.outgoing_cover.set_visible(false);
        });
        *self.cover_animation.borrow_mut() = Some(transition.clone());
        transition.play();
    }

    pub(super) fn cancel_cover_animation(&self) {
        self.cover_animation_generation
            .set(self.cover_animation_generation.get().wrapping_add(1));
        let previous = self.cover_animation.borrow_mut().take();
        if let Some(animation) = previous {
            animation.skip();
        }
        self.cover_transition_active.set(false);
        self.widgets.outgoing_cover.set_opacity(0.0);
        self.widgets.outgoing_cover.set_visible(false);
    }

    #[cfg(test)]
    pub(super) fn has_cover_transition(&self) -> bool {
        self.cover_transition_active.get()
    }
}
