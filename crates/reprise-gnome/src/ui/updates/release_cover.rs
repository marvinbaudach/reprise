//! Lazy release-group covers with an immediate, spinner-free fallback tile.

#![allow(dead_code)] // Constructed by the popover that follows NR-2.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::{artist_avatar, one_shot_task};

pub(in crate::ui) struct LazyReleaseCover {
    root: gtk4::Overlay,
}

impl LazyReleaseCover {
    pub(in crate::ui) fn new(release_group_mbid: &str, artist: &str, edge: i32) -> Self {
        let root = gtk4::Overlay::new();
        root.set_size_request(edge, edge);
        root.add_css_class("new-release-cover");

        let background = gtk4::DrawingArea::new();
        background.set_content_width(edge);
        background.set_content_height(edge);
        background.set_draw_func(move |_, context, width, height| {
            let accent = crate::ui::style::accent::accent_rgba();
            context.set_source_rgb(
                f64::from(accent.red()),
                f64::from(accent.green()),
                f64::from(accent.blue()),
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

        let hairline = gtk4::DrawingArea::new();
        hairline.set_can_target(false);
        hairline.set_draw_func(|_, context, width, height| {
            context.set_source_rgba(1.0, 1.0, 1.0, 0.22);
            context.set_line_width(1.0);
            context.rectangle(
                0.5,
                0.5,
                f64::from(width).max(1.0) - 1.0,
                f64::from(height).max(1.0) - 1.0,
            );
            let _ = context.stroke();
        });
        root.add_overlay(&hairline);

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
