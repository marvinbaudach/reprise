//! Keyed reconciliation for the bounded GTK list stores outside the track
//! table. Equal rows keep their existing `glib::Object`, changed rows replace
//! in place, and insertions/removals emit only their affected ranges.

use std::ops::Range;

use gtk4::prelude::*;
use gtk4::{gio, glib};

pub(crate) fn replace<R, K>(
    store: &gio::ListStore,
    rows: Vec<R>,
    row_for_object: impl Fn(&glib::Object) -> R,
    key_for_row: impl Fn(&R) -> K,
    object_for_row: impl Fn(R) -> glib::Object,
) where
    R: Clone + PartialEq,
    K: Eq,
{
    let rows = rows.into_boxed_slice();
    let old_rows = (0..store.n_items())
        .map(|position| {
            let object = store
                .item(position)
                .expect("a published list-store position has an object");
            row_for_object(&object)
        })
        .collect::<Vec<_>>();
    let old_keys = old_rows.iter().map(&key_for_row).collect::<Vec<_>>();
    let new_keys = rows.iter().map(&key_for_row).collect::<Vec<_>>();
    let anchors = longest_common_subsequence(&old_keys, &new_keys);

    let mut old_cursor = 0;
    let mut new_cursor = 0;
    let mut position = 0;
    for (old_anchor, new_anchor) in anchors {
        splice_gap(
            store,
            position,
            old_cursor..old_anchor,
            new_cursor..new_anchor,
            rows.as_ref(),
            &object_for_row,
        );
        position += u32::try_from(new_anchor - new_cursor)
            .expect("bounded list-store delta length fits u32");

        if old_rows[old_anchor] != rows[new_anchor] {
            store.splice(position, 1, &[object_for_row(rows[new_anchor].clone())]);
        }
        position += 1;
        old_cursor = old_anchor + 1;
        new_cursor = new_anchor + 1;
    }
    splice_gap(
        store,
        position,
        old_cursor..old_rows.len(),
        new_cursor..rows.len(),
        rows.as_ref(),
        &object_for_row,
    );
}

fn splice_gap<R>(
    store: &gio::ListStore,
    position: u32,
    old: Range<usize>,
    new: Range<usize>,
    rows: &[R],
    object_for_row: &impl Fn(R) -> glib::Object,
) where
    R: Clone,
{
    if old.is_empty() && new.is_empty() {
        return;
    }
    let removed = u32::try_from(old.len()).expect("bounded list-store delta length fits u32");
    let additions = rows[new]
        .iter()
        .cloned()
        .map(object_for_row)
        .collect::<Vec<_>>();
    store.splice(position, removed, &additions);
}

fn longest_common_subsequence<K: Eq>(old: &[K], new: &[K]) -> Vec<(usize, usize)> {
    let mut lengths = vec![vec![0_u32; new.len() + 1]; old.len() + 1];
    for old_position in (0..old.len()).rev() {
        for new_position in (0..new.len()).rev() {
            lengths[old_position][new_position] = if old[old_position] == new[new_position] {
                lengths[old_position + 1][new_position + 1] + 1
            } else {
                lengths[old_position + 1][new_position].max(lengths[old_position][new_position + 1])
            };
        }
    }

    let mut anchors = Vec::with_capacity(lengths[0][0] as usize);
    let mut old_position = 0;
    let mut new_position = 0;
    while old_position < old.len() && new_position < new.len() {
        if old[old_position] == new[new_position] {
            anchors.push((old_position, new_position));
            old_position += 1;
            new_position += 1;
        } else if lengths[old_position + 1][new_position] >= lengths[old_position][new_position + 1]
        {
            old_position += 1;
        } else {
            new_position += 1;
        }
    }
    anchors
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gtk4::prelude::*;

    use super::{longest_common_subsequence, replace};

    #[test]
    fn anchors_keep_only_the_ordered_shared_identity() {
        assert_eq!(
            longest_common_subsequence(&[1, 2, 3, 4], &[0, 2, 3, 5]),
            [(1, 1), (2, 2)]
        );
    }

    #[test]
    fn additions_and_removals_emit_only_their_own_ranges() {
        let store = gtk4::gio::ListStore::new::<gtk4::glib::BoxedAnyObject>();
        replace(
            &store,
            vec![1_i32, 2, 3, 4],
            boxed_i32,
            |value| *value,
            boxed_object,
        );
        let changes = Rc::new(RefCell::new(Vec::new()));
        {
            let changes = changes.clone();
            store.connect_items_changed(move |_, at, removed, added| {
                changes.borrow_mut().push((at, removed, added));
            });
        }

        replace(
            &store,
            vec![0_i32, 1, 3, 4, 5],
            boxed_i32,
            |value| *value,
            boxed_object,
        );

        assert_eq!(
            changes.borrow().as_slice(),
            [(0, 0, 1), (2, 1, 0), (4, 0, 1)]
        );
    }

    fn boxed_i32(object: &gtk4::glib::Object) -> i32 {
        *object
            .clone()
            .downcast::<gtk4::glib::BoxedAnyObject>()
            .expect("test store contains boxed integers")
            .borrow::<i32>()
    }

    fn boxed_object(value: i32) -> gtk4::glib::Object {
        gtk4::glib::BoxedAnyObject::new(value).upcast()
    }
}

#[cfg(test)]
pub(crate) mod display_test {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use gtk4::prelude::*;

    pub(crate) fn assert_viewport_selection_and_noop_bind_count(
        selection: gtk4::SelectionModel,
        change_one_row: impl FnOnce(),
        repeat_identical_rows: impl FnOnce(),
        selection_survived: impl Fn() -> bool,
    ) {
        let binds = Rc::new(Cell::new(0_u32));
        let factory = gtk4::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            item.downcast_ref::<gtk4::ListItem>()
                .expect("factory setup receives a list item")
                .set_child(Some(&gtk4::Label::new(Some("row"))));
        });
        {
            let binds = binds.clone();
            factory.connect_bind(move |_, _| binds.set(binds.get() + 1));
        }
        let list = gtk4::ListView::new(Some(selection), Some(factory));
        let scrolled = gtk4::ScrolledWindow::builder()
            .min_content_height(180)
            .child(&list)
            .build();
        let window = gtk4::Window::builder()
            .default_width(320)
            .default_height(180)
            .child(&scrolled)
            .build();
        window.present();
        let adjustment = scrolled.vadjustment();
        assert!(
            crate::ui::test_settle::settle_until(
                crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
                || adjustment.upper() > adjustment.page_size()
            ),
            "the list did not allocate a scrollable range"
        );
        list.scroll_to(75, gtk4::ListScrollFlags::NONE, None);
        assert!(
            crate::ui::test_settle::settle_until(
                crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
                || adjustment.value() > 0.0
            ),
            "the list did not leave the top"
        );
        let before = adjustment.value();

        change_one_row();
        crate::ui::test_settle::settle_for(Duration::from_millis(100));
        assert!(selection_survived(), "the selected row did not survive");
        assert!(
            (adjustment.value() - before).abs() <= 1.0,
            "one offscreen row update moved the viewport from {before} to {}",
            adjustment.value()
        );

        binds.set(0);
        repeat_identical_rows();
        crate::ui::test_settle::settle_for(Duration::from_millis(100));
        assert_eq!(binds.get(), 0, "an identical refresh rebound widgets");
        window.close();
    }
}
