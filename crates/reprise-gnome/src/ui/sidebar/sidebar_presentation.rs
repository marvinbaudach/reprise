//! Visual presentation for the navigation sidebar from design mockup 7a.

use gtk4::prelude::*;
use libadwaita as adw;

use crate::ui::strings;
use reprise_core::format::format_thousands;

const ICON_WIDTH: i32 = 16;
const ROW_HORIZONTAL_MARGIN: i32 = 12;
// These mirror libadwaita's current `.navigation-sidebar > row` geometry.
// Keep the platform rules untouched; the Xvfb layout regression catches drift.
pub(in crate::ui) const SIDEBAR_SURFACE_INSET: i32 = 6;
const ADWAITA_NAVIGATION_ROW_PADDING: i32 = 8;
pub(in crate::ui) const SIDEBAR_TEXT_INSET: i32 =
    SIDEBAR_SURFACE_INSET + ADWAITA_NAVIGATION_ROW_PADDING + ROW_HORIZONTAL_MARGIN;
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
    RecentlyPlayed,
    TopRated,
    RecentlyAdded,
    GenericSmart,
    ImportErrors,
    Missing,
    LibraryDoctor,
    Releases,
    Concerts,
    Podcasts,
    Youtube,
    Radio,
    MyStats,
    TurnedOff,
}

impl NavIcon {
    pub(in crate::ui) const fn icon_name(self) -> &'static str {
        match self {
            Self::Library => "folder-music-symbolic",
            Self::Queue | Self::GenericSmart => "view-list-symbolic",
            Self::Playlist => "media-playlist-consecutive-symbolic",
            Self::RecentlyAdded => "list-add-symbolic",
            Self::RecentlyPlayed => "document-open-recent-symbolic",
            Self::TopRated => "starred-symbolic",
            Self::ImportErrors => "dialog-warning-symbolic",
            Self::Missing => "edit-delete-symbolic",
            Self::LibraryDoctor => crate::ui::library_doctor::DOCTOR_GLYPH,
            Self::Releases => "star-new-symbolic",
            Self::Concerts => "ticket-symbolic",
            Self::Podcasts => "audio-input-microphone-symbolic",
            Self::Youtube => "video-x-generic-symbolic",
            Self::Radio => "reprise-radio-symbolic",
            Self::TurnedOff => "system-shutdown-symbolic",
            Self::MyStats => "reprise-stats-symbolic",
        }
    }

    pub(in crate::ui) const fn fallback_icon_name(self) -> &'static str {
        match self {
            // The app ships the first-aid kit itself, so a theme without the
            // app's icon directory in reach steps down to the magnifier — the
            // same step `library_doctor::doctor_glyph` takes for the start page
            // and the result card.
            Self::LibraryDoctor => crate::ui::library_doctor::DOCTOR_GLYPH_FALLBACK,
            Self::Releases => "starred-symbolic",
            Self::Concerts => "x-office-calendar-symbolic",
            Self::Radio => "audio-x-generic-symbolic",
            Self::MyStats => "view-list-symbolic",
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

fn issue_row_tooltip(presentation: IssueRowPresentation, icon: NavIcon) -> Option<String> {
    (icon == NavIcon::LibraryDoctor)
        .then_some(presentation.badge?)
        .and_then(|count| usize::try_from(count).ok())
        .map(strings::doctor_fixes_ready)
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

/// Builds a row whose trailing count can be updated without replacing the
/// row. Queue mutations use this seam so playback never pays for the full
/// sidebar query projection.
pub(in crate::ui) fn build_live_count_nav_row(
    title: &str,
    count: Option<i64>,
    icon: NavIcon,
) -> (gtk4::ListBoxRow, gtk4::Label) {
    let hbox = row_box();
    hbox.append(&nav_icon(icon));

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_hexpand(true);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    hbox.append(&title_label);

    let count_label = gtk4::Label::new(None);
    count_label.add_css_class("dim-label");
    count_label.add_css_class("numeric");
    update_live_count_label(&count_label, count);
    hbox.append(&count_label);

    (navigation_row(&hbox, title), count_label)
}

pub(in crate::ui) fn update_live_count_label(label: &gtk4::Label, count: Option<i64>) {
    if let Some(count) = count {
        label.set_label(&format_thousands(count));
        label.set_visible(true);
    } else {
        label.set_visible(false);
        label.set_label("");
    }
}

pub(in crate::ui) fn build_issue_nav_row(
    title: &str,
    presentation: IssueRowPresentation,
    icon: NavIcon,
) -> gtk4::ListBoxRow {
    let hbox = row_box();
    hbox.append(&nav_icon(icon));

    if presentation.attention {
        let dot = gtk4::Label::builder()
            .label("●")
            .accessible_role(gtk4::AccessibleRole::Presentation)
            .build();
        dot.add_css_class("warning");
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

    let row = navigation_row(&hbox, title);
    row.set_tooltip_text(issue_row_tooltip(presentation, icon).as_deref());
    row
}

fn section_header_label(text: &str) -> gtk4::Label {
    let label = gtk4::Label::builder()
        .label(text)
        .xalign(0.0)
        .accessible_role(gtk4::AccessibleRole::Heading)
        .build();
    label.add_css_class("caption-heading");
    label.add_css_class("reprise-text-secondary");
    label.set_margin_top(14);
    label.set_margin_bottom(4);
    label
}

fn navigation_header_label(text: &str) -> gtk4::Label {
    let label = section_header_label(text);
    label.set_margin_start(ROW_HORIZONTAL_MARGIN);
    label.set_margin_end(ROW_HORIZONTAL_MARGIN);
    label
}

fn standalone_header_label(text: &str) -> gtk4::Label {
    let label = section_header_label(text);
    label.set_margin_start(SIDEBAR_TEXT_INSET);
    label.set_margin_end(SIDEBAR_TEXT_INSET);
    label
}

pub(in crate::ui) fn append_header(listbox: &gtk4::ListBox, text: &str) -> gtk4::ListBoxRow {
    let label = navigation_header_label(text);
    let row = gtk4::ListBoxRow::builder()
        .child(&label)
        .selectable(false)
        .activatable(false)
        .focusable(false)
        .accessible_role(gtk4::AccessibleRole::Presentation)
        .build();
    listbox.append(&row);
    row
}

pub(in crate::ui) fn append_header_with_action(
    listbox: &gtk4::ListBox,
    text: &str,
    action_name: &str,
    on_activate: impl Fn() + 'static,
) -> gtk4::Button {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
    let label = navigation_header_label(text);
    label.set_hexpand(true);
    hbox.append(&label);

    let button = gtk4::Button::from_icon_name("list-add-symbolic");
    button.add_css_class("flat");
    button.set_tooltip_text(Some(action_name));
    button.update_property(&[gtk4::accessible::Property::Label(action_name)]);
    // a11y-semantics: role=button name=new-playlist state=focusable action=activate
    button.set_focusable(true);
    button.connect_clicked(move |_| on_activate());
    hbox.append(&button);

    let row = gtk4::ListBoxRow::builder()
        .child(&hbox)
        .selectable(false)
        .activatable(false)
        .focusable(false)
        .accessible_role(gtk4::AccessibleRole::Presentation)
        .build();
    listbox.append(&row);
    button
}

pub(in crate::ui) fn build_editable_playlist_row(
    title: &str,
    count: Option<i64>,
) -> (gtk4::ListBoxRow, gtk4::EditableLabel) {
    let hbox = row_box();
    hbox.append(&nav_icon(NavIcon::Playlist));

    let editor = gtk4::EditableLabel::builder()
        .text(title)
        .halign(gtk4::Align::Fill)
        .hexpand(true)
        .accessible_role(gtk4::AccessibleRole::TextBox)
        .build();
    editor.update_property(&[gtk4::accessible::Property::Label(
        &crate::ui::strings::text(crate::ui::strings::NEW_PLAYLIST_ENTRY_PLACEHOLDER),
    )]);
    // a11y-semantics: role=text-box name=playlist-name state=editable action=type
    editor.set_focusable(true);
    hbox.append(&editor);

    if let Some(count) = count {
        let count_label = gtk4::Label::new(Some(&format_thousands(count)));
        count_label.add_css_class("dim-label");
        count_label.add_css_class("numeric");
        hbox.append(&count_label);
    }

    (editable_navigation_row(&hbox, title), editor)
}

pub(in crate::ui) fn problem_header() -> gtk4::Label {
    standalone_header_label(&strings::text(strings::SIDEBAR_SECTION_ISSUES))
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
    let button = gtk4::Button::builder()
        .child(child)
        .has_frame(false)
        .focusable(false)
        .focus_on_click(false)
        .hexpand(true)
        .halign(gtk4::Align::Fill)
        .build();
    button.add_css_class("flat");
    button.add_css_class(crate::ui::style::buttons::SIDEBAR_ROW_ACTION_CLASS);
    crate::ui::style::buttons::arm_cursor(&button);
    button.update_property(&[gtk4::accessible::Property::Label(label)]);

    let row = gtk4::ListBoxRow::builder()
        .child(&button)
        .selectable(true)
        .activatable(true)
        .focusable(true)
        .accessible_role(gtk4::AccessibleRole::ListItem)
        .build();
    row.update_property(&[gtk4::accessible::Property::Label(label)]);
    let activated_row = row.downgrade();
    button.connect_clicked(move |_| {
        let Some(activated_row) = activated_row.upgrade() else {
            return;
        };
        activated_row.grab_focus();
        activated_row.activate();
    });
    row
}

/// The temporary inline playlist editor is already the one local control in
/// its row. Wrapping an editable text box in a button would create nested
/// interactive widgets and a second command, so it keeps the list-item row
/// semantics until the edit rebuilds into a regular navigation button.
fn editable_navigation_row(child: &gtk4::Box, label: &str) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::builder()
        .child(child)
        .selectable(true)
        .activatable(true)
        .focusable(true)
        .accessible_role(gtk4::AccessibleRole::ListItem)
        .build();
    row.update_property(&[gtk4::accessible::Property::Label(label)]);
    row
}

fn nav_icon(icon: NavIcon) -> gtk4::Widget {
    let icon_name = gtk4::gdk::Display::default().map_or_else(
        || resolved_icon_name(icon, false),
        |display| {
            let theme = gtk4::IconTheme::for_display(&display);
            resolved_icon_name(icon, theme.has_icon(icon.icon_name()))
        },
    );
    let image = gtk4::Image::from_icon_name(icon_name);
    image.set_width_request(ICON_WIDTH);
    image.set_pixel_size(ICON_WIDTH);
    image.set_valign(gtk4::Align::Center);
    image.upcast()
}

const fn resolved_icon_name(icon: NavIcon, primary_available: bool) -> &'static str {
    if primary_available {
        icon.icon_name()
    } else {
        icon.fallback_icon_name()
    }
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
        assert_eq!(NavIcon::ImportErrors.icon_name(), "dialog-warning-symbolic");
        assert_eq!(NavIcon::Missing.icon_name(), "edit-delete-symbolic");
        assert_eq!(NavIcon::MyStats.icon_name(), "reprise-stats-symbolic");
        assert_eq!(NavIcon::MyStats.fallback_icon_name(), "view-list-symbolic");
        assert_eq!(
            resolved_icon_name(NavIcon::MyStats, false),
            "view-list-symbolic"
        );
        assert_eq!(NavIcon::Releases.icon_name(), "star-new-symbolic");
        assert_eq!(NavIcon::Concerts.icon_name(), "ticket-symbolic");
        assert_eq!(
            NavIcon::Podcasts.icon_name(),
            "audio-input-microphone-symbolic"
        );
        assert_eq!(NavIcon::Youtube.icon_name(), "video-x-generic-symbolic");
        assert_eq!(NavIcon::Radio.icon_name(), "reprise-radio-symbolic");
        assert_eq!(
            NavIcon::Radio.fallback_icon_name(),
            "audio-x-generic-symbolic"
        );
        assert_eq!(
            resolved_icon_name(NavIcon::Radio, false),
            "audio-x-generic-symbolic"
        );
        assert_eq!(NavIcon::Releases.fallback_icon_name(), "starred-symbolic");
        assert_eq!(
            NavIcon::Concerts.fallback_icon_name(),
            "x-office-calendar-symbolic"
        );
    }

    /// The sidebar entry and the two doctor surfaces have to ask for one
    /// glyph. They did not: the start page and the result card resolved the
    /// shipped first-aid kit through `library_doctor::doctor_glyph`, while this
    /// row still named the magnifier that glyph replaced. Asserting against
    /// that same resolution — both of its answers — is what keeps them
    /// together, because `nav_icon` performs the identical theme check.
    #[test]
    fn the_library_doctor_row_asks_for_the_same_glyph_as_the_doctor_surfaces() {
        assert_eq!(
            NavIcon::LibraryDoctor.icon_name(),
            "io.github.marvinbaudach.Reprise-first-aid-symbolic"
        );
        assert_eq!(
            NavIcon::LibraryDoctor.icon_name(),
            crate::ui::library_doctor::doctor_glyph_for(true)
        );
        assert_eq!(
            NavIcon::LibraryDoctor.fallback_icon_name(),
            crate::ui::library_doctor::doctor_glyph_for(false)
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
        assert_eq!(
            issue_row_presentation(433, NavIcon::LibraryDoctor),
            IssueRowPresentation {
                badge: Some(433),
                attention: false,
            }
        );
        assert_eq!(
            issue_row_tooltip(
                issue_row_presentation(433, NavIcon::LibraryDoctor),
                NavIcon::LibraryDoctor
            ),
            Some("433 fixes ready".to_owned())
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn problem_sources_use_a_labeled_section_header() {
        gtk4::init().unwrap();
        let label = problem_header();

        assert_eq!(label.text(), "ISSUES");
        assert!(label.has_css_class("caption-heading"));
        assert!(label.has_css_class("reprise-text-secondary"));
        assert_eq!(label.accessible_role(), gtk4::AccessibleRole::Heading);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_11_sidebar_entries_are_named_actions_and_sections_are_headings() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let listbox = gtk4::ListBox::new();
        let header = append_header(&listbox, "SMART");
        let nav_row = build_nav_row("My Stats", None, NavIcon::MyStats);
        listbox.append(&nav_row);
        let issue_row = build_issue_nav_row(
            "Missing files",
            issue_row_presentation(2, NavIcon::Missing),
            NavIcon::Missing,
        );
        listbox.append(&issue_row);
        let window = gtk4::Window::builder().child(&listbox).build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!(gtk4::test_accessible_has_role(
            &nav_row,
            gtk4::AccessibleRole::ListItem
        ));
        assert!(gtk4::test_accessible_has_property(
            &nav_row,
            gtk4::AccessibleProperty::Label
        ));
        assert!(nav_row.is_activatable());

        let action = nav_row
            .child()
            .expect("a navigation row has an action child")
            .downcast::<gtk4::Button>()
            .expect("a navigation row's action is a real GtkButton");
        assert!(gtk4::test_accessible_has_role(
            &action,
            gtk4::AccessibleRole::Button
        ));
        assert!(gtk4::test_accessible_has_property(
            &action,
            gtk4::AccessibleProperty::Label
        ));
        let content = action
            .child()
            .expect("the button carries the existing row content")
            .downcast::<gtk4::Box>()
            .unwrap();
        let visible_name = content
            .first_child()
            .and_then(|widget| widget.next_sibling())
            .and_downcast::<gtk4::Label>()
            .expect("the row title stays inside the action");
        assert_eq!(visible_name.text(), "My Stats");
        assert!(nav_row.is_focusable());
        assert!(
            !action.is_focusable(),
            "the list row owns the single tab stop"
        );

        let issue_action = issue_row
            .child()
            .unwrap()
            .downcast::<gtk4::Button>()
            .expect("problem rows use the same real action widget");
        assert!(gtk4::test_accessible_has_property(
            &issue_action,
            gtk4::AccessibleProperty::Label
        ));
        assert!(!issue_action.is_focusable());

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
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_11_navigation_action_does_not_retain_its_row() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let (row_weak, button_weak) = {
            let content = row_box();
            let row = navigation_row(&content, "Queue");
            let button = row
                .child()
                .expect("a navigation row has an action child")
                .downcast::<gtk4::Button>()
                .expect("the navigation action is a real GtkButton");
            (row.downgrade(), button.downgrade())
        };

        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!(
            row_weak.upgrade().is_none(),
            "the navigation action retained its discarded row"
        );
        assert!(
            button_weak.upgrade().is_none(),
            "the discarded row retained its navigation action"
        );
    }

    /// GTK 4.22 only exports an accessible role to AT-SPI when it is a
    /// constructor property. A post-build setter changes the local getter but
    /// leaves the bus node at its original role, so this is a source-structure
    /// regression rather than another getter assertion.
    #[test]
    fn nav_11_sidebar_roles_are_constructor_properties() {
        let sources = sidebar_role_sources();
        assert!(
            sources.len() >= 20,
            "the Sidebar role guard checked suspiciously few Rust files"
        );
        let checked_bytes = sources
            .iter()
            .map(|(_, source)| source.len())
            .sum::<usize>();
        assert!(
            checked_bytes >= 200_000,
            "the Sidebar role guard checked a suspiciously small source tree"
        );
        assert!(
            sources.iter().any(|(name, source)| {
                name == "sidebar_presentation.rs" && source.contains("fn navigation_row(")
            }),
            "the Sidebar role guard did not inspect navigation_row"
        );

        let forbidden_setter = ["set_", "accessible", "_role"].concat();
        let offenders = sources
            .iter()
            .filter_map(|(name, source)| source.contains(&forbidden_setter).then_some(name))
            .collect::<Vec<_>>();

        assert!(
            offenders.is_empty(),
            "accessible roles must be constructor properties; post-build setters found in {offenders:?}"
        );
    }

    fn sidebar_role_sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui/sidebar");
        let mut pending = vec![root.clone()];
        let mut paths = Vec::new();
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
            {
                let path = entry.expect("failed to read a Sidebar source entry").path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        paths
            .into_iter()
            .map(|path| {
                let name = path
                    .strip_prefix(&root)
                    .expect("Sidebar source stays below its module root")
                    .display()
                    .to_string();
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
                (name, source)
            })
            .collect()
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn row_and_split_metrics_follow_the_compact_sidebar_design() {
        if gtk4::init().is_err() {
            return;
        }
        let row = build_nav_row("Queue", Some(1_674), NavIcon::Queue);
        let button = row.child().unwrap().downcast::<gtk4::Button>().unwrap();
        let content = button.child().unwrap().downcast::<gtk4::Box>().unwrap();
        let icon = content
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
}
