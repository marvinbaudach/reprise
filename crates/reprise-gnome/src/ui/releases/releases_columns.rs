#![allow(dead_code)]

use chrono::Local;
use gtk4::prelude::*;
use reprise_core::artist_news::{release_status, ReleaseStatus};
use reprise_core::artist_news_history::HistoryEntry;

use super::releases_model::ReleaseObject;
use super::releases_presentation::{format_release_date, release_status_label, release_type_label};
use crate::ui::strings;

pub(super) fn column_contract() -> Vec<String> {
    [
        strings::RELEASES_DATE,
        strings::RELEASES_TITLE,
        strings::RELEASES_ARTIST,
        strings::RELEASES_TYPE,
        strings::RELEASES_STATUS,
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

fn status_column(view: &gtk4::ColumnView) {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::new(None);
        label.add_css_class("reprise-release-pill");
        label.set_xalign(0.5);
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
            ReleaseStatus::Released => "reprise-release-pill-released",
        };
        label.add_css_class(class);
        label.set_text(&release_status_label(&entry, Local::now().date_naive()));
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
        .title(strings::text(strings::RELEASES_STATUS))
        .factory(&factory)
        .resizable(false)
        .build();
    view.append_column(&column);
}

pub(super) fn append_columns(view: &gtk4::ColumnView) -> gtk4::ColumnViewColumn {
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
    status_column(view);
    date
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nr_14_table_has_the_five_named_columns() {
        assert_eq!(
            column_contract(),
            ["Date", "Title", "Artist", "Type", "Status"]
        );
    }
}
