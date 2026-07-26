//! QUE-1: the Queue view's composite model — Now Playing, Play Next, and
//! a named virtual playback-context tail — composed from the controller
//! into one flat id list plus section ranges. The pure composition lives
//! here (testable without GTK); `wire_queue_header_factory` renders the
//! section titles through `ColumnView`'s header factory, driven by the
//! `gtk::SectionModel` ranges `TrackListModel` exposes for the Queue source.
//!
//! QUE-2 by construction: the display order is exactly the play order —
//! `next_target` pops Play Next FIFO first, then walks the snapshot from
//! the current position, which is precisely `play_next ++ up_next_rest`.

use std::fmt;
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
#[derive(Clone, Default)]
pub(crate) struct QueueViewModel {
    /// Materialized prefix only: optional Now Playing followed by manual
    /// entries. Context rows are supplied window-by-window by `context`.
    pub ids: Vec<i64>,
    pub sections: Vec<QueueSection>,
    context: Option<VirtualContextTail>,
}

#[derive(Clone)]
pub(crate) struct VirtualContextTail {
    count: usize,
    window: Rc<dyn Fn(usize, usize) -> Vec<i64>>,
    identity: Option<VirtualContextIdentity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VirtualContextIdentity {
    sequence: (u64, u64),
    start: usize,
}

impl VirtualContextTail {
    #[cfg(test)]
    pub(crate) fn new(count: usize, window: Rc<dyn Fn(usize, usize) -> Vec<i64>>) -> Self {
        Self {
            count,
            window,
            identity: None,
        }
    }

    pub(crate) fn identified(
        count: usize,
        sequence: (u64, u64),
        start: usize,
        window: Rc<dyn Fn(usize, usize) -> Vec<i64>>,
    ) -> Self {
        Self {
            count,
            window,
            identity: Some(VirtualContextIdentity { sequence, start }),
        }
    }
}

impl fmt::Debug for QueueViewModel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QueueViewModel")
            .field("ids", &self.ids)
            .field("sections", &self.sections)
            .field(
                "context_count",
                &self.context.as_ref().map(|tail| tail.count),
            )
            .finish()
    }
}

impl PartialEq for QueueViewModel {
    fn eq(&self, other: &Self) -> bool {
        self.ids == other.ids
            && self.sections == other.sections
            && self.context.as_ref().map(|tail| tail.count)
                == other.context.as_ref().map(|tail| tail.count)
    }
}

impl QueueViewModel {
    /// The shared queue projection used by the compact panel: the exact same
    /// model with the optional Now Playing prefix removed and section offsets
    /// rebased. No queue composition is repeated in the panel.
    pub(crate) fn upcoming(&self) -> Self {
        let now_playing_len = self
            .sections
            .first()
            .filter(|section| section.kind == QueueSectionKind::NowPlaying)
            .map_or(0, |section| section.len);
        let skip = usize::try_from(now_playing_len).unwrap_or(self.ids.len());
        let ids = self.ids.get(skip..).unwrap_or_default().to_vec();
        let sections = self
            .sections
            .iter()
            .filter(|section| section.kind != QueueSectionKind::NowPlaying)
            .map(|section| QueueSection {
                start: section.start.saturating_sub(now_playing_len),
                len: section.len,
                kind: section.kind.clone(),
            })
            .collect();
        Self {
            ids,
            sections,
            context: self.context.clone(),
        }
    }

    pub(crate) fn sidebar_count(&self) -> usize {
        self.sections
            .iter()
            .find(|section| section.kind == QueueSectionKind::PlayNext)
            .map_or(0, |section| section.len as usize)
    }

    pub(crate) fn total_len(&self) -> usize {
        self.ids.len() + self.context.as_ref().map_or(0, |tail| tail.count)
    }

    pub(crate) fn ids_window(&self, offset: usize, limit: usize) -> Vec<i64> {
        if limit == 0 || offset >= self.total_len() {
            return Vec::new();
        }
        let mut ids = Vec::with_capacity(limit.min(self.total_len() - offset));
        if offset < self.ids.len() {
            let end = offset.saturating_add(limit).min(self.ids.len());
            ids.extend_from_slice(&self.ids[offset..end]);
        }
        let remaining = limit.saturating_sub(ids.len());
        if remaining == 0 {
            return ids;
        }
        let Some(context) = &self.context else {
            return ids;
        };
        let context_offset = offset.saturating_sub(self.ids.len());
        let context_limit = remaining.min(context.count.saturating_sub(context_offset));
        ids.extend((context.window)(context_offset, context_limit));
        ids
    }

    pub(crate) fn all_ids(&self) -> Vec<i64> {
        self.ids_window(0, self.total_len())
    }

    /// Exact O(1) model delta for the two normal forward-playback shapes:
    /// consuming the first materialized Play Next row, or advancing through
    /// an unchanged virtual context. The lazy tail closure is live, so its
    /// frozen sequence/start identity is the proof for the virtual case.
    pub(crate) fn leading_removal_change_from(&self, old: &Self) -> Option<(u32, u32, u32)> {
        let material_removed = old.ids.len().checked_sub(self.ids.len())?;
        let context_unchanged = match (&old.context, &self.context) {
            (None, None) => true,
            (Some(old), Some(new)) => {
                old.identity.is_some() && old.identity == new.identity && old.count == new.count
            }
            _ => false,
        };
        if material_removed > 0
            && context_unchanged
            && old.ids.get(material_removed..) == Some(self.ids.as_slice())
        {
            return Some((0, u32::try_from(material_removed).unwrap_or(u32::MAX), 0));
        }
        if material_removed != 0 {
            return None;
        }
        let old_identity = old.context.as_ref()?.identity?;
        let new_identity = self.context.as_ref()?.identity?;
        if old_identity.sequence != new_identity.sequence || new_identity.start < old_identity.start
        {
            return None;
        }
        let removed = new_identity.start - old_identity.start;
        if old.context.as_ref()?.count != self.context.as_ref()?.count.saturating_add(removed) {
            return None;
        }
        if removed == 0 {
            return Some((0, 0, 0));
        }
        Some((
            u32::try_from(self.ids.len()).unwrap_or(u32::MAX),
            u32::try_from(removed).unwrap_or(u32::MAX),
            0,
        ))
    }
}

/// Composes the three queue parts into display order (QUE-1). Sections are
/// emitted only when non-empty; an entirely empty composition (nothing
/// playing, nothing pending) yields the empty model that routes the view to
/// the QUE-4 StatusPage.
#[cfg(test)]
pub(crate) fn compose(
    now_playing: Option<i64>,
    play_next: &[i64],
    up_next_rest: &[i64],
    origin_label: Option<&str>,
) -> QueueViewModel {
    let context_ids: Rc<[i64]> = Rc::from(up_next_rest);
    let context_for_window = context_ids.clone();
    let context = (!context_ids.is_empty()).then(|| {
        VirtualContextTail::new(
            context_ids.len(),
            Rc::new(move |offset, limit| {
                let end = offset.saturating_add(limit).min(context_for_window.len());
                context_for_window
                    .get(offset..end)
                    .unwrap_or_default()
                    .to_vec()
            }),
        )
    });
    compose_virtual(now_playing, play_next, context, origin_label)
}

pub(crate) fn compose_virtual(
    now_playing: Option<i64>,
    play_next: &[i64],
    context: Option<VirtualContextTail>,
    origin_label: Option<&str>,
) -> QueueViewModel {
    let mut ids = Vec::with_capacity(usize::from(now_playing.is_some()) + play_next.len());
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
    let context_count = context.as_ref().map_or(0, |tail| tail.count);
    if context_count > 0 {
        sections.push(QueueSection {
            start: u32::try_from(ids.len()).unwrap_or(u32::MAX),
            len: u32::try_from(context_count).unwrap_or(u32::MAX),
            kind: QueueSectionKind::UpNext {
                source_label: origin_label
                    .map_or_else(|| strings::text(strings::SIDEBAR_MUSIC), str::to_owned),
            },
        });
    }

    QueueViewModel {
        ids,
        sections,
        context,
    }
}

#[cfg(test)]
mod que_7_tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn que_7_sidebar_counts_only_the_manual_queue() {
        let context = VirtualContextTail::new(1_638, Rc::new(|_, _| Vec::new()));
        let model = compose_virtual(Some(1), &[10, 11], Some(context), Some("Music"));

        assert_eq!(model.sidebar_count(), 2);
    }

    #[test]
    fn que_7_context_tail_is_not_materialised() {
        let requested = Rc::new(Cell::new(0));
        let requested_for_window = requested.clone();
        let context = VirtualContextTail::new(
            1_638,
            Rc::new(move |offset, limit| {
                requested_for_window.set(limit);
                (offset..offset + limit)
                    .map(|position| i64::try_from(position).unwrap())
                    .collect()
            }),
        );
        let model = compose_virtual(Some(1), &[10, 11], Some(context), Some("Music"));

        assert_eq!(model.ids, [1, 10, 11]);
        assert_eq!(model.total_len(), 1_641);
        assert_eq!(requested.get(), 0);

        let window = model.ids_window(203, 20);
        assert_eq!(window.len(), 20);
        assert_eq!(requested.get(), 20);
    }
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
mod tests {
    use super::*;

    #[test]
    fn compose_builds_three_sections_in_display_order() {
        let model = compose(Some(1), &[2, 3], &[4, 5, 6], Some("Late Night"));
        assert_eq!(model.ids, vec![1, 2, 3]);
        assert_eq!(model.all_ids(), vec![1, 2, 3, 4, 5, 6]);
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
    fn upcoming_reuses_the_composition_without_the_now_playing_prefix() {
        let model = compose(Some(1), &[2, 3], &[4, 5], Some("Late Night")).upcoming();

        assert_eq!(model.ids, vec![2, 3]);
        assert_eq!(model.all_ids(), vec![2, 3, 4, 5]);
        assert_eq!(section_ranges(&model.sections), vec![(0, 2), (2, 4)]);
        assert_eq!(model.sections[0].kind, QueueSectionKind::PlayNext);
        assert_eq!(
            model.sections[1].kind,
            QueueSectionKind::UpNext {
                source_label: "Late Night".into()
            }
        );
    }

    #[test]
    fn compose_omits_empty_play_next_per_que1() {
        let model = compose(Some(9), &[], &[10], Some("Neverbloom"));
        assert_eq!(model.ids, vec![9]);
        assert_eq!(model.all_ids(), vec![9, 10]);
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
        assert_eq!(
            header_title(&model.sections, 1),
            "Playing from Neverbloom · 1 track"
        );
        assert_eq!(header_title(&model.sections, 99), String::new());
    }
}
