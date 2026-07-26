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
const SIDEBAR_MIN_WIDTH: f64 = 240.0;
const SIDEBAR_MAX_WIDTH: f64 = 240.0;
const SIDEBAR_WIDTH_FRACTION: f64 = 0.22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum NavIcon {
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
    Releases,
    Concerts,
    MyStats,
    Conversions,
}

impl NavIcon {
    pub(in crate::ui) const fn icon_name(self) -> &'static str {
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
            Self::Releases => "star-new-symbolic",
            Self::Concerts => "ticket-symbolic",
            // Unused: My Stats renders a drawn three-bar chart via `nav_icon`,
            // not a theme symbolic (so it never collides with `TopRated`'s
            // star). Kept only to satisfy the exhaustive match.
            Self::MyStats => "starred-symbolic",
            // INST-13: the experimental instrumental-conversions view.
            Self::Conversions => "applications-science-symbolic",
        }
    }

    pub(in crate::ui) const fn fallback_icon_name(self) -> &'static str {
        match self {
            Self::Releases => "starred-symbolic",
            Self::Concerts => "x-office-calendar-symbolic",
            _ => self.icon_name(),
        }
    }
}

pub(in crate::ui) fn smart_icon(sort_field: &str) -> NavIcon {
    match sort_field {
        "last_played_at" => NavIcon::RecentlyPlayed,
        "rating" => NavIcon::TopRated,
        "added_at" => NavIcon::RecentlyAdded,
        _ => NavIcon::GenericSmart,
    }
}

/// A sidebar count badge only renders when non-zero: `0` leaves the
/// right-hand column empty instead of displaying a literal zero.
pub(in crate::ui) fn nonzero_count(count: i64) -> Option<i64> {
    (count > 0).then_some(count)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct IssueRowPresentation {
    pub(in crate::ui) badge: Option<i64>,
    pub(in crate::ui) attention: bool,
}

pub(in crate::ui) fn issue_row_presentation(new_count: u32, icon: NavIcon) -> IssueRowPresentation {
    IssueRowPresentation {
        badge: nonzero_count(i64::from(new_count)),
        attention: new_count > 0 && matches!(icon, NavIcon::ImportErrors),
    }
}

pub(in crate::ui) fn build_nav_row(
    title: &str,
    count: Option<i64>,
    icon: NavIcon,
) -> gtk4::ListBoxRow {
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

    navigation_row(&hbox, title)
}

pub(in crate::ui) fn build_issue_nav_row(
    title: &str,
    presentation: IssueRowPresentation,
    icon: NavIcon,
) -> gtk4::ListBoxRow {
    let hbox = row_box();
    hbox.append(&nav_icon(icon));

    if presentation.attention {
        let dot = gtk4::Label::new(Some("●"));
        dot.add_css_class("warning");
        dot.set_accessible_role(gtk4::AccessibleRole::Presentation);
        hbox.append(&dot);
    }

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_hexpand(true);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    hbox.append(&title_label);

    if let Some(count) = presentation.badge {
        let badge = gtk4::Label::new(Some(&format_thousands(count)));
        badge.add_css_class("stats-badge");
        badge.add_css_class("numeric");
        hbox.append(&badge);
    }

    navigation_row(&hbox, title)
}

/// Builds a navigation row with a trailing badge label instead of a count
/// (e.g. "NEW"). The badge uses the accent color via `.stats-badge`.
#[allow(dead_code)]
pub(in crate::ui) fn build_nav_row_with_badge(
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

    navigation_row(&hbox, title)
}

pub(in crate::ui) fn append_header(listbox: &gtk4::ListBox, text: &str) -> gtk4::ListBoxRow {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("caption-heading");
    label.add_css_class("reprise-text-secondary");
    label.set_accessible_role(gtk4::AccessibleRole::Heading);
    label.set_margin_start(ROW_HORIZONTAL_MARGIN);
    label.set_margin_end(ROW_HORIZONTAL_MARGIN);
    label.set_margin_top(14);
    label.set_margin_bottom(4);

    let row = gtk4::ListBoxRow::builder()
        .child(&label)
        .selectable(false)
        .activatable(false)
        .focusable(false)
        .build();
    row.set_accessible_role(gtk4::AccessibleRole::Presentation);
    listbox.append(&row);
    row
}

pub(in crate::ui) struct PlaylistActionRows {
    pub(in crate::ui) new_playlist: gtk4::ListBoxRow,
    pub(in crate::ui) import_playlist: gtk4::ListBoxRow,
}

pub(in crate::ui) fn append_playlist_action_rows(listbox: &gtk4::ListBox) -> PlaylistActionRows {
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
    label_id: &str,
    icon: NavIcon,
) -> gtk4::ListBoxRow {
    let hbox = row_box();
    hbox.append(&nav_icon(icon));

    let label_text = strings::text(label_id);
    let label = gtk4::Label::new(Some(&label_text));
    label.set_xalign(0.0);
    label.add_css_class("dim-label");
    hbox.append(&label);

    let row = gtk4::ListBoxRow::builder()
        .child(&hbox)
        .selectable(false)
        .activatable(true)
        .focusable(true)
        .build();
    row.set_accessible_role(gtk4::AccessibleRole::ListItem);
    row.update_property(&[gtk4::accessible::Property::Label(&label_text)]);
    listbox.append(&row);
    row
}

pub(in crate::ui) fn append_problem_header(listbox: &gtk4::ListBox) -> gtk4::ListBoxRow {
    append_header(listbox, &strings::text(strings::SIDEBAR_SECTION_ISSUES))
}

/// Pins the navigation sidebar at [`SIDEBAR_MIN_WIDTH`] real pixels (NPP-1).
///
/// The unit matters: `AdwOverlaySplitView` measures its sidebar in `sp`
/// (scale-independent units) by default, so the 240 below rendered as 295 px
/// on a display whose text scale is 1.23 — the "240 left, 300 right" ratio
/// silently inverted. The info panel hits its 300 px exactly because it also
/// carries a `width_request`, which is always in pixels; asking for `Px` here
/// makes both columns speak the same unit.
pub(in crate::ui) fn style_overlay_split_view(split: &adw::OverlaySplitView) {
    split.set_sidebar_width_unit(adw::LengthUnit::Px);
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

fn navigation_row(child: &gtk4::Box, label: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::builder()
        .child(child)
        .selectable(true)
        .activatable(true)
        .focusable(true)
        .build();
    row.set_accessible_role(gtk4::AccessibleRole::ListItem);
    row.update_property(&[gtk4::accessible::Property::Label(label)]);
    row
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
    let icon_name = gtk4::gdk::Display::default().map_or_else(
        || icon.fallback_icon_name(),
        |display| {
            let theme = gtk4::IconTheme::for_display(&display);
            if theme.has_icon(icon.icon_name()) {
                icon.icon_name()
            } else {
                icon.fallback_icon_name()
            }
        },
    );
    let image = gtk4::Image::from_icon_name(icon_name);
    image.set_width_request(ICON_WIDTH);
    image.set_pixel_size(ICON_WIDTH);
    image.set_valign(gtk4::Align::Center);
    image.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UX NPP-1: the sidebar half of the fixed 240/300 geometry — pinned to a
    /// single value, not a range, so the asymmetry against the 300 px panel is
    /// deliberate rather than a function of window width.
    #[test]
    fn npp_1_sidebar_uses_the_fixed_pixel_width() {
        assert_eq!(SIDEBAR_MIN_WIDTH, 240.0);
        assert_eq!(SIDEBAR_MAX_WIDTH, 240.0);
    }

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
        assert_eq!(NavIcon::Releases.icon_name(), "star-new-symbolic");
        assert_eq!(NavIcon::Concerts.icon_name(), "ticket-symbolic");
        assert_eq!(NavIcon::Releases.fallback_icon_name(), "starred-symbolic");
        assert_eq!(
            NavIcon::Concerts.fallback_icon_name(),
            "x-office-calendar-symbolic"
        );
    }

    #[test]
    fn smart_playlist_icons_use_stable_sort_fields_with_a_generic_fallback() {
        assert_eq!(smart_icon("last_played_at"), NavIcon::RecentlyPlayed);
        assert_eq!(smart_icon("rating"), NavIcon::TopRated);
        assert_eq!(smart_icon("added_at"), NavIcon::RecentlyAdded);
        assert_eq!(smart_icon("custom_field"), NavIcon::GenericSmart);
    }

    #[test]
    fn issue_rows_project_only_new_counts_and_import_attention() {
        assert_eq!(
            issue_row_presentation(4, NavIcon::ImportErrors),
            IssueRowPresentation {
                badge: Some(4),
                attention: true,
            }
        );
        assert_eq!(
            issue_row_presentation(0, NavIcon::ImportErrors),
            IssueRowPresentation {
                badge: None,
                attention: false,
            }
        );
        assert_eq!(
            issue_row_presentation(2, NavIcon::Missing),
            IssueRowPresentation {
                badge: Some(2),
                attention: false,
            }
        );
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
        assert!(!row.is_focusable());
        let label = row.child().unwrap().downcast::<gtk4::Label>().unwrap();
        assert_eq!(label.text(), "ISSUES");
        assert!(label.has_css_class("caption-heading"));
        assert!(label.has_css_class("reprise-text-secondary"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_11_sidebar_entries_are_named_actions_and_sections_are_headings() {
        gtk4::init().unwrap();
        let listbox = gtk4::ListBox::new();
        let header = append_header(&listbox, "SMART");
        let nav_row = build_nav_row("My Stats", None, NavIcon::MyStats);
        listbox.append(&nav_row);
        let action_rows = append_playlist_action_rows(&listbox);

        for row in [&nav_row, &action_rows.new_playlist] {
            assert!(gtk4::test_accessible_has_role(
                row,
                gtk4::AccessibleRole::ListItem
            ));
            assert!(gtk4::test_accessible_has_property(
                row,
                gtk4::AccessibleProperty::Label
            ));
            assert!(row.is_activatable());
        }

        let activated = std::rc::Rc::new(std::cell::Cell::new(false));
        listbox.connect_row_activated({
            let activated = activated.clone();
            let expected_row = nav_row.clone();
            move |_, row| {
                if row == &expected_row {
                    activated.set(true);
                }
            }
        });
        assert!(gtk4::prelude::WidgetExt::activate(&nav_row));
        assert!(activated.get());

        let heading = header.child().unwrap().downcast::<gtk4::Label>().unwrap();
        assert!(gtk4::test_accessible_has_role(
            &heading,
            gtk4::AccessibleRole::Heading
        ));
        assert!(!header.is_activatable());
        assert!(!header.is_focusable());
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

        let split = test_split_view();
        style_overlay_split_view(&split);
        assert_eq!(split.min_sidebar_width(), 240.0);
        assert_eq!(split.max_sidebar_width(), 240.0);
        assert_eq!(split.sidebar_width_fraction(), 0.22);
        // NPP-1 is a PIXEL contract. Adwaita defaults this to `Sp`, which
        // rendered the 240 above as 295 px at text scale 1.23 while the info
        // panel (pinned by a pixel `width_request`) stayed at 300 — the
        // deliberate 240/300 asymmetry collapsed. Assert the unit, not just
        // the number.
        assert_eq!(split.sidebar_width_unit(), adw::LengthUnit::Px);
    }

    fn test_split_view() -> adw::OverlaySplitView {
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let content = gtk4::Label::new(Some("Library"));
        adw::OverlaySplitView::builder()
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
