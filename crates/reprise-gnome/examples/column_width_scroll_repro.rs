//! Standalone GTK reproduction for side-table columns (Releases, Podcasts,
//! Radio, Concerts) changing width as the user scrolls.
//!
//! Mirrors those tables exactly: a `GtkColumnView` whose columns are built
//! with `resizable(true)` + `expand(bool)` and **no** `set_fixed_width`, and
//! whose cells are ellipsizing labels. A column left at the default
//! `fixed-width = -1` "grows to fit its contents" — and the contents it can
//! see are only the cells realised right now. `GtkColumnView` recycles row
//! widgets while scrolling, so every batch of rows scrolled into view
//! re-measures the column and the whole table shifts sideways.
//!
//! This binary drives that measurement dependency directly — it swaps the
//! rows' text and re-measures — because the scroll *timing* itself cannot be
//! reproduced headlessly: without a compositor the frame clock stops ticking
//! after the first frame, GTK never trims the rows it realised up front, and
//! a scrolled `ColumnView` keeps every row alive (observed: 205 rows realised
//! for an 11-row viewport). The dependency below is the whole mechanism; the
//! scroll is only what triggers it in the live app.
//!
//! The music library does not shift because `column_layout.rs` gives every
//! track column a `set_fixed_width(..)`.
//!
//! Run: `xvfb-run -a cargo run -p reprise-gnome --example
//! column_width_scroll_repro` — the default run reproduces the drift and
//! exits non-zero; `--fixed-width` applies the music-library policy and must
//! stay stable.

use gtk4::prelude::*;
use gtk4::{gio, glib};

const PROBE_CLASSES: [&str; 4] = ["date", "title", "artist", "type"];
/// One viewport worth of rows, so every row is realised and the measurement
/// is not muddied by GTK's untrimmed overshoot.
const ROWS: usize = 12;
/// Rows near the top of a Releases sort, and rows further down. Scrolling
/// swaps the first band of row *data* into the same recycled cell widgets —
/// which is what this binary does directly.
const TOP_BAND: usize = 0;
const LOWER_BAND: usize = 400;
/// Widths the music library pins these columns to.
const ARTIST_FIXED_WIDTH: i32 = 260;
const TITLE_MIN_WIDTH: i32 = 120;
const DATE_FIXED_WIDTH: i32 = 160;
const TYPE_FIXED_WIDTH: i32 = 120;

fn artist_name(index: usize) -> String {
    if index < LOWER_BAND {
        format!("Air {index}")
    } else {
        format!("Godspeed You! Black Emperor {index}")
    }
}

/// Releases renders relative dates: short near the top of the sort, long
/// further down.
fn release_date(index: usize) -> String {
    if index < LOWER_BAND {
        "Today".to_owned()
    } else {
        format!("14 September 2026 (announced {index})")
    }
}

fn release_type(index: usize) -> String {
    if index < LOWER_BAND {
        "EP".to_owned()
    } else {
        "Compilation".to_owned()
    }
}

/// Widths of the realised cells carrying `class`, plus one sample of their text.
fn cell_widths(view: &gtk4::ColumnView, class: &str) -> (Vec<i32>, String) {
    fn walk(widget: &gtk4::Widget, class: &str, out: &mut Vec<i32>, sample: &mut String) {
        if widget.has_css_class(class) {
            if let Some(cell) = widget.parent() {
                out.push(cell.width());
            }
            if sample.is_empty() {
                if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
                    *sample = label.text().to_string();
                }
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            walk(&current, class, out, sample);
            child = current.next_sibling();
        }
    }
    let mut out = Vec::new();
    let mut sample = String::new();
    walk(
        view.upcast_ref::<gtk4::Widget>(),
        class,
        &mut out,
        &mut sample,
    );
    (out, sample)
}

/// The realised column width: every visible cell of the column shares it.
fn column_width(view: &gtk4::ColumnView, class: &str) -> i32 {
    cell_widths(view, class).0.into_iter().max().unwrap_or(0)
}

/// Every realised text of the cells carrying `class`, in tree order.
fn all_texts(view: &gtk4::ColumnView, class: &str) -> Vec<String> {
    fn walk(widget: &gtk4::Widget, class: &str, out: &mut Vec<String>) {
        if widget.has_css_class(class) {
            if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
                out.push(label.text().to_string());
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            walk(&current, class, out);
            child = current.next_sibling();
        }
    }
    let mut out = Vec::new();
    walk(view.upcast_ref::<gtk4::Widget>(), class, &mut out);
    out
}

fn report(view: &gtk4::ColumnView, when: &str) -> Vec<i32> {
    let widths: Vec<i32> = PROBE_CLASSES
        .iter()
        .map(|class| column_width(view, class))
        .collect();
    let samples: Vec<String> = PROBE_CLASSES
        .iter()
        .map(|class| cell_widths(view, class).1)
        .collect();
    println!("{when:>10}: {PROBE_CLASSES:?} = {widths:?}  sample={samples:?}");
    let titles = all_texts(view, "title");
    println!(
        "{:>10}  realised rows: {} first={:?} last={:?}",
        "",
        titles.len(),
        titles.first(),
        titles.last()
    );
    widths
}

/// Drives the main loop *blocking*, so frame-clock ticks (and with them the
/// relayout that rebinds recycled rows) actually get dispatched.
fn pump() {
    let done = std::rc::Rc::new(std::cell::Cell::new(false));
    let flag = done.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(600), move || {
        flag.set(true);
    });
    while !done.get() {
        glib::MainContext::default().iteration(true);
    }
}

fn text_column(
    view: &gtk4::ColumnView,
    title: &'static str,
    expand: bool,
    class: &'static str,
    render: impl Fn(usize) -> String + 'static,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(move |_, object| {
        let item = object.downcast_ref::<gtk4::ListItem>().unwrap();
        let label = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .css_classes([class])
            .build();
        item.set_child(Some(&label));
    });
    factory.connect_bind(move |_, object| {
        let item = object.downcast_ref::<gtk4::ListItem>().unwrap();
        let label = item.child().and_downcast::<gtk4::Label>().unwrap();
        let boxed = item
            .item()
            .and_downcast::<glib::BoxedAnyObject>()
            .expect("row object");
        let index: std::cell::Ref<usize> = boxed.borrow();
        label.set_text(&render(*index));
    });
    let column = gtk4::ColumnViewColumn::builder()
        .title(title)
        .factory(&factory)
        .resizable(true)
        .expand(expand)
        .build();
    view.append_column(&column);
    column
}

fn main() {
    let fixed_width = std::env::args().any(|arg| arg == "--fixed-width");
    gtk4::init().unwrap();

    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    for index in 0..ROWS {
        store.append(&glib::BoxedAnyObject::new(TOP_BAND + index));
    }
    let selection = gtk4::SingleSelection::new(Some(store.clone()));
    let column_view = gtk4::ColumnView::builder()
        .model(&selection)
        .show_row_separators(false)
        .show_column_separators(false)
        .build();

    // The live Releases column contract: Date, Title, Artist, Type.
    let date = text_column(&column_view, "Date", false, "date", release_date);
    let title = text_column(&column_view, "Title", true, "title", |index| {
        format!("Track {index}")
    });
    let artist = text_column(&column_view, "Artist", true, "artist", artist_name);
    let release_type_column = text_column(&column_view, "Type", false, "type", release_type);
    if fixed_width {
        // The music-library policy: pin every column, and let exactly one of
        // them (the filler) additionally absorb the leftover width.
        date.set_fixed_width(DATE_FIXED_WIDTH);
        release_type_column.set_fixed_width(TYPE_FIXED_WIDTH);
        artist.set_expand(false);
        artist.set_fixed_width(ARTIST_FIXED_WIDTH);
        title.set_fixed_width(TITLE_MIN_WIDTH);
    }

    let scroller = gtk4::ScrolledWindow::builder()
        .child(&column_view)
        .vexpand(true)
        .hexpand(true)
        .build();
    // `--window-width N` checks what pinning does when the pinned widths no
    // longer fit: the table must keep its columns and scroll horizontally
    // rather than squeeze them (the music library behaves the same, STYLE-6).
    let window_width = std::env::args()
        .skip_while(|arg| arg != "--window-width")
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(900);
    let window = gtk4::Window::builder()
        .default_width(window_width)
        .default_height(400)
        .child(&scroller)
        .build();
    window.present();
    pump();

    let before = report(&column_view, "top rows");

    // What a scroll does to the cells: the same recycled widgets get bound to
    // a different band of rows.
    for index in 0..ROWS {
        store.splice(
            index as u32,
            1,
            &[glib::BoxedAnyObject::new(LOWER_BAND + index)],
        );
    }
    pump();
    scroller.allocate(scroller.width(), scroller.height(), -1, None);
    pump();

    let after = report(&column_view, "lower rows");

    let drift: i32 = before
        .iter()
        .zip(after.iter())
        .map(|(before, after)| (after - before).abs())
        .max()
        .unwrap_or(0);
    println!("largest drift: {drift}px (fixed_width={fixed_width})");
    if fixed_width {
        assert_eq!(
            drift, 0,
            "fixed-width columns must not be re-measured from their cell contents"
        );
        println!("OK: pinned columns ignored the content change");
    } else {
        assert_ne!(
            drift, 0,
            "expected the unpinned columns to drift — the bug did not reproduce"
        );
        println!("REPRODUCED: unpinned columns re-measure from whatever rows are on screen");
    }
}
