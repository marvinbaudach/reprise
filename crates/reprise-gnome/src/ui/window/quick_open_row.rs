//! Recycled `GtkListView` rows for quick open.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_view::quick_open::{QuickOpenKind, QuickOpenRow};

#[derive(Clone)]
pub(super) struct PresentedRow {
    pub(super) row: QuickOpenRow,
    pub(super) starts_group: bool,
}

struct RowWidgets {
    root: gtk4::Box,
    heading: gtk4::Label,
    title: gtk4::Label,
    subtitle: gtk4::Label,
}

pub(super) fn factory(rows: Rc<RefCell<Vec<PresentedRow>>>) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let heading = gtk4::Label::builder().xalign(0.0).build();
        heading.add_css_class("heading");
        heading.set_margin_top(10);
        let title = gtk4::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        title.add_css_class("body");
        let subtitle = gtk4::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        subtitle.add_css_class("dim-label");
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_margin_top(6);
        root.set_margin_bottom(6);
        root.append(&heading);
        root.append(&title);
        root.append(&subtitle);
        root.set_accessible_role(gtk4::AccessibleRole::ListItem);
        item.set_child(Some(&root));
    });
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let index = item.position() as usize;
        let rows = rows.borrow();
        let Some(presented) = rows.get(index) else {
            return;
        };
        let Some(widgets) = row_widgets(item) else {
            return;
        };
        bind(&widgets, presented);
    });
    factory
}

fn row_widgets(item: &gtk4::ListItem) -> Option<RowWidgets> {
    let root = item.child()?.downcast::<gtk4::Box>().ok()?;
    let heading = root.first_child()?.downcast::<gtk4::Label>().ok()?;
    let title = heading.next_sibling()?.downcast::<gtk4::Label>().ok()?;
    let subtitle = title.next_sibling()?.downcast::<gtk4::Label>().ok()?;
    Some(RowWidgets {
        root,
        heading,
        title,
        subtitle,
    })
}

fn bind(widgets: &RowWidgets, presented: &PresentedRow) {
    let kind = row_kind(&presented.row);
    widgets.heading.set_visible(presented.starts_group);
    widgets.heading.set_label(kind.section_label());
    let (title, subtitle, accessible) = match &presented.row {
        QuickOpenRow::Item(item) => (
            item.title.clone(),
            item.subtitle.clone(),
            format!(
                "{}: {}, {}",
                item.kind.singular_label(),
                item.title,
                item.subtitle
            ),
        ),
        QuickOpenRow::ShowAll { count, .. } => {
            let title = format!("Show all {count} in {}", kind.section_label());
            (title.clone(), String::new(), title)
        }
    };
    widgets.title.set_label(&title);
    widgets.subtitle.set_label(&subtitle);
    widgets.subtitle.set_visible(!subtitle.is_empty());
    widgets
        .root
        .update_property(&[gtk4::accessible::Property::Label(&accessible)]);
}

pub(super) const fn row_kind(row: &QuickOpenRow) -> QuickOpenKind {
    match row {
        QuickOpenRow::Item(item) => item.kind,
        QuickOpenRow::ShowAll { kind, .. } => *kind,
    }
}
