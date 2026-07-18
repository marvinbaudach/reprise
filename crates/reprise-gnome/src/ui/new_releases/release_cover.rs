//! Lazy release-group covers with an immediate, spinner-free fallback tile.

#![allow(dead_code)] // Constructed by the popover and digest tasks that follow NR-2.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::{artist_avatar, one_shot_task};

const DEFAULT_ACCENT: (u8, u8, u8) = (53, 132, 228);

pub(in crate::ui) fn fallback_accent_for_artist(
    conn: &rusqlite::Connection,
    artist: &str,
) -> Option<String> {
    let track_path = reprise_core::artist_news::most_played_album_track_path(conn, artist)
        .ok()
        .flatten()?;
    let source = reprise_core::cover::resolve_source(&track_path)?;
    let thumbnail =
        reprise_core::cover::thumbnail(&source, reprise_core::cover::ThumbnailSize::Portrait)
            .ok()?;
    let accent = crate::ui::style::cover_accent::accent_from_cover_file(&thumbnail)?;
    Some(format!("#{:02X}{:02X}{:02X}", accent.r, accent.g, accent.b))
}

pub(in crate::ui) struct LazyReleaseCover {
    root: gtk4::Overlay,
}

impl LazyReleaseCover {
    pub(in crate::ui) fn new(
        release_group_mbid: &str,
        artist: &str,
        accent: &str,
        edge: i32,
    ) -> Self {
        let root = gtk4::Overlay::new();
        root.set_size_request(edge, edge);
        root.add_css_class("new-release-cover");

        let (red, green, blue) = parse_accent(accent).unwrap_or(DEFAULT_ACCENT);
        let background = gtk4::DrawingArea::new();
        background.set_content_width(edge);
        background.set_content_height(edge);
        background.set_draw_func(move |_, context, width, height| {
            context.set_source_rgb(
                f64::from(red) / 255.0,
                f64::from(green) / 255.0,
                f64::from(blue) / 255.0,
            );
            context.rectangle(0.0, 0.0, f64::from(width), f64::from(height));
            let _ = context.fill();
        });
        root.set_child(Some(&background));

        let initials = gtk4::Label::new(Some(&artist_avatar::initials(artist)));
        initials.set_halign(gtk4::Align::Center);
        initials.set_valign(gtk4::Align::Center);
        initials.add_css_class("title-3");
        root.add_overlay(&initials);

        let picture = gtk4::Picture::new();
        picture.set_content_fit(gtk4::ContentFit::Cover);
        picture.set_can_shrink(true);
        picture.set_visible(false);
        root.add_overlay(&picture);

        wire_lazy_fetch(&root, &picture, release_group_mbid);
        Self { root }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Overlay {
        &self.root
    }
}

fn wire_lazy_fetch(root: &gtk4::Overlay, picture: &gtk4::Picture, release_group_mbid: &str) {
    let started = Rc::new(Cell::new(false));
    let release_group_mbid = release_group_mbid.to_string();
    let picture = picture.clone();
    root.connect_map(move |_| {
        if started.replace(true) {
            return;
        }
        let release_group_mbid = release_group_mbid.clone();
        let Ok(receiver) = one_shot_task::spawn("reprise-release-cover", move || {
            reprise_core::cover_download::fetch_release_group_cover(&release_group_mbid)
        }) else {
            return;
        };
        let picture = picture.clone();
        gtk4::glib::spawn_future_local(async move {
            if let Ok(reprise_core::cover_download::ReleaseGroupCover::Image(path)) =
                receiver.recv().await
            {
                picture.set_filename(Some(path));
                picture.set_visible(true);
            }
        });
    });
}

fn parse_accent(value: &str) -> Option<(u8, u8, u8)> {
    let value = value.strip_prefix('#')?;
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some((
        u8::from_str_radix(&value[0..2], 16).ok()?,
        u8::from_str_radix(&value[2..4], 16).ok()?,
        u8::from_str_radix(&value[4..6], 16).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_fallback_accent_parses_without_a_widget_or_display() {
        assert_eq!(parse_accent("#1234AB"), Some((0x12, 0x34, 0xAB)));
        assert_eq!(parse_accent("broken"), None);
    }
}
