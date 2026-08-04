//! QUE-1: GTK section headers for the Queue view.
//!
//! The toolkit-free queue model lives in the sibling `queue_model` module
//! and is re-exported here so existing GTK call sites keep their established
//! path. This adapter owns only GTK rendering and its GTK-bound regressions.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::up_next::QueueItem;

use crate::ui::strings;
use crate::ui::track_list::Shared;

use reprise_view::queue as queue_model;
pub(crate) use reprise_view::queue::{
    section_ranges, ContextWindow, QueueSection, QueueSectionKind, QueueViewModel, VirtualContext,
};

#[cfg(test)]
pub(crate) fn compose(
    now_playing: Option<QueueItem>,
    play_next: &[QueueItem],
    up_next_rest: &[i64],
    origin_label: Option<&str>,
) -> QueueViewModel {
    let fallback = strings::text(strings::SIDEBAR_MUSIC);
    queue_model::compose(
        now_playing,
        play_next,
        up_next_rest,
        origin_label,
        &fallback,
    )
}

pub(crate) fn compose_virtual(
    now_playing: Option<QueueItem>,
    play_next: &[QueueItem],
    context: Option<VirtualContext>,
    origin_label: Option<&str>,
) -> QueueViewModel {
    // Only translate when the model can actually reach for the fallback. Before
    // the extraction this lookup sat inside `origin_label.map_or_else(…)` and
    // ran only when there was no label; `compose_virtual` runs on every queue
    // mutation, so making it unconditional would have put a gettext call on
    // that path for nothing. The remaining miss — no label and no context, where
    // the model builds no Up Next section at all — would need `VirtualContext`
    // to expose its count, which is not worth widening the type for.
    let fallback = if origin_label.is_none() {
        strings::text(strings::SIDEBAR_MUSIC)
    } else {
        String::new()
    };
    queue_model::compose_virtual(now_playing, play_next, context, origin_label, &fallback)
}

fn render_message(message: &reprise_view::strings::Message) -> String {
    let args = message
        .args
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect::<Vec<_>>();
    match &message.plural {
        Some(plural) => strings::plural(
            message.id,
            plural.id,
            usize::try_from(plural.count).unwrap_or(usize::MAX),
            &args,
        ),
        None => strings::formatted(message.id, &args),
    }
}

/// The section title shown for the section starting at `start`.
pub(crate) fn header_title(sections: &[QueueSection], start: u32) -> String {
    queue_model::header_title(sections, start)
        .as_ref()
        .map_or_else(String::new, render_message)
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
    fn header_titles_preserve_all_three_rendered_forms() {
        let model = compose(
            Some(QueueItem::Track(1)),
            &[QueueItem::Track(2), QueueItem::Track(3)],
            &[4, 5],
            Some("Neverbloom"),
        );
        assert_eq!(header_title(&model.sections, 0), "Now Playing");
        assert_eq!(header_title(&model.sections, 1), "Play Next");
        assert_eq!(
            header_title(&model.sections, 3),
            "Playing from Neverbloom · 2 tracks"
        );
        assert_eq!(header_title(&model.sections, 99), String::new());
    }

    #[test]
    fn missing_origin_uses_the_surface_rendered_music_label() {
        let model = compose(None, &[], &[2], None);

        assert_eq!(
            header_title(&model.sections, 0),
            "Playing from Music · 1 track"
        );
    }
}
