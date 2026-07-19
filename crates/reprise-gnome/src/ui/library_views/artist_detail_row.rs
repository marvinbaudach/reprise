//! Cell and section-shell builders for the Artists detail pane: the albums and
//! top-tracks section containers, plus the album-row cards and top-track rows
//! that fill them. Split from `artist_detail_pane.rs` (which owns the pane
//! struct, hero, and rebuild orchestration) so each file stays cohesive.
//!
//! The cell builders take the shared generation `Cell` used to guard async
//! cover loads, so a fast artist switch (which bumps the generation) can't
//! apply a stale cover to a recycled cell — the pattern `album_view.rs` uses.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;
use reprise_core::library::artist_detail::ArtistTopTrack;
use reprise_core::queries::ArtistAlbum;

use crate::ui::artist_detail_pane::{AlbumCallback, TrackCallback};
use crate::ui::cover_loader::CoverLoader;
use crate::ui::playing_marker;
use crate::ui::strings;

/// Album cover edge in the albums row.
const ALBUM_COVER_SIZE: i32 = 150;
/// Track cover edge in the top-tracks list.
const TRACK_COVER_SIZE: i32 = 30;

/// A realized top-track row plus the `track_id` it displays, so the pane's
/// `set_now_playing_track` can light the matching row's mini-EQ without
/// walking the widget tree.
pub(in crate::ui) struct TopTrackRow {
    pub(in crate::ui) track_id: i64,
    /// The shared playing marker; visibility is the only
    /// per-row control — shown on the now-playing track's row.
    pub(in crate::ui) eq: gtk4::Box,
}

impl TopTrackRow {
    /// Shows this row's mini-EQ iff `now_playing` is this row's track.
    pub(in crate::ui) fn set_now_playing(&self, now_playing: Option<i64>) {
        self.eq.set_visible(now_playing == Some(self.track_id));
    }
}

/// Builds one album card (cover + title + "YEAR · N tracks"). Clicking it
/// invokes `on_activate(album, artist)`. v1: the cover and the whole card both
/// route to the same album source — the play-vs-open distinction is deferred.
pub(in crate::ui) fn build_album_card(
    cover_loader: &Rc<CoverLoader>,
    generation: &Rc<Cell<u64>>,
    token: u64,
    album: ArtistAlbum,
    artist: String,
    on_activate: AlbumCallback,
) -> gtk4::Button {
    let image = gtk4::Image::builder()
        .pixel_size(ALBUM_COVER_SIZE)
        .width_request(ALBUM_COVER_SIZE)
        .height_request(ALBUM_COVER_SIZE)
        .build();
    image.add_css_class("artist-album-cover");
    cover_loader.load_into(
        &image,
        &album.representative_path,
        ThumbnailSize::Grid,
        token,
        generation,
    );

    let title = card_label(&album.album, "artist-album-title");
    let meta = card_label(
        &strings::artist_album_meta(album.year, album.track_count),
        "artist-album-meta",
    );
    meta.add_css_class("dim-label");

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    content.append(&image);
    content.append(&title);
    content.append(&meta);

    let button = gtk4::Button::builder()
        .child(&content)
        .has_frame(false)
        .tooltip_text(&album.album)
        .build();
    button.add_css_class("artist-album-card");
    button.connect_clicked(move |_| {
        let callback = on_activate.borrow().clone();
        if let Some(callback) = callback {
            callback(album.clone(), artist.clone());
        }
    });
    button
}

/// Builds one top-track row: rank, small cover, title/album stack, play count,
/// duration, and a trailing `EqBars`. Returns the row widget plus its
/// [`TopTrackRow`] handle (for the now-playing indicator).
pub(in crate::ui) fn build_top_track_row(
    cover_loader: &Rc<CoverLoader>,
    generation: &Rc<Cell<u64>>,
    token: u64,
    rank: usize,
    track: &ArtistTopTrack,
    on_activate: TrackCallback,
    artist: String,
) -> (gtk4::Box, TopTrackRow) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    row.add_css_class("artist-top-track");

    let rank_label = gtk4::Label::new(Some(&rank.to_string()));
    rank_label.set_xalign(0.5);
    rank_label.add_css_class("artist-top-track-rank");
    rank_label.add_css_class("dim-label");
    rank_label.set_width_request(20);
    row.append(&rank_label);

    let image = gtk4::Image::builder()
        .pixel_size(TRACK_COVER_SIZE)
        .width_request(TRACK_COVER_SIZE)
        .height_request(TRACK_COVER_SIZE)
        .build();
    image.add_css_class("artist-top-track-cover");
    cover_loader.load_into(
        &image,
        &track.track_path,
        ThumbnailSize::List,
        token,
        generation,
    );
    row.append(&image);

    let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
    text_box.set_valign(gtk4::Align::Center);
    text_box.set_hexpand(true);
    let title = card_label(&track.title, "artist-top-track-title");
    let album = card_label(&track.album, "artist-top-track-album");
    album.add_css_class("dim-label");
    text_box.append(&title);
    text_box.append(&album);
    row.append(&text_box);

    let plays = gtk4::Label::new(Some(&strings::artist_counts_plays(track.play_count)));
    plays.set_xalign(1.0);
    plays.add_css_class("artist-top-track-plays");
    plays.add_css_class("dim-label");
    row.append(&plays);

    let duration = gtk4::Label::new(Some(&reprise_core::format::format_duration(
        track.duration_ms,
    )));
    duration.set_xalign(1.0);
    duration.add_css_class("artist-top-track-duration");
    duration.add_css_class("dim-label");
    row.append(&duration);

    let eq = playing_marker::build();
    eq.set_visible(false);
    row.append(&eq);

    // Double-click plays this track in the artist's context. A plain `Box` in a
    // vertical list (not a `ColumnView` cell), so the `GestureClick` is
    // delivered reliably — no row machinery competes for the sequence.
    let click = gtk4::GestureClick::new();
    click.set_button(gtk4::gdk::BUTTON_PRIMARY);
    let track_id = track.track_id;
    click.connect_released(move |gesture, n_press, _x, _y| {
        if n_press != 2 {
            return;
        }
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let callback = on_activate.borrow().clone();
        if let Some(callback) = callback {
            callback(track_id, artist.clone());
        }
    });
    row.add_controller(click);

    let handle = TopTrackRow {
        track_id: track.track_id,
        eq,
    };
    (row, handle)
}

/// Builds the albums section shell — title, FlowBox, empty hint, Show-all.
/// Returns `(section, flow, hint, show_all)`; the pane fills the flow and
/// toggles the hint/button.
pub(in crate::ui) fn build_albums_section(
    albums_per_row: u32,
) -> (gtk4::Box, gtk4::FlowBox, gtk4::Label, gtk4::Button) {
    let section = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    section.add_css_class("artist-albums-section");
    section.append(&section_title(&strings::text(
        strings::ARTIST_DETAIL_ALBUMS,
    )));

    let flow = gtk4::FlowBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .column_spacing(16)
        .row_spacing(16)
        .min_children_per_line(1)
        .max_children_per_line(albums_per_row)
        .homogeneous(false)
        .valign(gtk4::Align::Start)
        .build();
    flow.add_css_class("artist-albums");
    section.append(&flow);

    let hint = gtk4::Label::new(Some(&strings::text(strings::ARTIST_DETAIL_NO_ALBUMS)));
    hint.set_xalign(0.0);
    hint.add_css_class("artist-albums-hint");
    hint.add_css_class("dim-label");
    hint.set_visible(false);
    section.append(&hint);

    let show_all = flat_show_all_button(
        &strings::text(strings::ARTIST_DETAIL_SHOW_ALL),
        "artist-albums-show-all",
    );
    section.append(&show_all);
    (section, flow, hint, show_all)
}

/// Builds the top-tracks section shell — title, rows box, Show-all-tracks.
/// Returns `(section, rows, show_all)`.
pub(in crate::ui) fn build_top_section() -> (gtk4::Box, gtk4::Box, gtk4::Button) {
    let section = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    section.add_css_class("artist-top-section");
    section.append(&section_title(&strings::text(
        strings::ARTIST_DETAIL_TOP_TRACKS,
    )));

    let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    rows.add_css_class("artist-top-tracks");
    section.append(&rows);

    let show_all = flat_show_all_button("", "artist-top-show-all");
    section.append(&show_all);
    (section, rows, show_all)
}

/// A left-aligned, initially-hidden flat "show all" button.
fn flat_show_all_button(label: &str, css_class: &str) -> gtk4::Button {
    let button = gtk4::Button::with_label(label);
    button.add_css_class(css_class);
    button.add_css_class("flat");
    button.set_halign(gtk4::Align::Start);
    button.set_visible(false);
    button
}

fn section_title(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("artist-section-title");
    label
}

/// A left-aligned, ellipsized card label with the given style class.
fn card_label(text: &str, css_class: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.add_css_class(css_class);
    label
}
