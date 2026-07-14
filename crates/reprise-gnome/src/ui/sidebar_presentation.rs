//! Visual presentation for the navigation sidebar from design mockup 7a.

use gtk4::prelude::*;
use libadwaita as adw;

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
    RecentlyPlayed,
    TopRated,
    RecentlyAdded,
    GenericSmart,
    ImportErrors,
    Missing,
}

impl NavIcon {
    pub(super) const fn icon_name(self) -> &'static str {
        match self {
            Self::Library => "folder-music-symbolic",
            Self::Queue | Self::GenericSmart => "view-list-symbolic",
            Self::Playlist => "media-playlist-consecutive-symbolic",
            Self::NewPlaylist | Self::RecentlyAdded => "list-add-symbolic",
            Self::RecentlyPlayed => "document-open-recent-symbolic",
            Self::TopRated => "starred-symbolic",
            Self::ImportErrors => "dialog-warning-symbolic",
            Self::Missing => "edit-delete-symbolic",
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

pub(super) fn append_new_playlist_row(listbox: &gtk4::ListBox) -> gtk4::ListBoxRow {
    let hbox = row_box();
    hbox.append(&nav_icon(NavIcon::NewPlaylist));

    let label = gtk4::Label::new(Some(&strings::text(strings::SIDEBAR_NEW_PLAYLIST)));
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

fn nav_icon(icon: NavIcon) -> gtk4::Image {
    let image = gtk4::Image::from_icon_name(icon.icon_name());
    image.set_width_request(ICON_WIDTH);
    image.set_pixel_size(ICON_WIDTH);
    image.set_valign(gtk4::Align::Center);
    image
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
        assert_eq!(NavIcon::ImportErrors.icon_name(), "dialog-warning-symbolic");
        assert_eq!(NavIcon::Missing.icon_name(), "edit-delete-symbolic");
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
}
