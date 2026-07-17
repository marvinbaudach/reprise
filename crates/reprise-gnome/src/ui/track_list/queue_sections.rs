//! QUE-1: the Queue view's composite model — Now Playing, Play Next, and
//! "Up Next · from <source>" — composed from the controller's three lists
//! into one flat id list plus section ranges. The pure composition lives
//! here (testable without GTK); `wire_queue_header_factory` renders the
//! section titles through `ColumnView`'s header factory, driven by the
//! `gtk::SectionModel` ranges `TrackListModel` exposes for the Queue source.
//!
//! QUE-2 by construction: the display order is exactly the play order —
//! `next_target` pops Play Next FIFO first, then walks the snapshot from
//! the current position, which is precisely `play_next ++ up_next_rest`.

use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::strings;
use crate::ui::track_list::Shared;

/// What a queue section IS — drives its header title (and, later, the
/// header's actions: QUE-3 puts "Clear" on the Play Next header).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum QueueSectionKind {
    NowPlaying,
    PlayNext,
    UpNext { source_label: String },
}

/// One contiguous section of the composite Queue view.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QueueSection {
    pub start: u32,
    pub len: u32,
    pub kind: QueueSectionKind,
}

/// The composite Queue view: the flat id list the windowed query renders,
/// plus the section ranges the header factory titles.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct QueueViewModel {
    pub ids: Vec<i64>,
    pub sections: Vec<QueueSection>,
}

/// Composes the three queue parts into display order (QUE-1). Sections are
/// emitted only when non-empty; an entirely empty composition (nothing
/// playing, nothing pending) yields the empty model that routes the view to
/// the QUE-4 StatusPage.
pub(crate) fn compose(
    now_playing: Option<i64>,
    play_next: &[i64],
    up_next_rest: &[i64],
    origin_label: Option<&str>,
) -> QueueViewModel {
    let mut ids = Vec::with_capacity(
        usize::from(now_playing.is_some()) + play_next.len() + up_next_rest.len(),
    );
    let mut sections = Vec::new();

    if let Some(current) = now_playing {
        sections.push(QueueSection {
            start: 0,
            len: 1,
            kind: QueueSectionKind::NowPlaying,
        });
        ids.push(current);
    }
    if !play_next.is_empty() {
        sections.push(QueueSection {
            start: u32::try_from(ids.len()).unwrap_or(u32::MAX),
            len: u32::try_from(play_next.len()).unwrap_or(u32::MAX),
            kind: QueueSectionKind::PlayNext,
        });
        ids.extend_from_slice(play_next);
    }
    if !up_next_rest.is_empty() {
        sections.push(QueueSection {
            start: u32::try_from(ids.len()).unwrap_or(u32::MAX),
            len: u32::try_from(up_next_rest.len()).unwrap_or(u32::MAX),
            kind: QueueSectionKind::UpNext {
                source_label: origin_label
                    .map_or_else(|| strings::text(strings::SIDEBAR_MUSIC), str::to_owned),
            },
        });
        ids.extend_from_slice(up_next_rest);
    }

    QueueViewModel { ids, sections }
}

/// The `(start, end)` ranges `gtk::SectionModel::section(position)` answers
/// from — half-open, in model coordinates.
pub(crate) fn section_ranges(sections: &[QueueSection]) -> Vec<(u32, u32)> {
    sections
        .iter()
        .map(|section| (section.start, section.start.saturating_add(section.len)))
        .collect()
}

/// The section title shown for the section starting at `start`.
pub(crate) fn header_title(sections: &[QueueSection], start: u32) -> String {
    let kind = sections
        .iter()
        .find(|section| section.start == start)
        .map(|section| &section.kind);
    match kind {
        Some(QueueSectionKind::NowPlaying) => strings::text(strings::QUEUE_SECTION_NOW_PLAYING),
        Some(QueueSectionKind::PlayNext) => strings::text(strings::QUEUE_SECTION_PLAY_NEXT),
        Some(QueueSectionKind::UpNext { source_label }) => {
            strings::text(strings::QUEUE_SECTION_UP_NEXT_FROM).replace("{}", source_label)
        }
        None => String::new(),
    }
}

/// Installs (or removes) the Queue view's section header factory. Only the
/// Queue source renders sections — every other source gets its factory
/// cleared again, mirroring how `artist_master.rs` toggles its alphabet
/// headers. Called from `reload` on every source switch.
pub(in crate::ui) fn apply_queue_header_factory(shared: &Rc<Shared>, is_queue: bool) {
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
            let title = {
                let sections = shared.queue_sections.borrow();
                header_title(&sections, header.start())
            };
            let label = gtk4::Label::builder()
                .label(&title)
                .xalign(0.0)
                .css_classes(["queue-section-header", "heading"])
                .build();
            header.set_child(Some(&label));
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
mod tests {
    use super::*;

    #[test]
    fn compose_builds_three_sections_in_display_order() {
        let model = compose(Some(1), &[2, 3], &[4, 5, 6], Some("Late Night"));
        assert_eq!(model.ids, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(model.sections.len(), 3);
        assert_eq!(model.sections[0].kind, QueueSectionKind::NowPlaying);
        assert_eq!((model.sections[0].start, model.sections[0].len), (0, 1));
        assert_eq!(model.sections[1].kind, QueueSectionKind::PlayNext);
        assert_eq!((model.sections[1].start, model.sections[1].len), (1, 2));
        assert_eq!(
            model.sections[2].kind,
            QueueSectionKind::UpNext {
                source_label: "Late Night".into()
            }
        );
        assert_eq!((model.sections[2].start, model.sections[2].len), (3, 3));
        assert_eq!(
            section_ranges(&model.sections),
            vec![(0, 1), (1, 3), (3, 6)]
        );
    }

    #[test]
    fn compose_omits_empty_play_next_per_que1() {
        let model = compose(Some(9), &[], &[10], Some("Neverbloom"));
        assert_eq!(model.ids, vec![9, 10]);
        assert_eq!(model.sections.len(), 2);
        assert_eq!(model.sections[1].start, 1);
    }

    #[test]
    fn compose_without_playback_still_lists_pending_play_next() {
        // Stopped but manually queued tracks exist: no Now Playing row, the
        // pending section still shows (QUE-4's StatusPage is only for the
        // fully empty case).
        let model = compose(None, &[7], &[], None);
        assert_eq!(model.ids, vec![7]);
        assert_eq!(model.sections.len(), 1);
        assert_eq!(model.sections[0].kind, QueueSectionKind::PlayNext);
    }

    #[test]
    fn compose_fully_empty_yields_the_empty_model() {
        let model = compose(None, &[], &[], None);
        assert!(model.ids.is_empty());
        assert!(model.sections.is_empty());
    }

    #[test]
    fn header_title_resolves_up_next_label_and_unknown_start_degrades() {
        let model = compose(Some(1), &[], &[2], Some("Neverbloom"));
        assert!(header_title(&model.sections, 1).contains("Neverbloom"));
        assert_eq!(header_title(&model.sections, 99), String::new());
    }
}
