//! Visual presentation for the navigation sidebar from design mockup 7a.

use gtk4::prelude::*;
use libadwaita as adw;

use crate::ui::eq_bars::{self, EqVariant};
use crate::ui::strings;
use reprise_core::format::format_thousands;

const ICON_WIDTH: i32 = 16;
const ROW_HORIZONTAL_MARGIN: i32 = 12;
const ROW_VERTICAL_MARGIN: i32 = 5;
const ROW_SPACING: i32 = 10;
const SIDEBAR_MIN_WIDTH: f64 = 220.0;
const SIDEBAR_MAX_WIDTH: f64 = 280.0;
const SIDEBAR_WIDTH_FRACTION: f64 = 0.22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NavIcon {
    Library,
    Queue,
    Playlist,
    NewPlaylist,
    ImportPlaylist,
    RecentlyPlayed,
    TopRated,
    RecentlyAdded,
    GenericSmart,
    ImportErrors,
    Missing,
    MyStats,
}

impl NavIcon {
    pub(super) const fn icon_name(self) -> &'static str {
        match self {
            Self::Library => "folder-music-symbolic",
            Self::Queue | Self::GenericSmart => "view-list-symbolic",
            Self::Playlist => "media-playlist-consecutive-symbolic",
            Self::NewPlaylist | Self::RecentlyAdded => "list-add-symbolic",
            Self::ImportPlaylist => "document-open-symbolic",
            Self::RecentlyPlayed => "document-open-recent-symbolic",
            Self::TopRated => "starred-symbolic",
            Self::ImportErrors => "dialog-warning-symbolic",
            Self::Missing => "edit-delete-symbolic",
            // Unused: My Stats renders a drawn three-bar chart via `nav_icon`,
            // not a theme symbolic (so it never collides with `TopRated`'s
            // star). Kept only to satisfy the exhaustive match.
            Self::MyStats => "starred-symbolic",
        }
    }
}

pub(super) fn smart_icon(sort_field: &str) -> NavIcon {
    match sort_field {
        "last_played_at" => NavIcon::RecentlyPlayed,
        "rating" => NavIcon::TopRated,
        "added_at" => NavIcon::RecentlyAdded,
        _ => NavIcon::GenericSmart,
    }
}

pub(super) fn build_nav_row(title: &str, count: Option<i64>, icon: NavIcon) -> gtk4::ListBoxRow {
    let hbox = row_box();
    hbox.append(&nav_icon(icon));

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_hexpand(true);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    hbox.append(&title_label);

    if let Some(count) = count {
        let count_label = gtk4::Label::new(Some(&format_thousands(count)));
        count_label.add_css_class("dim-label");
        count_label.add_css_class("numeric");
        hbox.append(&count_label);
    }

    gtk4::ListBoxRow::builder().child(&hbox).build()
}

/// Builds a navigation row with a trailing badge label instead of a count
/// (e.g. "NEW"). The badge uses the accent color via `.stats-badge`.
#[allow(dead_code)]
pub(super) fn build_nav_row_with_badge(
    title: &str,
    badge_text: &str,
    icon: NavIcon,
) -> gtk4::ListBoxRow {
    let hbox = row_box();
    hbox.append(&nav_icon(icon));

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_hexpand(true);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    hbox.append(&title_label);

    let badge = gtk4::Label::new(Some(badge_text));
    badge.add_css_class("stats-badge");
    hbox.append(&badge);

    gtk4::ListBoxRow::builder().child(&hbox).build()
}

pub(super) fn append_header(listbox: &gtk4::ListBox, text: &str) -> gtk4::ListBoxRow {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("caption-heading");
    label.add_css_class("dim-label");
    label.set_margin_start(ROW_HORIZONTAL_MARGIN);
    label.set_margin_end(ROW_HORIZONTAL_MARGIN);
    label.set_margin_top(14);
    label.set_margin_bottom(4);

    let row = gtk4::ListBoxRow::builder()
        .child(&label)
        .selectable(false)
        .activatable(false)
        .build();
    listbox.append(&row);
    row
}

pub(super) struct PlaylistActionRows {
    pub(super) new_playlist: gtk4::ListBoxRow,
    pub(super) import_playlist: gtk4::ListBoxRow,
}

pub(super) fn append_playlist_action_rows(listbox: &gtk4::ListBox) -> PlaylistActionRows {
    let new_playlist =
        append_playlist_action_row(listbox, strings::SIDEBAR_NEW_PLAYLIST, NavIcon::NewPlaylist);
    let import_playlist =
        append_playlist_action_row(listbox, strings::IMPORT_PLAYLIST, NavIcon::ImportPlaylist);
    PlaylistActionRows {
        new_playlist,
        import_playlist,
    }
}

fn append_playlist_action_row(
    listbox: &gtk4::ListBox,
    label: &str,
    icon: NavIcon,
) -> gtk4::ListBoxRow {
    let hbox = row_box();
    hbox.append(&nav_icon(icon));

    let label = gtk4::Label::new(Some(&strings::text(label)));
    label.set_xalign(0.0);
    label.add_css_class("dim-label");
    hbox.append(&label);

    let row = gtk4::ListBoxRow::builder()
        .child(&hbox)
        .selectable(false)
        .activatable(true)
        .build();
    listbox.append(&row);
    row
}

pub(super) fn append_problem_header(listbox: &gtk4::ListBox) -> gtk4::ListBoxRow {
    append_header(listbox, &strings::text(strings::SIDEBAR_SECTION_ISSUES))
}

pub(super) fn style_split_view(split: &adw::NavigationSplitView) {
    split.set_min_sidebar_width(SIDEBAR_MIN_WIDTH);
    split.set_max_sidebar_width(SIDEBAR_MAX_WIDTH);
    split.set_sidebar_width_fraction(SIDEBAR_WIDTH_FRACTION);
}

fn row_box() -> gtk4::Box {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
    hbox.set_margin_start(ROW_HORIZONTAL_MARGIN);
    hbox.set_margin_end(ROW_HORIZONTAL_MARGIN);
    hbox.set_margin_top(ROW_VERTICAL_MARGIN);
    hbox.set_margin_bottom(ROW_VERTICAL_MARGIN);
    hbox
}

fn nav_icon(icon: NavIcon) -> gtk4::Widget {
    // My Stats renders a drawn three-bar chart (see `eq_bars`) rather than a
    // theme symbolic, so it reads as "stats" and is unmistakably distinct from
    // the "Top rated" star — two identical icons in one section aren't allowed.
    if matches!(icon, NavIcon::MyStats) {
        let bars = eq_bars::build(EqVariant::Static);
        bars.set_valign(gtk4::Align::Center);
        return bars.upcast();
    }
    let image = gtk4::Image::from_icon_name(icon.icon_name());
    image.set_width_request(ICON_WIDTH);
    image.set_pixel_size(ICON_WIDTH);
    image.set_valign(gtk4::Align::Center);
    image.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_match_each_mockup_navigation_kind() {
        assert_eq!(NavIcon::Library.icon_name(), "folder-music-symbolic");
        assert_eq!(NavIcon::Queue.icon_name(), "view-list-symbolic");
        assert_eq!(
            NavIcon::Playlist.icon_name(),
            "media-playlist-consecutive-symbolic"
        );
        assert_eq!(NavIcon::NewPlaylist.icon_name(), "list-add-symbolic");
        assert_eq!(
            NavIcon::ImportPlaylist.icon_name(),
            "document-open-symbolic"
        );
        assert_eq!(NavIcon::ImportErrors.icon_name(), "dialog-warning-symbolic");
        assert_eq!(NavIcon::Missing.icon_name(), "edit-delete-symbolic");
        assert_eq!(NavIcon::MyStats.icon_name(), "starred-symbolic");
    }

    #[test]
    fn smart_playlist_icons_use_stable_sort_fields_with_a_generic_fallback() {
        assert_eq!(smart_icon("last_played_at"), NavIcon::RecentlyPlayed);
        assert_eq!(smart_icon("rating"), NavIcon::TopRated);
        assert_eq!(smart_icon("added_at"), NavIcon::RecentlyAdded);
        assert_eq!(smart_icon("custom_field"), NavIcon::GenericSmart);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn playlist_actions_group_creation_and_import() {
        gtk4::init().unwrap();
        let listbox = gtk4::ListBox::new();
        let rows = append_playlist_action_rows(&listbox);

        assert_eq!(row_label(&rows.new_playlist), "New playlist");
        assert_eq!(row_label(&rows.import_playlist), "Import playlist…");
        assert_eq!(
            rows.new_playlist.next_sibling(),
            Some(rows.import_playlist.clone().upcast())
        );
        assert!(!rows.new_playlist.is_selectable());
        assert!(!rows.import_playlist.is_selectable());
        assert!(rows.new_playlist.is_activatable());
        assert!(rows.import_playlist.is_activatable());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn problem_sources_use_a_labeled_section_header() {
        gtk4::init().unwrap();
        let listbox = gtk4::ListBox::new();
        let row = append_problem_header(&listbox);

        assert!(!row.is_selectable());
        assert!(!row.is_activatable());
        let label = row.child().unwrap().downcast::<gtk4::Label>().unwrap();
        assert_eq!(label.text(), "ISSUES");
        assert!(label.has_css_class("caption-heading"));
        assert!(label.has_css_class("dim-label"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn row_and_split_metrics_follow_the_compact_sidebar_design() {
        if gtk4::init().is_err() {
            return;
        }
        let row = build_nav_row("Queue", Some(1_674), NavIcon::Queue);
        let child = row.child().unwrap();
        let icon = child
            .first_child()
            .unwrap()
            .downcast::<gtk4::Image>()
            .unwrap();
        let title = icon
            .next_sibling()
            .unwrap()
            .downcast::<gtk4::Label>()
            .unwrap();
        let count = title
            .next_sibling()
            .unwrap()
            .downcast::<gtk4::Label>()
            .unwrap();

        assert_eq!(icon.icon_name().as_deref(), Some("view-list-symbolic"));
        assert_eq!(icon.width_request(), 16);
        assert_eq!(title.text(), "Queue");
        assert_eq!(count.text(), "1,674");
        assert!(count.has_css_class("numeric"));

        let split = test_navigation();
        style_split_view(&split);
        assert_eq!(split.min_sidebar_width(), 220.0);
        assert_eq!(split.max_sidebar_width(), 280.0);
        assert_eq!(split.sidebar_width_fraction(), 0.22);
    }

    fn test_navigation() -> adw::NavigationSplitView {
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let content = adw::NavigationPage::builder()
            .title("Library")
            .child(&gtk4::Label::new(Some("Library")))
            .build();
        adw::NavigationSplitView::builder()
            .sidebar(&sidebar)
            .content(&content)
            .build()
    }

    fn row_label(row: &gtk4::ListBoxRow) -> String {
        row.child()
            .unwrap()
            .last_child()
            .unwrap()
            .downcast::<gtk4::Label>()
            .unwrap()
            .text()
            .to_string()
    }
}
