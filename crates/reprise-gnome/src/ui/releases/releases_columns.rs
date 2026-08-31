use std::rc::Rc;

use chrono::Local;
use gtk4::prelude::*;
use reprise_core::artist_news::{release_status, ReleaseStatus};
use reprise_core::artist_news_history::HistoryEntry;
use reprise_view::columns::{ColumnKey, ReleaseColumn};

use super::releases_cell_surface::{self as cell_surface, OnWireCell};
use super::releases_column_layout::column_contract;
use super::releases_filter_bar::ReleasesFilterBar;
use super::releases_model::ReleaseObject;
use super::releases_presentation::{
    format_partial_date, release_link, release_status_label, release_type_label,
};
use crate::ui::strings;
use crate::ui::table_column_widths as widths;

pub(super) type OnOpenTarget = Rc<dyn Fn(String)>;

/// `sizing` fixes the column's width; see [`widths`] for why every column
/// must carry one (STYLE-9).
fn text_column(
    view: &gtk4::ColumnView,
    on_wire_cell: &OnWireCell,
    title: &str,
    id: Option<&str>,
    sizing: widths::Sizing,
    query: Option<&crate::ui::search_highlight::QuerySource>,
    render: impl Fn(&HistoryEntry) -> String + 'static,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();
    let on_wire_cell = on_wire_cell.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        cell_surface::set_child(item, &label, on_wire_cell.as_ref());
    });
    let query = query.cloned();
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = cell_surface::child::<gtk4::Label>(item) else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ReleaseObject>() else {
            return;
        };
        let text = render(&object.entry());
        if let Some(query) = query.as_ref() {
            crate::ui::search_highlight::apply(&label, &text, &query());
        } else {
            label.set_text(&text);
        }
    });
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = cell_surface::child::<gtk4::Label>(item) else {
            return;
        };
        label.set_text("");
    });
    let column = gtk4::ColumnViewColumn::builder()
        .title(title)
        .factory(&factory)
        .resizable(true)
        .build();
    sizing.apply(&column);
    if let Some(id) = id {
        column.set_id(Some(id));
        column.set_sorter(Some(&gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)));
    }
    view.append_column(&column);
    column
}

fn status_column(view: &gtk4::ColumnView, on_wire_cell: &OnWireCell) {
    let factory = gtk4::SignalListItemFactory::new();
    let on_wire_cell = on_wire_cell.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::new(None);
        label.add_css_class("reprise-release-pill");
        label.set_xalign(0.5);
        cell_surface::set_child(item, &label, on_wire_cell.as_ref());
    });
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = cell_surface::child::<gtk4::Label>(item) else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ReleaseObject>() else {
            return;
        };
        let entry = object.entry();
        for class in [
            "reprise-release-pill-owned",
            "reprise-release-pill-upcoming",
            "reprise-release-pill-released",
        ] {
            label.remove_css_class(class);
        }
        let class = match release_status(&entry, Local::now().date_naive()) {
            ReleaseStatus::InLibrary => "reprise-release-pill-owned",
            ReleaseStatus::Upcoming => "reprise-release-pill-upcoming",
            ReleaseStatus::Incomplete | ReleaseStatus::Missing => "reprise-release-pill-released",
        };
        label.add_css_class(class);
        label.set_text(&release_status_label(&entry, Local::now().date_naive()));
    });
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = cell_surface::child::<gtk4::Label>(item) else {
            return;
        };
        label.set_text("");
    });
    let column = gtk4::ColumnViewColumn::builder()
        .id(ReleaseColumn::Status.as_str())
        .title(strings::text(strings::RELEASES_STATUS))
        .factory(&factory)
        .resizable(false)
        .build();
    // This is a fixed cell width, not a layout pin.
    widths::pin(&column, widths::PILL);
    view.append_column(&column);
}

fn link_column(view: &gtk4::ColumnView, on_open: &OnOpenTarget, on_wire_cell: &OnWireCell) {
    let factory = gtk4::SignalListItemFactory::new();
    let tooltips = crate::ui::lazy_tooltip::ListItemTooltips::default();
    let on_open = on_open.clone();
    let on_wire_cell = on_wire_cell.clone();
    let tooltips_for_setup = tooltips.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let cell = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let button = gtk4::Button::new();
        button.add_css_class("flat");
        button.add_css_class("link");
        let item_weak = item.downgrade();
        let on_open = on_open.clone();
        button.connect_clicked(move |_| {
            let Some(item) = item_weak.upgrade() else {
                return;
            };
            let Some(object) = item.item().and_downcast::<ReleaseObject>() else {
                return;
            };
            if let Some(link) = release_link(&object.entry()) {
                on_open(link.target().to_owned());
            }
        });
        cell.append(&button);
        tooltips_for_setup.install(item, &cell);
        cell_surface::set_child(item, &cell, on_wire_cell.as_ref());
    });
    let tooltips_for_bind = tooltips.clone();
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(cell) = cell_surface::child::<gtk4::Box>(item) else {
            return;
        };
        let Some(button) = cell.first_child().and_downcast::<gtk4::Button>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ReleaseObject>() else {
            return;
        };
        let entry = object.entry();
        if let Some(link) = release_link(&entry) {
            let label = link.label();
            button.set_label(&label);
            button.set_visible(true);
            tooltips_for_bind.set_text(item, &cell, Some(link.target().to_owned()));
            button.update_property(&[gtk4::accessible::Property::Label(&label)]);
        } else {
            button.set_label("");
            button.set_visible(false);
            tooltips_for_bind.set_text(item, &cell, None);
            button.reset_property(gtk4::AccessibleProperty::Label);
        }
    });
    let tooltips_for_unbind = tooltips;
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(cell) = cell_surface::child::<gtk4::Box>(item) else {
            return;
        };
        let Some(button) = cell.first_child().and_downcast::<gtk4::Button>() else {
            return;
        };
        button.set_label("");
        tooltips_for_unbind.set_text(item, &cell, None);
        button.set_visible(false);
        button.reset_property(gtk4::AccessibleProperty::Label);
    });
    let column = gtk4::ColumnViewColumn::builder()
        .id(ReleaseColumn::Buy.as_str())
        .title(strings::text(strings::RELEASES_LINK))
        .factory(&factory)
        .resizable(false)
        .build();
    widths::pin(&column, widths::ACTION);
    view.append_column(&column);
}

pub(super) fn append_columns(
    view: &gtk4::ColumnView,
    on_open: &OnOpenTarget,
    filter_bar: &Rc<ReleasesFilterBar>,
    on_wire_cell: &OnWireCell,
    artist_image: &Rc<crate::ui::artist_portrait_tiles::ArtistPortraitTiles>,
) -> gtk4::ColumnViewColumn {
    let query: crate::ui::search_highlight::QuerySource = {
        let filter_bar = filter_bar.clone();
        Rc::new(move || filter_bar.query())
    };
    append_columns_with_query_and_wire(view, on_open, &query, on_wire_cell, artist_image)
}

#[cfg(test)]
fn append_columns_with_query(
    view: &gtk4::ColumnView,
    on_open: &OnOpenTarget,
    query: &crate::ui::search_highlight::QuerySource,
) -> gtk4::ColumnViewColumn {
    let on_wire_cell: OnWireCell = Rc::new(|_, _| {});
    let artist_image = crate::ui::artist_portrait_tiles::ArtistPortraitTiles::for_test(|_| None);
    append_columns_with_query_and_wire(view, on_open, query, &on_wire_cell, &artist_image)
}

fn append_columns_with_query_and_wire(
    view: &gtk4::ColumnView,
    on_open: &OnOpenTarget,
    query: &crate::ui::search_highlight::QuerySource,
    on_wire_cell: &OnWireCell,
    artist_image: &Rc<crate::ui::artist_portrait_tiles::ArtistPortraitTiles>,
) -> gtk4::ColumnViewColumn {
    let titles = column_contract();
    super::releases_cover_column::append(view, on_wire_cell, artist_image);
    let date = text_column(
        view,
        on_wire_cell,
        &titles[1],
        Some(ReleaseColumn::Date.as_str()),
        widths::Sizing::pinned(widths::DATE),
        None,
        |entry| {
            format_partial_date(
                &entry.first_release_date,
                &crate::ui::date_format::current().date,
            )
        },
    );
    // Title is the filler: it owns whatever width the pinned columns leave.
    text_column(
        view,
        on_wire_cell,
        &titles[2],
        Some(ReleaseColumn::Title.as_str()),
        widths::Sizing::filler(widths::TITLE_MIN),
        Some(query),
        |entry| entry.title.clone(),
    );
    text_column(
        view,
        on_wire_cell,
        &titles[3],
        Some(ReleaseColumn::Artist.as_str()),
        widths::Sizing::pinned(widths::NAME),
        Some(query),
        |entry| entry.artist_name.clone(),
    );
    text_column(
        view,
        on_wire_cell,
        &titles[4],
        Some(ReleaseColumn::Type.as_str()),
        widths::Sizing::pinned(widths::SHORT_LABEL),
        None,
        |entry| release_type_label(&entry.release_type),
    );
    status_column(view, on_wire_cell);
    link_column(view, on_open, on_wire_cell);
    date
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;

    use reprise_core::artist_news::LibraryPresence;

    fn entry(artist: &str, title: &str, release_type: &str, date: &str) -> HistoryEntry {
        HistoryEntry {
            release_group_mbid: "mbid".into(),
            artist_name: artist.into(),
            title: title.into(),
            release_type: release_type.into(),
            first_release_date: date.into(),
            first_seen: None,
            seen_at: None,
            hidden: false,
            hidden_at: None,
            presence: LibraryPresence::Absent,
            announce_url: None,
            track_count: None,
            local_track_count: 0,
        }
    }

    fn descendant_labels(widget: &gtk4::Widget) -> Vec<gtk4::Label> {
        let mut labels = widget
            .clone()
            .downcast::<gtk4::Label>()
            .ok()
            .into_iter()
            .collect::<Vec<_>>();
        let mut child = widget.first_child();
        while let Some(current) = child {
            labels.extend(descendant_labels(&current));
            child = current.next_sibling();
        }
        labels
    }

    fn descendant_buttons(widget: &gtk4::Widget) -> Vec<gtk4::Button> {
        let mut buttons = widget
            .clone()
            .downcast::<gtk4::Button>()
            .ok()
            .into_iter()
            .collect::<Vec<_>>();
        let mut child = widget.first_child();
        while let Some(current) = child {
            buttons.extend(descendant_buttons(&current));
            child = current.next_sibling();
        }
        buttons
    }

    /// UX FIL-5a: Releases marks the matching title and artist, leaves an
    /// unrelated visible field plain, and keeps selection as a separate row
    /// state under the translucent 18% text tint.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_5a_releases_mark_hits_without_replacing_selection_tint() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let store = gtk4::gio::ListStore::new::<ReleaseObject>();
        store.append(&ReleaseObject::new(entry(
            "Falling Leaves",
            "Falling Apart",
            "Album",
            "2026-01-02",
        )));
        let selection = gtk4::SingleSelection::new(Some(store));
        selection.set_selected(0);
        let view = gtk4::ColumnView::new(Some(selection.clone()));
        let on_open: OnOpenTarget = Rc::new(|_| {});
        let query: crate::ui::search_highlight::QuerySource = Rc::new(|| "fall".into());
        append_columns_with_query(&view, &on_open, &query);

        let window = gtk4::Window::new();
        window.set_default_size(1200, 300);
        window.set_child(Some(&view));
        window.present();
        crate::ui::source_context_surface::settle_layout();

        let labels = descendant_labels(view.upcast_ref());
        for text in ["Falling Apart", "Falling Leaves"] {
            assert!(
                labels
                    .iter()
                    .any(|label| label.text() == text && label.uses_markup()),
                "searched field {text:?} was not highlighted"
            );
        }
        assert!(
            labels
                .iter()
                .any(|label| label.text() == "Album" && !label.uses_markup()),
            "a non-searched field claimed the hit"
        );
        assert_eq!(
            selection.selected(),
            0,
            "highlighting replaced row selection"
        );
    }

    /// STYLE-9: the releases table must not re-measure itself
    /// from the rows currently on screen, or every scroll shifts the columns.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_9_releases_columns_keep_their_width_when_the_rows_change() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let store = gtk4::gio::ListStore::new::<ReleaseObject>();
        store.append(&ReleaseObject::new(entry(
            "Air",
            "Moon",
            "EP",
            "2026-01-02",
        )));
        let view = gtk4::ColumnView::new(Some(gtk4::SingleSelection::new(Some(store.clone()))));
        let on_open: OnOpenTarget = Rc::new(|_| {});
        let query: crate::ui::search_highlight::QuerySource = Rc::new(String::new);
        append_columns_with_query(&view, &on_open, &query);

        crate::ui::table_column_widths::assert_stable_across_row_change(&view, || {
            store.splice(
                0,
                1,
                &[ReleaseObject::new(entry(
                    "Godspeed You! Black Emperor and Friends",
                    "Lift Your Skinny Fists Like Antennas to Heaven",
                    "Compilation",
                    "2026-09-14",
                ))],
            );
        });
    }

    /// STYLE-11: the column that started this — it wrote `29 May 26` for a
    /// full date and `May 2026` for a month-precision one, two- and four-digit
    /// years in the same column. Measured on the rendered cell rather than on
    /// the formatter, because that is where the drift lived.
    ///
    /// This proves the releases table here rather than through
    /// `date_format_display_tests`: the full releases view stays on its
    /// "No discography data yet" empty state until a fetch pipeline has run,
    /// so a view-level fixture asserts against an empty table and passes for
    /// the wrong reason.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_11_the_releases_date_column_renders_the_pinned_pattern() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        std::env::set_var(crate::ui::date_format::PATTERN_ENV, "%d.%m.%Y");
        gtk4::init().unwrap();

        // The process resolves its date format once, so this test must be the
        // first read — which is why display tests run one per process
        // (`--exact`). Asserted rather than assumed: on a machine whose own
        // locale is already day-first, a batch run would pass for the wrong
        // reason.
        assert_eq!(
            crate::ui::date_format::current().date,
            reprise_core::format::DatePattern::from_platform("%d.%m.%Y"),
            "another test resolved the date format first; run this one with --exact"
        );

        let store = gtk4::gio::ListStore::new::<ReleaseObject>();
        store.append(&ReleaseObject::new(entry(
            "Artist",
            "Full",
            "Album",
            "2026-05-29",
        )));
        store.append(&ReleaseObject::new(entry(
            "Artist", "Month", "Album", "2026-05",
        )));
        store.append(&ReleaseObject::new(entry(
            "Artist", "Year", "Album", "2026",
        )));
        let view = gtk4::ColumnView::new(Some(gtk4::SingleSelection::new(Some(store))));
        let on_open: OnOpenTarget = Rc::new(|_| {});
        let query: crate::ui::search_highlight::QuerySource = Rc::new(String::new);
        append_columns_with_query(&view, &on_open, &query);

        let window = gtk4::Window::new();
        window.set_default_size(1200, 300);
        window.set_child(Some(&view));
        window.present();
        crate::ui::source_context_surface::settle_layout();

        let texts = descendant_labels(view.upcast_ref())
            .iter()
            .map(|label| label.text().to_string())
            .collect::<Vec<_>>();
        for expected in ["29.05.2026", "05.2026", "2026"] {
            assert!(
                texts.iter().any(|text| text == expected),
                "no cell rendered {expected:?}; rendered labels were {texts:?}"
            );
        }
    }

    #[test]
    fn nr_33_table_has_the_five_named_columns() {
        let columns = column_contract();
        assert_eq!(
            &columns[1..6],
            ["Date", "Release", "Artist", "Type", "Status"]
        );
    }

    #[test]
    fn nr_33_table_ends_with_the_release_link_column() {
        assert_eq!(
            column_contract(),
            ["Cover", "Date", "Release", "Artist", "Type", "Status", "Link"]
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nr_33_release_link_cell_binds_and_clears_the_visible_affordance() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let store = gtk4::gio::ListStore::new::<ReleaseObject>();
        store.append(&ReleaseObject::new(entry(
            "Artist", "Album", "Album", "2026",
        )));
        let view = gtk4::ColumnView::new(Some(gtk4::SingleSelection::new(Some(store.clone()))));
        let opened = Rc::new(RefCell::new(None));
        let on_open: OnOpenTarget = {
            let opened = opened.clone();
            Rc::new(move |target| *opened.borrow_mut() = Some(target))
        };
        let query: crate::ui::search_highlight::QuerySource = Rc::new(String::new);
        append_columns_with_query(&view, &on_open, &query);
        let window = gtk4::Window::new();
        window.set_default_size(1200, 300);
        window.set_child(Some(&view));
        window.present();
        crate::ui::source_context_surface::settle_layout();

        let button = descendant_buttons(view.upcast_ref())
            .into_iter()
            .find(|button| button.label().as_deref() == Some("MusicBrainz"))
            .expect("the fallback link button was not rendered");
        let cell = button.parent().and_downcast::<gtk4::Box>().unwrap();
        assert!(button.has_css_class("flat") && button.has_css_class("link"));
        assert_eq!(
            cell.tooltip_text().as_deref(),
            Some("https://musicbrainz.org/release-group/mbid")
        );
        assert!(gtk4::test_accessible_has_property(
            &button,
            gtk4::AccessibleProperty::Label
        ));
        button.emit_clicked();
        assert_eq!(
            opened.borrow().as_deref(),
            Some("https://musicbrainz.org/release-group/mbid")
        );

        store.remove(0);
        crate::ui::source_context_surface::settle_layout();
        assert!(!button.is_visible());
        assert_eq!(button.label().as_deref(), Some(""));
        assert_eq!(cell.tooltip_text(), None);
        assert!(!gtk4::test_accessible_has_property(
            &button,
            gtk4::AccessibleProperty::Label
        ));
    }

    /// STYLE-10: the Releases header exposes the shared editor, and editing a
    /// free column cannot hide its fixed cover or action columns.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_13_releases_header_right_click_edits_the_table() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
        let on_open: OnOpenTarget = Rc::new(|_| {});
        let query: crate::ui::search_highlight::QuerySource = Rc::new(String::new);
        append_columns_with_query(&view, &on_open, &query);
        let registry = super::super::releases_column_layout::registry(
            &view,
            Rc::new(crate::test_db::open().unwrap()),
        );
        let model = super::super::releases_column_layout::model(&registry);
        crate::ui::table_columns::header_popover::install_header_popover(&view, &model);

        // Present it: without a realised header the gesture cannot tell a
        // header click from a row click, and the popover has nothing to point
        // at. A test that skips this asserts against a zero-height table.
        let window = gtk4::Window::new();
        window.set_default_size(1200, 300);
        window.set_child(Some(&view));
        window.present();
        crate::ui::source_context_surface::settle_layout();

        let controllers = view.observe_controllers();
        let gesture = (0..controllers.n_items())
            .filter_map(|index| controllers.item(index).and_downcast::<gtk4::GestureClick>())
            .find(|gesture| gesture.button() == gtk4::gdk::BUTTON_SECONDARY)
            .expect("no secondary-button gesture was installed on the header");

        // Drive the handler the way the rest of this codebase drives gestures
        // in tests. This does not reproduce GTK's own claim race — only a real
        // pointer does — but it does prove the part we wrote: the click lands
        // in the header band and a popover is realised on the view.
        gesture.emit_by_name::<()>("pressed", &[&1i32, &40.0f64, &4.0f64]);
        crate::ui::source_context_surface::settle_layout();

        let mut child = view.first_child();
        let mut popovers = 0;
        while let Some(current) = child {
            if current
                .downcast_ref::<gtk4::Popover>()
                .is_some_and(|popover| popover.has_css_class("reprise-column-header-popover"))
            {
                popovers += 1;
            }
            child = current.next_sibling();
        }
        assert_eq!(
            popovers, 1,
            "a right-click on the header band did not open the column editor"
        );

        model.set_visible("type", false);
        use reprise_view::columns::ReleaseColumn;
        assert!(!registry.is_visible(ReleaseColumn::Type));
        assert!(registry.is_visible(ReleaseColumn::Cover));
        assert!(registry.is_visible(ReleaseColumn::Status));
        assert!(registry.is_visible(ReleaseColumn::Buy));
    }
}
