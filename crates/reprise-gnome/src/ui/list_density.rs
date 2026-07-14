//! Live list-density projection for the virtualized library table.
//!
//! `GtkColumnView` creates private row/cell widgets whose style contexts do
//! not reliably rematch an ancestor class at runtime. The factories mark
//! each app-owned cell with `reprise-track-cell`; density is applied to those
//! concrete widgets and inherited by cells created later while scrolling.
//! Their content minima combine with GTK's cell chrome while preserving the
//! interactive rating buttons, producing three visibly distinct row sizes.

use gtk4::prelude::*;
use reprise_core::library::settings::ListDensity;

const DENSITY_CSS: &str = ".reprise-track-cell.reprise-density-comfortable { min-height: 32px; }\n\
     .reprise-track-cell.reprise-density-standard { min-height: 24px; }\n\
     .reprise-track-cell.reprise-density-compact { min-height: 12px; font-size: 10px; }\n\
     .reprise-rating-star { min-height: 0; border: 0; padding: 0; margin: 0; }\n\
     .reprise-rating-star.reprise-density-compact { font-size: 10px; }";

fn density_class(density: ListDensity) -> &'static str {
    match density {
        ListDensity::Comfortable => "reprise-density-comfortable",
        ListDensity::Standard => "reprise-density-standard",
        ListDensity::Compact => "reprise-density-compact",
    }
}

pub(super) fn install(view: &gtk4::ColumnView) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(DENSITY_CSS);
    gtk4::style_context_add_provider_for_display(
        &view.display(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

pub(super) fn apply(view: &gtk4::ColumnView, density: ListDensity) {
    set_density_class(view.upcast_ref(), density);
    apply_to_density_widgets(view.upcast_ref(), density);
    view.queue_resize();
}

pub(super) fn inherit(view: &gtk4::ColumnView, cell: &impl IsA<gtk4::Widget>) {
    let density = if view.has_css_class(density_class(ListDensity::Comfortable)) {
        ListDensity::Comfortable
    } else if view.has_css_class(density_class(ListDensity::Compact)) {
        ListDensity::Compact
    } else {
        ListDensity::Standard
    };
    apply_to_density_widgets(cell.upcast_ref(), density);
}

fn set_density_class(widget: &gtk4::Widget, density: ListDensity) {
    for class in [
        "reprise-density-comfortable",
        "reprise-density-standard",
        "reprise-density-compact",
    ] {
        widget.remove_css_class(class);
    }
    widget.add_css_class(density_class(density));
    widget.queue_resize();
}

fn apply_to_density_widgets(widget: &gtk4::Widget, density: ListDensity) {
    if widget.has_css_class("reprise-track-cell") || widget.has_css_class("reprise-rating-star") {
        set_density_class(widget, density);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        apply_to_density_widgets(&current, density);
        child = current.next_sibling();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rating::RatingWidget;
    use crate::ui::track_cover::TrackCover;

    #[test]
    fn every_density_has_one_stable_css_class_and_rule() {
        for density in [
            ListDensity::Comfortable,
            ListDensity::Standard,
            ListDensity::Compact,
        ] {
            assert!(DENSITY_CSS.contains(density_class(density)));
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn list_density_changes_a_representative_track_table_row() {
        fn find_track_cell(widget: &gtk4::Widget) -> Option<gtk4::Widget> {
            if widget.has_css_class("reprise-track-cell") {
                return Some(widget.clone());
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                if let Some(found) = find_track_cell(&current) {
                    return Some(found);
                }
                child = current.next_sibling();
            }
            None
        }

        if gtk4::init().is_err() {
            return;
        }

        let model = gtk4::StringList::new(&["Track"]);
        let selection = gtk4::NoSelection::new(Some(model));
        let view = gtk4::ColumnView::new(Some(selection));
        let factory = gtk4::SignalListItemFactory::new();
        let cover_view = view.clone();
        let bytes = gtk4::glib::Bytes::from_owned(vec![0xff_u8; 48 * 48 * 4]);
        let texture = gtk4::gdk::MemoryTexture::new(
            48,
            48,
            gtk4::gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            48 * 4,
        );
        factory.connect_setup(move |_, item| {
            let item = item.downcast_ref::<gtk4::ListItem>().unwrap();
            let cover = TrackCover::new();
            crate::ui::track_list_row_interaction::expand_to_cell(&cover);
            cover.set_paintable(Some(&texture));
            item.set_child(Some(&cover));
            inherit(&cover_view, &cover);
        });
        view.append_column(
            &gtk4::ColumnViewColumn::builder()
                .title("Cover")
                .factory(&factory)
                .build(),
        );

        let text_factory = gtk4::SignalListItemFactory::new();
        let text_view = view.clone();
        text_factory.connect_setup(move |_, item| {
            let item = item.downcast_ref::<gtk4::ListItem>().unwrap();
            let label = gtk4::Label::new(Some("Track"));
            crate::ui::track_list_row_interaction::expand_to_cell(&label);
            item.set_child(Some(&label));
            inherit(&text_view, &label);
        });
        view.append_column(
            &gtk4::ColumnViewColumn::builder()
                .title("Title")
                .factory(&text_factory)
                .build(),
        );

        let rating_factory = gtk4::SignalListItemFactory::new();
        let rating_view = view.clone();
        rating_factory.connect_setup(move |_, item| {
            let item = item.downcast_ref::<gtk4::ListItem>().unwrap();
            let rating = RatingWidget::new();
            crate::ui::track_list_row_interaction::expand_to_cell(&rating);
            item.set_child(Some(&rating));
            inherit(&rating_view, &rating);
        });
        view.append_column(
            &gtk4::ColumnViewColumn::builder()
                .title("Rating")
                .factory(&rating_factory)
                .build(),
        );

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&view);
        install(&view);
        let window = gtk4::Window::builder().child(&root).build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        apply(&view, ListDensity::Compact);
        while gtk4::glib::MainContext::default().iteration(false) {}
        let cell = find_track_cell(view.upcast_ref()).unwrap();
        assert!(cell.has_css_class("reprise-density-compact"));
        let (_, compact, _, _) = view.measure(gtk4::Orientation::Vertical, -1);

        apply(&view, ListDensity::Standard);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(cell.has_css_class("reprise-density-standard"));
        let (_, standard, _, _) = view.measure(gtk4::Orientation::Vertical, -1);

        apply(&view, ListDensity::Comfortable);
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(cell.has_css_class("reprise-density-comfortable"));
        let (_, comfortable, _, _) = view.measure(gtk4::Orientation::Vertical, -1);

        assert_eq!(
            (standard - compact, comfortable - standard),
            (6, 8),
            "measured compact={compact}, standard={standard}, comfortable={comfortable}"
        );
        window.close();
    }
}
