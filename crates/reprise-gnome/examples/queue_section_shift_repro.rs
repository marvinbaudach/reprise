//! Standalone GTK reproduction for the Queue-view section headers going
//! stale after an `items_changed`-only queue advance.
//!
//! Mirrors production exactly: a `GtkSectionModel` behind a
//! `GtkMultiSelection` in a `GtkColumnView` with a header factory, first
//! composed as Now Playing (1) + Play Next (3) + context (2), then advanced
//! by removing the leading row and re-declaring the shifted section ranges.
//!
//! Why an example and not a test: the `gtk::SectionModel` interface is
//! compiled out of `TrackListModel` under `cfg(test)` (its `interface_init`
//! asserts the registering thread ran `gtk4::init()`, which `cargo test`'s
//! worker threads race for), so no in-crate test can exercise section
//! headers at all. This binary runs against the real interface.
//!
//! Run: `xvfb-run -a cargo run -p reprise-gnome --example
//! queue_section_shift_repro`. It exits non-zero unless the advance keeps
//! all three headers; pass `--no-sections-changed` to watch the bug itself
//! (Play Next disappears and its rows fall under "Now Playing").

use std::cell::RefCell;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{gio, glib};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ReproModel {
        pub items: RefCell<Vec<String>>,
        pub sections: RefCell<Vec<(u32, u32)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ReproModel {
        const NAME: &'static str = "ReproSectionModel";
        type Type = super::ReproModel;
        type ParentType = glib::Object;
        type Interfaces = (gio::ListModel, gtk4::SectionModel);
    }

    impl ObjectImpl for ReproModel {}

    impl ListModelImpl for ReproModel {
        fn item_type(&self) -> glib::Type {
            glib::BoxedAnyObject::static_type()
        }
        fn n_items(&self) -> u32 {
            self.items.borrow().len() as u32
        }
        fn item(&self, position: u32) -> Option<glib::Object> {
            self.items
                .borrow()
                .get(position as usize)
                .map(|title| glib::BoxedAnyObject::new(title.clone()).upcast())
        }
    }

    impl SectionModelImpl for ReproModel {
        fn section(&self, position: u32) -> (u32, u32) {
            let sections = self.sections.borrow();
            for &(start, end) in sections.iter() {
                if position >= start && position < end {
                    return (start, end);
                }
            }
            let last_end = sections.iter().map(|&(_, end)| end).max().unwrap_or(0);
            let total = self.items.borrow().len() as u32;
            (last_end, total.max(position.saturating_add(1)))
        }
    }
}

glib::wrapper! {
    pub struct ReproModel(ObjectSubclass<imp::ReproModel>)
        @implements gio::ListModel, gtk4::SectionModel;
}

impl ReproModel {
    fn new(items: &[&str], sections: &[(u32, u32)]) -> Self {
        let model: Self = glib::Object::new();
        *model.imp().items.borrow_mut() = items.iter().map(|s| (*s).to_owned()).collect();
        *model.imp().sections.borrow_mut() = sections.to_vec();
        model
    }

    fn advance(&self, items: &[&str], sections: &[(u32, u32)], emit_sections_changed: bool) {
        *self.imp().items.borrow_mut() = items.iter().map(|s| (*s).to_owned()).collect();
        *self.imp().sections.borrow_mut() = sections.to_vec();
        self.items_changed(0, 1, 0);
        if emit_sections_changed {
            self.sections_changed(0, self.n_items());
        }
    }
}

/// Section titles keyed by `(start, end, title)`, as the header factory
/// resolves them.
type SectionTitles = &'static [(u32, u32, &'static str)];

fn title_for(sections: &[(u32, u32, &str)], start: u32) -> String {
    sections
        .iter()
        .find(|(section_start, _, _)| *section_start == start)
        .map_or_else(String::new, |(_, _, title)| (*title).to_owned())
}

fn rendered_headers(widget: &gtk4::Widget) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(widget: &gtk4::Widget, out: &mut Vec<String>) {
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            if label.has_css_class("section-header") {
                out.push(label.label().to_string());
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            walk(&current, out);
            child = current.next_sibling();
        }
    }
    walk(widget, &mut out);
    out
}

fn pump() {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(400);
    while std::time::Instant::now() < deadline {
        glib::MainContext::default().iteration(false);
    }
}

fn main() {
    let emit_sections_changed = !std::env::args().any(|arg| arg == "--no-sections-changed");
    gtk4::init().unwrap();

    // Titles keyed by section start, for both the before and after states.
    const BEFORE: SectionTitles = &[
        (0, 1, "Now Playing"),
        (1, 4, "Play Next"),
        (4, 6, "Playing from Music"),
    ];
    const AFTER: SectionTitles = &[
        (0, 1, "Now Playing"),
        (1, 3, "Play Next"),
        (3, 5, "Playing from Music"),
    ];
    let live_titles: std::rc::Rc<RefCell<SectionTitles>> = std::rc::Rc::new(RefCell::new(BEFORE));

    let model = ReproModel::new(
        &["np", "next-1", "next-2", "next-3", "ctx-1", "ctx-2"],
        &[(0, 1), (1, 4), (4, 6)],
    );
    let selection = gtk4::MultiSelection::new(Some(model.clone()));
    let column_view = gtk4::ColumnView::new(Some(selection));

    let cell_factory = gtk4::SignalListItemFactory::new();
    cell_factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<gtk4::ListItem>().unwrap();
        item.set_child(Some(&gtk4::Label::new(None)));
    });
    cell_factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk4::ListItem>().unwrap();
        let boxed = item
            .item()
            .unwrap()
            .downcast::<glib::BoxedAnyObject>()
            .unwrap();
        let title: std::cell::Ref<String> = boxed.borrow();
        item.child()
            .unwrap()
            .downcast::<gtk4::Label>()
            .unwrap()
            .set_label(&title);
    });
    column_view.append_column(&gtk4::ColumnViewColumn::new(
        Some("Title"),
        Some(cell_factory),
    ));

    let header_factory = gtk4::SignalListItemFactory::new();
    {
        let live_titles = live_titles.clone();
        header_factory.connect_bind(move |_, header| {
            let header = header.downcast_ref::<gtk4::ListHeader>().unwrap();
            let label = gtk4::Label::builder()
                .label(title_for(&live_titles.borrow(), header.start()))
                .css_classes(["section-header"])
                .build();
            header.set_child(Some(&label));
        });
    }
    column_view.set_header_factory(Some(&header_factory));

    let scroller = gtk4::ScrolledWindow::builder().child(&column_view).build();
    let window = gtk4::Window::builder()
        .default_width(600)
        .default_height(500)
        .child(&scroller)
        .build();
    window.present();
    pump();

    let before = rendered_headers(column_view.upcast_ref::<gtk4::Widget>());
    println!("before advance: {before:?}");
    assert_eq!(
        before,
        ["Now Playing", "Play Next", "Playing from Music"],
        "precondition: the composed queue renders all three headers"
    );

    *live_titles.borrow_mut() = AFTER;
    model.advance(
        &["next-1", "next-2", "next-3", "ctx-1", "ctx-2"],
        &[(0, 1), (1, 3), (3, 5)],
        emit_sections_changed,
    );
    pump();

    let after = rendered_headers(column_view.upcast_ref::<gtk4::Widget>());
    println!("after advance (sections_changed={emit_sections_changed}): {after:?}");
    let expected = ["Now Playing", "Play Next", "Playing from Music"];
    if emit_sections_changed {
        assert_eq!(
            after, expected,
            "advancing must not merge Play Next into the Now Playing section"
        );
        println!("OK: every section header survived the advance");
    } else {
        println!("expected with the fix: {expected:?}");
    }
}
