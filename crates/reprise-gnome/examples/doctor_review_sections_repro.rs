//! Reproduces the Library Doctor album-section update contract.
//!
//! A normal run replaces one boxed row through `ListStore::splice`, which
//! makes `SortListModel` re-run its sort and section sorters. Pass
//! `--no-sections-changed` to mutate the boxed row silently and reproduce the
//! stale-section failure this example guards against.

use gtk4::glib;
use gtk4::prelude::*;

#[derive(Clone)]
struct Row {
    album: u32,
    position: u32,
}

fn compare(left: &glib::Object, right: &glib::Object, section_only: bool) -> gtk4::Ordering {
    let left = left
        .downcast_ref::<glib::BoxedAnyObject>()
        .unwrap()
        .borrow::<Row>();
    let right = right
        .downcast_ref::<glib::BoxedAnyObject>()
        .unwrap()
        .borrow::<Row>();
    let order = if section_only {
        left.album.cmp(&right.album)
    } else {
        (left.album, left.position).cmp(&(right.album, right.position))
    };
    match order {
        std::cmp::Ordering::Less => gtk4::Ordering::Smaller,
        std::cmp::Ordering::Equal => gtk4::Ordering::Equal,
        std::cmp::Ordering::Greater => gtk4::Ordering::Larger,
    }
}

fn sections(model: &gtk4::SortListModel) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    let mut position = 0;
    while position < model.n_items() {
        let range = model.section(position);
        ranges.push(range);
        position = range.1;
    }
    ranges
}

fn pump() {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(200);
    while std::time::Instant::now() < deadline {
        glib::MainContext::default().iteration(false);
    }
}

fn main() {
    gtk4::init().unwrap();
    let silent = std::env::args().any(|argument| argument == "--no-sections-changed");
    let store = gtk4::gio::ListStore::new::<glib::BoxedAnyObject>();
    for row in [
        Row {
            album: 0,
            position: 0,
        },
        Row {
            album: 0,
            position: 1,
        },
        Row {
            album: 1,
            position: 0,
        },
        Row {
            album: 2,
            position: 0,
        },
    ] {
        store.append(&glib::BoxedAnyObject::new(row));
    }
    let sorter = gtk4::CustomSorter::new(|left, right| compare(left, right, false));
    let sorted = gtk4::SortListModel::new(Some(store.clone()), Some(sorter));
    let section_sorter = gtk4::CustomSorter::new(|left, right| compare(left, right, true));
    sorted.set_section_sorter(Some(&section_sorter));
    let selection = gtk4::NoSelection::new(Some(sorted.clone()));
    let cells = gtk4::SignalListItemFactory::new();
    cells.connect_setup(|_, object| {
        let item = object.downcast_ref::<gtk4::ListItem>().unwrap();
        item.set_child(Some(&gtk4::Label::new(Some("change"))));
    });
    let rows = gtk4::ListView::new(Some(selection), Some(cells));
    let headers = gtk4::SignalListItemFactory::new();
    headers.connect_bind(|_, object| {
        let header = object.downcast_ref::<gtk4::ListHeader>().unwrap();
        header.set_child(Some(&gtk4::Label::new(Some(&format!(
            "album {}",
            header.start()
        )))));
    });
    rows.set_header_factory(Some(&headers));
    let window = gtk4::Window::builder()
        .default_width(400)
        .default_height(300)
        .child(&rows)
        .build();
    window.present();
    pump();
    assert_eq!(sections(&sorted), [(0, 2), (2, 3), (3, 4)]);

    if silent {
        let boxed = store
            .item(1)
            .unwrap()
            .downcast::<glib::BoxedAnyObject>()
            .unwrap();
        boxed.borrow_mut::<Row>().album = 1;
    } else {
        let replacement = glib::BoxedAnyObject::new(Row {
            album: 1,
            position: 1,
        });
        store.splice(1, 1, &[replacement]);
    }
    pump();
    let after = sections(&sorted);
    println!("sections after update: {after:?}");
    assert_eq!(after, [(0, 1), (1, 3), (3, 4)]);
}
