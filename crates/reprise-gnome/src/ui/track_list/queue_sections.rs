//! QUE-1: GTK section headers for the Queue view.
//!
//! The toolkit-free queue model lives in the sibling `queue_model` module
//! and is re-exported here so existing GTK call sites keep their established
//! path. This adapter owns only GTK rendering and its GTK-bound regressions.

use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::strings;
use crate::ui::track_list::Shared;

#[cfg(test)]
pub(crate) use super::queue_model::compose;
pub(crate) use super::queue_model::{
    compose_virtual, section_ranges, ContextWindow, QueueSection, QueueSectionKind, QueueViewModel,
    VirtualContext,
};
#[cfg(test)]
use reprise_core::up_next::QueueItem;

/// The section title shown for the section starting at `start`.
pub(crate) fn header_title(sections: &[QueueSection], start: u32) -> String {
    let section = sections.iter().find(|section| section.start == start);
    match section {
        Some(QueueSection {
            kind: QueueSectionKind::NowPlaying,
            ..
        }) => strings::text(strings::QUEUE_SECTION_NOW_PLAYING),
        Some(QueueSection {
            kind: QueueSectionKind::PlayNext,
            ..
        }) => strings::text(strings::QUEUE_SECTION_PLAY_NEXT),
        Some(QueueSection {
            len,
            kind: QueueSectionKind::UpNext { source_label },
            ..
        }) => strings::queue_context_tail(source_label, *len as usize),
        None => String::new(),
    }
}

/// Installs (or removes) the Queue view's section header factory. Only the
/// Queue source renders sections — every other source gets its factory
/// cleared again, mirroring how `artist_master.rs` toggles its alphabet
/// headers. Called from `reload` AFTER the query swap (never between
/// `set_sections` and `set_query` — see `reload`'s comment at the call
/// site: the factory flip makes GTK re-match section headers synchronously
/// and must only see a consistent sections/row-count pair).
///
/// A no-op when the view is already in the requested state: queue reloads
/// happen on every queue mutation (auto-advance, reorder, remove), and
/// re-setting a fresh factory each time would tear down and rebuild every
/// visible section header for no benefit. The bind closure reads the
/// CURRENT `shared.queue_sections` at bind time, so a reused factory always
/// titles the sections the latest reload declared.
pub(in crate::ui) fn apply_queue_header_factory(shared: &Rc<Shared>, is_queue: bool) {
    let has_factory = shared.column_view.header_factory().is_some();
    if is_queue == has_factory {
        return;
    }
    if !is_queue {
        shared
            .column_view
            .set_header_factory(gtk4::ListItemFactory::NONE);
        return;
    }
    let factory = gtk4::SignalListItemFactory::new();
    {
        let shared = Rc::downgrade(shared);
        factory.connect_bind(move |_, header| {
            let Some(header) = header.downcast_ref::<gtk4::ListHeader>() else {
                return;
            };
            let Some(shared) = shared.upgrade() else {
                return;
            };
            let (title, is_play_next) = {
                let sections = shared.queue_sections.borrow();
                let is_play_next = sections.iter().any(|section| {
                    section.start == header.start() && section.kind == QueueSectionKind::PlayNext
                });
                (header_title(&sections, header.start()), is_play_next)
            };
            let label = gtk4::Label::builder()
                .label(&title)
                .xalign(0.0)
                .hexpand(true)
                .css_classes(["queue-section-header", "heading"])
                .build();
            if !is_play_next {
                header.set_child(Some(&label));
                return;
            }
            // QUE-3: the Play Next header carries the "Clear" button — it
            // empties exactly the section it titles, nothing else. A real
            // `gtk::Button` (flat), never a click gesture on a Box (see the
            // gtk4 skill's cell-input rule; headers recycle like cells).
            let clear = gtk4::Button::builder()
                .label(strings::text(strings::QUEUE_CLEAR_PLAY_NEXT))
                .has_frame(false)
                .css_classes(["flat", "queue-clear-play-next"])
                .build();
            {
                let player = shared.player.borrow().clone();
                clear.connect_clicked(move |_| match player.upgrade() {
                    Some(player) => player.clear_play_next(),
                    None => tracing::warn!("clear play-next clicked without a player"),
                });
            }
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            row.append(&label);
            row.append(&clear);
            header.set_child(Some(&row));
        });
    }
    factory.connect_unbind(|_, header| {
        if let Some(header) = header.downcast_ref::<gtk4::ListHeader>() {
            header.set_child(gtk4::Widget::NONE);
        }
    });
    shared.column_view.set_header_factory(Some(&factory));
}

#[cfg(test)]
mod que_7_tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    fn tracks(ids: &[i64]) -> Vec<QueueItem> {
        ids.iter().copied().map(QueueItem::Track).collect()
    }

    struct RecordingContextWindow {
        requested: Rc<Cell<usize>>,
    }

    impl ContextWindow for RecordingContextWindow {
        fn rows(&self, offset: usize, limit: usize) -> Vec<QueueItem> {
            self.requested.set(limit);
            (offset..offset + limit)
                .map(|position| QueueItem::Track(i64::try_from(position).unwrap()))
                .collect()
        }
    }

    #[test]
    fn que_7_context_tail_is_not_materialised() {
        let requested = Rc::new(Cell::new(0));
        let context = VirtualContext::new(1_638);
        let window = RecordingContextWindow {
            requested: requested.clone(),
        };
        let model = compose_virtual(
            Some(QueueItem::Track(1)),
            &tracks(&[10, 11]),
            Some(context),
            Some("Music"),
        );

        assert_eq!(model.items, tracks(&[1, 10, 11]));
        assert_eq!(model.total_len(), 1_641);
        assert_eq!(requested.get(), 0);

        let items = model.items_window(203, 20, &window);
        assert_eq!(items.len(), 20);
        assert_eq!(requested.get(), 20);
    }
}

#[cfg(test)]
mod que_10_tests {
    use super::*;

    #[test]
    fn episode_context_skip_is_one_leading_removal() {
        let old = compose_virtual(
            Some(QueueItem::Episode(19)),
            &[QueueItem::Track(10)],
            Some(VirtualContext::identified(2, (42, 7), 0)),
            Some("VOID PREACHER"),
        )
        .upcoming();
        let new = compose_virtual(
            Some(QueueItem::Episode(20)),
            &[QueueItem::Track(10)],
            Some(VirtualContext::identified(1, (42, 7), 1)),
            Some("VOID PREACHER"),
        )
        .upcoming();

        assert_eq!(new.leading_removal_change_from(&old), Some((1, 1, 0)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_title_resolves_up_next_label_and_unknown_start_degrades() {
        let model = compose(Some(QueueItem::Track(1)), &[], &[2], Some("Neverbloom"));
        assert_eq!(
            header_title(&model.sections, 1),
            "Playing from Neverbloom · 1 track"
        );
        assert_eq!(header_title(&model.sections, 99), String::new());
    }
}
