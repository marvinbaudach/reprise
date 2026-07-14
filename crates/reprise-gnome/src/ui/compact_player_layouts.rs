use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::settings::CompactLayout;

use super::cover_loader::CoverLoader;
use super::player_bar::{ICON_NEXT, ICON_PLAY, ICON_PREVIOUS, ICON_REPEAT_ALL, ICON_SHUFFLE};
use super::strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LayoutMetrics {
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) separate_header: bool,
    pub(super) direct_shuffle: bool,
    pub(super) direct_repeat: bool,
}

impl LayoutMetrics {
    pub(super) const fn new(
        width: i32,
        height: i32,
        separate_header: bool,
        direct_shuffle: bool,
        direct_repeat: bool,
    ) -> Self {
        Self {
            width,
            height,
            separate_header,
            direct_shuffle,
            direct_repeat,
        }
    }
}

pub(super) const fn metrics(layout: CompactLayout) -> LayoutMetrics {
    match layout {
        CompactLayout::Cover => LayoutMetrics::new(380, 560, true, false, false),
        CompactLayout::Pill => LayoutMetrics::new(720, 96, false, false, false),
        CompactLayout::Card => LayoutMetrics::new(520, 300, true, false, false),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ControlRole {
    Playback,
    WindowAction,
}

pub(super) const fn control_is_sensitive(role: ControlRole, playback_available: bool) -> bool {
    match role {
        ControlRole::Playback => playback_available,
        ControlRole::WindowAction => true,
    }
}

pub(super) const fn layout_token(layout: CompactLayout) -> &'static str {
    match layout {
        CompactLayout::Cover => "cover",
        CompactLayout::Pill => "pill",
        CompactLayout::Card => "card",
    }
}

pub(super) fn layout_from_token(token: &str) -> Option<CompactLayout> {
    match token {
        "cover" => Some(CompactLayout::Cover),
        "pill" => Some(CompactLayout::Pill),
        "card" => Some(CompactLayout::Card),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MetadataRow {
    Album,
    Year,
}

pub(super) fn visible_detail_rows(
    layout: CompactLayout,
    album: &str,
    year: Option<i32>,
) -> Vec<MetadataRow> {
    let mut rows = Vec::with_capacity(2);
    if matches!(layout, CompactLayout::Cover | CompactLayout::Card) && !album.trim().is_empty() {
        rows.push(MetadataRow::Album);
    }
    if layout == CompactLayout::Card && year.is_some() {
        rows.push(MetadataRow::Year);
    }
    rows
}

pub(super) fn is_drag_region(region: &str) -> bool {
    region == "metadata"
}

pub(super) struct LayoutWidgets {
    pub(super) layout: CompactLayout,
    pub(super) root: gtk4::Widget,
    pub(super) cover: gtk4::Image,
    pub(super) title: gtk4::Label,
    pub(super) artist: gtk4::Label,
    pub(super) album: gtk4::Label,
    pub(super) year: gtk4::Label,
    pub(super) previous: gtk4::Button,
    pub(super) play_pause: gtk4::Button,
    pub(super) next: gtk4::Button,
    pub(super) shuffle: Option<gtk4::ToggleButton>,
    pub(super) repeat: Option<gtk4::Button>,
    pub(super) position: gtk4::Label,
    pub(super) duration: gtk4::Label,
    pub(super) scale: gtk4::Scale,
    pub(super) menu: gtk4::Button,
    pub(super) volume_scroll_region: gtk4::Widget,
}

impl LayoutWidgets {
    fn common(layout: CompactLayout, cover_size: i32) -> Self {
        let cover = gtk4::Image::new();
        cover.set_pixel_size(cover_size);
        cover.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::COMPACT_COVER,
        ))]);
        CoverLoader::set_placeholder(&cover);

        let title = metadata_label(strings::COMPACT_TITLE);
        title.add_css_class("heading");
        let artist = metadata_label(strings::COMPACT_ARTIST);
        artist.add_css_class("dim-label");
        let album = metadata_label(strings::COMPACT_ALBUM);
        album.add_css_class("dim-label");
        album.set_visible(false);
        let year = metadata_label(strings::TAG_YEAR);
        year.add_css_class("dim-label");
        year.set_visible(false);

        let previous = icon_button(ICON_PREVIOUS, strings::PREVIOUS);
        let play_pause = icon_button(ICON_PLAY, strings::PLAY);
        play_pause.add_css_class("circular");
        play_pause.add_css_class("suggested-action");
        let next = icon_button(ICON_NEXT, strings::NEXT);
        let metrics = metrics(layout);
        let shuffle = metrics
            .direct_shuffle
            .then(|| toggle_button(ICON_SHUFFLE, strings::SHUFFLE));
        let repeat = metrics
            .direct_repeat
            .then(|| icon_button(ICON_REPEAT_ALL, strings::REPEAT));

        let position = time_label(strings::CURRENT_POSITION);
        let duration = time_label(strings::TOTAL_DURATION);
        let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, None::<&gtk4::Adjustment>);
        scale.set_range(0.0, 1.0);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.set_tooltip_text(Some(&strings::text(strings::PLAYBACK_POSITION)));
        scale.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::PLAYBACK_POSITION,
        ))]);

        let menu = icon_button("open-menu-symbolic", strings::COMPACT_MENU);
        let placeholder = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let volume_scroll_region = cover.clone().upcast();

        Self {
            layout,
            root: placeholder.clone().upcast(),
            cover,
            title,
            artist,
            album,
            year,
            previous,
            play_pause,
            next,
            shuffle,
            repeat,
            position,
            duration,
            scale,
            menu,
            volume_scroll_region,
        }
    }
}

pub(super) fn build(layout: CompactLayout) -> LayoutWidgets {
    match layout {
        CompactLayout::Cover => build_cover(),
        CompactLayout::Pill => build_pill(),
        CompactLayout::Card => build_card(),
    }
}

fn build_cover() -> LayoutWidgets {
    let mut widgets = LayoutWidgets::common(CompactLayout::Cover, 300);
    let info = metadata_box(&widgets, 320);
    widgets.title.set_xalign(0.5);
    widgets.artist.set_xalign(0.5);
    widgets.album.set_xalign(0.5);
    widgets.year.set_xalign(0.5);
    widgets.volume_scroll_region = widgets.cover.clone().upcast();
    let controls = transport_box(&widgets, false);
    controls.set_halign(gtk4::Align::Center);
    let seek = seek_box(&widgets);
    let root = padded_box(gtk4::Orientation::Vertical, 10);
    widgets.cover.set_halign(gtk4::Align::Center);
    root.append(&widgets.cover);
    root.append(&info);
    root.append(&seek);
    root.append(&controls);
    widgets.root = with_window_chrome(CompactLayout::Cover, root, &widgets);
    widgets
}

fn build_pill() -> LayoutWidgets {
    debug_assert!(is_drag_region("metadata"));
    let mut widgets = LayoutWidgets::common(CompactLayout::Pill, 56);
    let info = metadata_box(&widgets, 160);
    widgets.volume_scroll_region = info.clone().upcast();
    let handle = gtk4::WindowHandle::new();
    handle.set_child(Some(&info));
    handle.set_hexpand(true);
    let controls = transport_box(&widgets, false);
    let seek = seek_box(&widgets);
    seek.set_width_request(180);
    let root = padded_box(gtk4::Orientation::Horizontal, 10);
    root.set_margin_top(8);
    root.set_margin_bottom(8);
    root.append(&widgets.cover);
    root.append(&handle);
    root.append(&controls);
    root.append(&seek);
    root.append(&widgets.menu);
    root.append(&gtk4::WindowControls::new(gtk4::PackType::End));
    widgets.root = with_window_chrome(CompactLayout::Pill, root, &widgets);
    widgets
}

fn build_card() -> LayoutWidgets {
    let mut widgets = LayoutWidgets::common(CompactLayout::Card, 160);
    let info = metadata_box(&widgets, 260);
    widgets.volume_scroll_region = info.clone().upcast();
    let controls = transport_box(&widgets, false);
    let seek = seek_box(&widgets);
    let right = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    right.set_hexpand(true);
    right.append(&info);
    right.append(&seek);
    right.append(&controls);
    let root = padded_box(gtk4::Orientation::Horizontal, 16);
    root.append(&widgets.cover);
    root.append(&right);
    widgets.root = with_window_chrome(CompactLayout::Card, root, &widgets);
    widgets
}

fn with_window_chrome(
    layout: CompactLayout,
    content: gtk4::Box,
    widgets: &LayoutWidgets,
) -> gtk4::Widget {
    if !metrics(layout).separate_header {
        return content.upcast();
    }
    let subtitle = match layout {
        CompactLayout::Bar => strings::COMPACT_LAYOUT_BAR,
        CompactLayout::Cover => strings::COMPACT_LAYOUT_COVER,
        CompactLayout::Pill => strings::COMPACT_LAYOUT_PILL,
        CompactLayout::Card => strings::COMPACT_LAYOUT_CARD,
    };
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &strings::text(strings::APP_NAME),
        &strings::text(subtitle),
    )));
    header.pack_end(&widgets.menu);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    toolbar.upcast()
}

fn padded_box(orientation: gtk4::Orientation, spacing: i32) -> gtk4::Box {
    let root = gtk4::Box::new(orientation, spacing);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root
}

fn metadata_box(widgets: &LayoutWidgets, width: i32) -> gtk4::Box {
    let info = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    info.set_width_request(width);
    info.set_hexpand(true);
    info.append(&widgets.title);
    info.append(&widgets.artist);
    info.append(&widgets.album);
    info.append(&widgets.year);
    info
}

fn transport_box(widgets: &LayoutWidgets, direct_secondary: bool) -> gtk4::Box {
    let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    if direct_secondary {
        if let Some(shuffle) = &widgets.shuffle {
            controls.append(shuffle);
        }
    }
    controls.append(&widgets.previous);
    controls.append(&widgets.play_pause);
    controls.append(&widgets.next);
    if direct_secondary {
        if let Some(repeat) = &widgets.repeat {
            controls.append(repeat);
        }
    }
    controls
}

fn seek_box(widgets: &LayoutWidgets) -> gtk4::Box {
    let seek = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    seek.append(&widgets.position);
    seek.append(&widgets.scale);
    seek.append(&widgets.duration);
    seek
}

fn metadata_label(accessible_name: &str) -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.set_xalign(0.0);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        accessible_name,
    ))]);
    label
}

fn time_label(accessible_name: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some("0:00"));
    label.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        accessible_name,
    ))]);
    label
}

fn icon_button(icon: &str, label: &str) -> gtk4::Button {
    let text = strings::text(label);
    let button = gtk4::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(&text));
    button.update_property(&[gtk4::accessible::Property::Label(&text)]);
    button
}

fn toggle_button(icon: &str, label: &str) -> gtk4::ToggleButton {
    let text = strings::text(label);
    let button = gtk4::ToggleButton::builder()
        .icon_name(icon)
        .tooltip_text(&text)
        .build();
    button.update_property(&[gtk4::accessible::Property::Label(&text)]);
    button
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layout_has_exact_metrics_and_direct_controls() {
        assert_eq!(
            metrics(CompactLayout::Cover),
            LayoutMetrics::new(380, 560, true, false, false)
        );
        assert_eq!(
            metrics(CompactLayout::Pill),
            LayoutMetrics::new(720, 96, false, false, false)
        );
        assert_eq!(
            metrics(CompactLayout::Card),
            LayoutMetrics::new(520, 300, true, false, false)
        );
        assert!(!control_is_sensitive(ControlRole::Playback, false));
        assert!(control_is_sensitive(ControlRole::Playback, true));
        assert!(control_is_sensitive(ControlRole::WindowAction, false));
    }

    #[test]
    fn every_layout_has_a_stable_stack_token() {
        assert_eq!(layout_token(CompactLayout::Cover), "cover");
        assert_eq!(layout_token(CompactLayout::Pill), "pill");
        assert_eq!(layout_token(CompactLayout::Card), "card");
    }

    #[test]
    fn every_stable_token_maps_back_to_a_layout() {
        assert_eq!(layout_from_token("bar"), None);
        assert_eq!(layout_from_token("cover"), Some(CompactLayout::Cover));
        assert_eq!(layout_from_token("pill"), Some(CompactLayout::Pill));
        assert_eq!(layout_from_token("card"), Some(CompactLayout::Card));
        assert_eq!(layout_from_token("unknown"), None);
    }

    #[test]
    fn missing_album_and_year_do_not_create_metadata_rows() {
        assert_eq!(
            visible_detail_rows(CompactLayout::Pill, "Album", Some(2026)),
            Vec::<MetadataRow>::new()
        );
        assert_eq!(
            visible_detail_rows(CompactLayout::Cover, "Album", Some(2026)),
            vec![MetadataRow::Album]
        );
        assert_eq!(
            visible_detail_rows(CompactLayout::Card, "", None),
            Vec::<MetadataRow>::new()
        );
        assert_eq!(
            visible_detail_rows(CompactLayout::Card, "Album", None),
            vec![MetadataRow::Album]
        );
        assert_eq!(
            visible_detail_rows(CompactLayout::Card, "", Some(2026)),
            vec![MetadataRow::Year]
        );
        assert_eq!(
            visible_detail_rows(CompactLayout::Card, "Album", Some(2026)),
            vec![MetadataRow::Album, MetadataRow::Year]
        );
    }

    #[test]
    fn pill_marks_only_its_free_metadata_region_as_draggable() {
        assert!(is_drag_region("metadata"));
        for region in ["cover", "transport", "seek", "menu"] {
            assert!(!is_drag_region(region));
        }
    }
}
