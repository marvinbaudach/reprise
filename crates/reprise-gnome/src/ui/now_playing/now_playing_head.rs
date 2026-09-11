use gtk4::prelude::*;

use super::cover_bloom;
use super::cover_cloud;
use super::cover_loader::CoverLoader;
use crate::ui::cover_lift::CoverLift;
use crate::ui::style::tokens;

pub(super) struct HeadWidgets {
    pub(super) head_group: gtk4::Overlay,
    #[cfg(test)]
    pub(super) head_column: gtk4::Box,
    #[cfg(test)]
    pub(super) artwork_overlay: gtk4::Overlay,
    #[cfg(test)]
    pub(super) artwork_band: gtk4::Box,
    #[cfg(test)]
    pub(super) head: gtk4::Box,
    #[cfg(test)]
    pub(super) metadata: gtk4::Box,
    pub(super) bloom: cover_bloom::CoverBloom,
    pub(super) cloud: cover_cloud::CoverCloud,
    pub(super) cover_stack: gtk4::Stack,
    pub(super) external_cover: gtk4::Box,
    pub(super) cover: gtk4::Image,
    pub(super) outgoing_cover: gtk4::Image,
    pub(super) title: gtk4::Label,
    pub(super) artist: gtk4::Label,
    pub(super) album: gtk4::Label,
}

pub(super) fn build_head() -> HeadWidgets {
    let cover = gtk4::Image::builder()
        .pixel_size(tokens::NOW_PLAYING_COVER_SIZE)
        .width_request(tokens::NOW_PLAYING_COVER_SIZE)
        .height_request(tokens::NOW_PLAYING_COVER_SIZE)
        .build();
    cover.set_accessible_role(gtk4::AccessibleRole::Link);
    cover.add_css_class("reprise-now-playing-cover");
    CoverLoader::set_placeholder(&cover);
    let outgoing_cover = gtk4::Image::builder()
        .pixel_size(tokens::NOW_PLAYING_COVER_SIZE)
        .width_request(tokens::NOW_PLAYING_COVER_SIZE)
        .height_request(tokens::NOW_PLAYING_COVER_SIZE)
        .can_target(false)
        .opacity(0.0)
        .visible(false)
        .build();
    outgoing_cover.add_css_class("reprise-now-playing-cover");
    outgoing_cover.set_accessible_role(gtk4::AccessibleRole::Presentation);
    let cover_transition = gtk4::Overlay::new();
    cover_transition.set_child(Some(&cover));
    cover_transition.add_overlay(&outgoing_cover);
    let cover_lift = CoverLift::new_still(&cover_transition, tokens::NOW_PLAYING_COVER_SIZE);
    let external_cover = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    external_cover.set_size_request(
        tokens::NOW_PLAYING_COVER_SIZE,
        tokens::NOW_PLAYING_COVER_SIZE,
    );
    external_cover.set_halign(gtk4::Align::Center);
    external_cover.set_valign(gtk4::Align::Center);
    let cover_stack = gtk4::Stack::new();
    cover_stack.add_named(cover_lift.widget(), Some("track"));
    cover_stack.add_named(&external_cover, Some("external"));
    cover_stack.set_visible_child_name("track");

    let title = gtk4::Label::builder()
        .xalign(0.5)
        .justify(gtk4::Justification::Center)
        .wrap(false)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    title.set_accessible_role(gtk4::AccessibleRole::Link);
    title.add_css_class("reprise-now-playing-title");
    let artist = gtk4::Label::builder()
        .xalign(0.5)
        .justify(gtk4::Justification::Center)
        .wrap(false)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    artist.set_accessible_role(gtk4::AccessibleRole::Link);
    artist.add_css_class("reprise-now-playing-artist");
    let album = gtk4::Label::builder()
        .xalign(0.5)
        .justify(gtk4::Justification::Center)
        .wrap(false)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    album.set_accessible_role(gtk4::AccessibleRole::Link);
    album.add_css_class("reprise-now-playing-album");

    let subtitle_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    subtitle_row.set_halign(gtk4::Align::Center);
    subtitle_row.add_css_class("reprise-now-playing-subtitle-row");
    subtitle_row.append(&artist);
    subtitle_row.append(&album);

    let metadata = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    metadata.add_css_class("reprise-now-playing-metadata");
    metadata.set_halign(gtk4::Align::Fill);
    metadata.append(&title);
    metadata.append(&subtitle_row);
    let head = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    head.add_css_class("reprise-now-playing-head");
    head.set_halign(gtk4::Align::Center);
    // Start alignment keeps the cover at the named head inset rather than
    // re-centering it when the title block leaves this box.
    head.set_valign(gtk4::Align::Start);
    head.append(&cover_stack);

    let glow = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    glow.add_css_class("reprise-now-playing-glow");
    glow.set_can_target(false);
    let artwork_band = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    artwork_band.set_height_request(tokens::NOW_PLAYING_ARTWORK_BAND);
    artwork_band.set_can_target(false);
    let artwork_overlay = gtk4::Overlay::new();
    artwork_overlay.set_child(Some(&artwork_band));
    let bloom = cover_bloom::CoverBloom::new();
    let cloud = cover_cloud::CoverCloud::new();
    // Within the artwork band, bottom to top: the transparent geometry band,
    // the blurred cover, the drifting clouds (which own the scrim), then the
    // cover. Metadata is a sibling below this overlay, so no cover-derived
    // pixel can paint behind it — and the cover is above both moving layers,
    // which is what keeps it from ever turning or growing with them.
    artwork_overlay.add_overlay(bloom.widget());
    artwork_overlay.add_overlay(cloud.widget());
    artwork_overlay.add_overlay(&head);
    let head_column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    head_column.append(&artwork_overlay);
    head_column.append(&metadata);
    let head_group = gtk4::Overlay::new();
    head_group.set_child(Some(&glow));
    head_group.add_overlay(&head_column);
    head_group.set_measure_overlay(&head_column, true);

    HeadWidgets {
        head_group,
        #[cfg(test)]
        head_column,
        #[cfg(test)]
        artwork_overlay,
        #[cfg(test)]
        artwork_band,
        #[cfg(test)]
        head,
        #[cfg(test)]
        metadata,
        bloom,
        cloud,
        cover_stack,
        external_cover,
        cover,
        outgoing_cover,
        title,
        artist,
        album,
    }
}

#[cfg(test)]
#[path = "now_playing_head_tests.rs"]
mod tests;
