//! Static, preblurred Now Playing cover texture behind the Albums view.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::cover::{blur_reduced_thumbnail, ThumbnailSize};

use crate::ui::cover_loader::CoverLoader;

const BLUR_SIGMA: f32 = 6.0;
pub(in crate::ui) const CSS_CLASS: &str = "album-now-playing-glow";

#[derive(Clone)]
pub(in crate::ui) struct AlbumGlow {
    picture: gtk4::Picture,
    generation: Rc<Cell<u64>>,
    cover_loader: Rc<CoverLoader>,
}

impl AlbumGlow {
    pub(in crate::ui) fn new(cover_loader: Rc<CoverLoader>) -> Self {
        let picture = gtk4::Picture::new();
        picture.add_css_class(CSS_CLASS);
        picture.set_content_fit(gtk4::ContentFit::Cover);
        picture.set_can_shrink(true);
        picture.set_can_target(false);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        picture.set_visible(false);

        let picture_for_contrast = picture.clone();
        libadwaita::StyleManager::default().connect_high_contrast_notify(move |manager| {
            picture_for_contrast.set_visible(
                !manager.is_high_contrast() && picture_for_contrast.paintable().is_some(),
            );
        });

        Self {
            picture,
            generation: Rc::new(Cell::new(0)),
            cover_loader,
        }
    }

    pub(in crate::ui) fn set_track_path(&self, track_path: Option<&str>) {
        let token = self.generation.get().wrapping_add(1);
        self.generation.set(token);
        self.picture.set_paintable(gtk4::gdk::Paintable::NONE);
        self.picture.set_visible(false);
        let Some(track_path) = track_path else {
            return;
        };

        // Reuse the established cover resolver/downloader at the final 32 px
        // size, then blur that already-reduced thumbnail once off-thread.
        let staging = gtk4::Image::new();
        let picture = self.picture.downgrade();
        let generation = self.generation.clone();
        self.cover_loader.load_into_with_resolution(
            &staging,
            track_path,
            ThumbnailSize::Glow,
            token,
            &self.generation,
            move |resolved| {
                let Some(resolved) = resolved else {
                    return;
                };
                let picture = picture.clone();
                let generation = generation.clone();
                glib::spawn_future_local(async move {
                    let texture_path = gio::spawn_blocking(move || {
                        blur_reduced_thumbnail(&resolved, BLUR_SIGMA).ok()
                    })
                    .await
                    .ok()
                    .flatten();
                    if generation.get() != token {
                        return;
                    }
                    let (Some(picture), Some(texture_path)) = (picture.upgrade(), texture_path)
                    else {
                        return;
                    };
                    let Ok(texture) = gtk4::gdk::Texture::from_filename(texture_path) else {
                        return;
                    };
                    picture.set_paintable(Some(&texture));
                    picture.set_visible(!libadwaita::StyleManager::default().is_high_contrast());
                });
            },
        );
    }

    pub(in crate::ui) fn picture(&self) -> &gtk4::Picture {
        &self.picture
    }

    #[cfg(test)]
    pub(in crate::ui) fn generation(&self) -> u64 {
        self.generation.get()
    }
}

pub(in crate::ui) fn css() -> String {
    format!(
        ".{CSS_CLASS} {{ opacity: 0.22; }}\n\
         .album-view-content, .album-view-content scrolledwindow, \
         .album-view-content .library-grid {{ background-color: transparent; }}"
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn grid_7_glow_css_is_static_subtle_and_cover_independent() {
        let css = super::css();

        assert!(css.contains(".album-now-playing-glow { opacity: 0.22;"));
        assert!(css.contains("background-color: transparent"));
        assert!(!css.contains("blur("));
        assert!(!css.contains("@reprise_player_accent"));
    }
}
