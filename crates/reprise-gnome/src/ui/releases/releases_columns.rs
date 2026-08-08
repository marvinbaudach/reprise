#![allow(dead_code)]

use std::cell::Cell;
use std::rc::Rc;

use chrono::Local;
use gtk4::prelude::*;
use reprise_core::artist_news::{release_status, ReleaseStatus};
use reprise_core::artist_news_history::HistoryEntry;

use super::releases_filter_bar::ReleasesFilterBar;
use super::releases_model::ReleaseObject;
use super::releases_presentation::{
    bandcamp_purchase_target, format_release_date, release_status_label, release_type_label,
};
use crate::ui::strings;
use crate::ui::table_column_widths as widths;

const PILL_PAGE: &str = "pill";
const ACTION_PAGE: &str = "action";

pub(super) type OnSetHidden = Rc<dyn Fn(String, bool)>;
pub(super) type OnOpenTarget = Rc<dyn Fn(String)>;

pub(super) fn column_contract() -> Vec<String> {
    [
        strings::RELEASES_DATE,
        strings::RELEASES_TITLE,
        strings::RELEASES_ARTIST,
        strings::RELEASES_TYPE,
        strings::RELEASES_STATUS,
        strings::RELEASES_BUY,
    ]
    .into_iter()
    .map(strings::text)
    .collect()
}

/// `sizing` fixes the column's width; see [`widths`] for why every column
/// must carry one (STYLE-9).
fn text_column(
    view: &gtk4::ColumnView,
    title: &str,
    id: Option<&str>,
    sizing: widths::Sizing,
    query: Option<&crate::ui::search_highlight::QuerySource>,
    render: impl Fn(&HistoryEntry) -> String + 'static,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        item.set_child(Some(&label));
    });
    let query = query.cloned();
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
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
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
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

fn status_column(view: &gtk4::ColumnView, on_set_hidden: &OnSetHidden) {
    let factory = gtk4::SignalListItemFactory::new();
    let on_set_hidden = on_set_hidden.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let cell = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let label = gtk4::Label::new(None);
        label.add_css_class("reprise-release-pill");
        label.set_xalign(0.5);
        let button = gtk4::Button::new();
        button.add_css_class("flat");
        let item_weak = item.downgrade();
        let on_set_hidden = on_set_hidden.clone();
        button.connect_clicked(move |_| {
            let Some(item) = item_weak.upgrade() else {
                return;
            };
            let Some(object) = item.item().and_downcast::<ReleaseObject>() else {
                return;
            };
            let entry = object.entry();
            on_set_hidden(entry.release_group_mbid, !entry.hidden);
        });
        let stack = gtk4::Stack::new();
        stack.add_named(&label, Some(PILL_PAGE));
        stack.add_named(&button, Some(ACTION_PAGE));
        stack.set_visible_child_name(PILL_PAGE);
        let pointer_inside = Rc::new(Cell::new(false));
        let focus_inside = Rc::new(Cell::new(false));
        let motion = gtk4::EventControllerMotion::new();
        {
            let stack = stack.clone();
            let pointer_inside = pointer_inside.clone();
            let focus_inside = focus_inside.clone();
            motion.connect_enter(move |_, _, _| {
                pointer_inside.set(true);
                stack.set_visible_child_name(ACTION_PAGE);
                if focus_inside.get() {
                    stack.set_visible_child_name(ACTION_PAGE);
                }
            });
        }
        {
            let stack = stack.clone();
            let pointer_inside = pointer_inside.clone();
            let focus_inside = focus_inside.clone();
            motion.connect_leave(move |_| {
                pointer_inside.set(false);
                if !focus_inside.get() {
                    stack.set_visible_child_name(PILL_PAGE);
                }
            });
        }
        cell.add_controller(motion);
        let focus = gtk4::EventControllerFocus::new();
        {
            let stack = stack.clone();
            let focus_inside = focus_inside.clone();
            focus.connect_enter(move |_| {
                focus_inside.set(true);
                stack.set_visible_child_name(ACTION_PAGE);
            });
        }
        {
            let stack = stack.clone();
            let pointer_inside = pointer_inside.clone();
            let focus_inside = focus_inside.clone();
            focus.connect_leave(move |_| {
                focus_inside.set(false);
                if !pointer_inside.get() {
                    stack.set_visible_child_name(PILL_PAGE);
                }
            });
        }
        cell.add_controller(focus);
        cell.append(&stack);
        item.set_child(Some(&cell));
    });
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(cell) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(stack) = cell.first_child().and_downcast::<gtk4::Stack>() else {
            return;
        };
        let Some(label) = stack.child_by_name(PILL_PAGE).and_downcast::<gtk4::Label>() else {
            return;
        };
        let Some(button) = stack
            .child_by_name(ACTION_PAGE)
            .and_downcast::<gtk4::Button>()
        else {
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
        let action = strings::text(if entry.hidden {
            strings::SHOW_AGAIN
        } else {
            strings::RELEASES_HIDE
        });
        button.set_label(&action);
        button.set_tooltip_text(Some(&action));
        button.update_property(&[gtk4::accessible::Property::Label(&action)]);
        stack.set_visible_child_name(PILL_PAGE);
    });
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(cell) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(stack) = cell.first_child().and_downcast::<gtk4::Stack>() else {
            return;
        };
        let Some(label) = stack.child_by_name(PILL_PAGE).and_downcast::<gtk4::Label>() else {
            return;
        };
        let Some(button) = stack
            .child_by_name(ACTION_PAGE)
            .and_downcast::<gtk4::Button>()
        else {
            return;
        };
        label.set_text("");
        button.set_label("");
        button.set_tooltip_text(None);
        stack.set_visible_child_name(PILL_PAGE);
    });
    let column = gtk4::ColumnViewColumn::builder()
        .title(strings::text(strings::RELEASES_STATUS))
        .factory(&factory)
        .resizable(false)
        .build();
    // The pill and the hover action swap inside a Stack, and both are wider
    // in some rows than others — unpinned, this cell alone re-sizes the table.
    widths::pin(&column, widths::PILL);
    view.append_column(&column);
}

fn purchase_column(view: &gtk4::ColumnView, on_open: &OnOpenTarget) {
    let factory = gtk4::SignalListItemFactory::new();
    let on_open = on_open.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
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
            if let Some(target) = bandcamp_purchase_target(&object.entry()) {
                on_open(target.to_owned());
            }
        });
        item.set_child(Some(&button));
    });
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(button) = item.child().and_downcast::<gtk4::Button>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ReleaseObject>() else {
            return;
        };
        let entry = object.entry();
        let target = bandcamp_purchase_target(&entry);
        button.set_label(&strings::text(strings::RELEASES_BANDCAMP));
        button.set_tooltip_text(target);
        button.set_visible(target.is_some());
        let accessible_label = strings::text(strings::RELEASES_BUY_ON_BANDCAMP);
        button.update_property(&[gtk4::accessible::Property::Label(&accessible_label)]);
    });
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(button) = item.child().and_downcast::<gtk4::Button>() else {
            return;
        };
        button.set_label("");
        button.set_tooltip_text(None);
        button.set_visible(false);
    });
    let column = gtk4::ColumnViewColumn::builder()
        .title(strings::text(strings::RELEASES_BUY))
        .factory(&factory)
        .resizable(false)
        .build();
    widths::pin(&column, widths::ACTION);
    view.append_column(&column);
}

pub(super) fn append_columns(
    view: &gtk4::ColumnView,
    on_set_hidden: &OnSetHidden,
    on_open: &OnOpenTarget,
    filter_bar: &Rc<ReleasesFilterBar>,
) -> gtk4::ColumnViewColumn {
    let query: crate::ui::search_highlight::QuerySource = {
        let filter_bar = filter_bar.clone();
        Rc::new(move || filter_bar.query())
    };
    append_columns_with_query(view, on_set_hidden, on_open, &query)
}

fn append_columns_with_query(
    view: &gtk4::ColumnView,
    on_set_hidden: &OnSetHidden,
    on_open: &OnOpenTarget,
    query: &crate::ui::search_highlight::QuerySource,
) -> gtk4::ColumnViewColumn {
    let titles = column_contract();
    let date = text_column(
        view,
        &titles[0],
        Some("date"),
        widths::Sizing::pinned(widths::DATE),
        None,
        |entry| format_release_date(&entry.first_release_date, Local::now().date_naive()),
    );
    // Title is the filler: it owns whatever width the pinned columns leave.
    text_column(
        view,
        &titles[1],
        None,
        widths::Sizing::filler(widths::TITLE_MIN),
        Some(query),
        |entry| entry.title.clone(),
    );
    text_column(
        view,
        &titles[2],
        None,
        widths::Sizing::pinned(widths::NAME),
        Some(query),
        |entry| entry.artist_name.clone(),
    );
    text_column(
        view,
        &titles[3],
        None,
        widths::Sizing::pinned(widths::SHORT_LABEL),
        None,
        |entry| release_type_label(&entry.release_type),
    );
    status_column(view, on_set_hidden);
    purchase_column(view, on_open);
    date
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let on_set_hidden: OnSetHidden = Rc::new(|_, _| {});
        let on_open: OnOpenTarget = Rc::new(|_| {});
        let query: crate::ui::search_highlight::QuerySource = Rc::new(|| "fall".into());
        append_columns_with_query(&view, &on_set_hidden, &on_open, &query);

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
        let on_set_hidden: OnSetHidden = Rc::new(|_, _| {});
        let on_open: OnOpenTarget = Rc::new(|_| {});
        let query: crate::ui::search_highlight::QuerySource = Rc::new(String::new);
        append_columns_with_query(&view, &on_set_hidden, &on_open, &query);

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

    #[test]
    fn nr_25_table_has_the_five_named_columns() {
        let columns = column_contract();
        assert_eq!(&columns[..5], ["Date", "Title", "Artist", "Type", "Status"]);
    }

    #[test]
    fn nr_20_table_adds_a_bandcamp_purchase_column() {
        assert_eq!(
            column_contract(),
            ["Date", "Title", "Artist", "Type", "Status", "Buy"]
        );
    }
}
