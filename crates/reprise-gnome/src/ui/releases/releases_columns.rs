#![allow(dead_code)]

use std::cell::Cell;
use std::rc::Rc;

use chrono::Local;
use gtk4::prelude::*;
use reprise_core::artist_news::{release_status, ReleaseStatus};
use reprise_core::artist_news_history::HistoryEntry;

use super::releases_model::ReleaseObject;
use super::releases_presentation::{
    bandcamp_purchase_target, format_release_date, release_status_label, release_type_label,
};
use crate::ui::strings;

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

fn text_column(
    view: &gtk4::ColumnView,
    title: &str,
    id: Option<&str>,
    expand: bool,
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
        label.set_text(&render(&object.entry()));
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
        .expand(expand)
        .build();
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
    view.append_column(&column);
}

pub(super) fn append_columns(
    view: &gtk4::ColumnView,
    on_set_hidden: &OnSetHidden,
    on_open: &OnOpenTarget,
) -> gtk4::ColumnViewColumn {
    let titles = column_contract();
    let date = text_column(view, &titles[0], Some("date"), false, |entry| {
        format_release_date(&entry.first_release_date, Local::now().date_naive())
    });
    text_column(view, &titles[1], None, true, |entry| entry.title.clone());
    text_column(view, &titles[2], None, true, |entry| {
        entry.artist_name.clone()
    });
    text_column(view, &titles[3], None, false, |entry| {
        release_type_label(&entry.release_type)
    });
    status_column(view, on_set_hidden);
    purchase_column(view, on_open);
    date
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nr_17_table_has_the_five_named_columns() {
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
