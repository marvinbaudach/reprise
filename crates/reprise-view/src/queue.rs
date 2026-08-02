//! QUE-1: the Queue view's toolkit-free composite model — Now Playing, Play Next, and
//! a named virtual playback-context tail — composed from the controller
//! into one flat item list plus section ranges. The GTK adapter in
//! `queue_sections` renders the section titles and re-exports this surface.
//!
//! QUE-2 by construction: the display order is exactly the play order —
//! `next_target` pops Play Next FIFO first, then walks the snapshot from
//! the current position, which is precisely `play_next ++ up_next_rest`.

use crate::strings::{Message, Plural};
use reprise_core::up_next::QueueItem;

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

const QUEUE_SECTION_NOW_PLAYING: &str = N_!("Now Playing");
const QUEUE_SECTION_PLAY_NEXT: &str = N_!("Play Next");

const fn plural(singular: &'static str, plural: &'static str) -> (&'static str, &'static str) {
    (singular, plural)
}

const QUEUE_CONTEXT_TAIL: (&str, &str) = plural(
    "Playing from {source} · {count} track",
    "Playing from {source} · {count} tracks",
);

/// P1a's binding rule: no view model may hold a closure, because a planned
/// Android surface reaches this type through UniFFI, and UniFFI cannot carry
/// a closure across an FFI boundary. `Rc<dyn Fn(usize, usize) -> Vec<QueueItem>>`
/// is rejected there with `TypeId`, `Lower` and `Lift` trait-bound errors —
/// measured against UniFFI 0.29, not assumed.
///
/// `Rc<dyn Fn>` is neither `Send` nor `Sync` while every other field here is,
/// so this assertion fails to compile the moment a closure comes back. It is
/// a permanent guard, not a one-off migration check.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<QueueViewModel>();
};

/// What a queue section IS — drives its header title (and, later, the
/// header's actions: QUE-3 puts "Clear" on the Play Next header).
#[derive(Clone, Debug, PartialEq)]
pub enum QueueSectionKind {
    NowPlaying,
    PlayNext,
    UpNext { source_label: String },
}

/// One contiguous section of the composite Queue view.
#[derive(Clone, Debug, PartialEq)]
pub struct QueueSection {
    pub start: u32,
    pub len: u32,
    pub kind: QueueSectionKind,
}

/// The composite Queue view: the flat id list the windowed query renders,
/// plus the section ranges the header factory titles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueueViewModel {
    /// Materialized prefix only: optional Now Playing followed by manual
    /// entries. `context` describes the tail; callers supply its rows.
    pub items: Vec<QueueItem>,
    pub sections: Vec<QueueSection>,
    context: Option<VirtualContext>,
}

/// How long the virtual context tail is, and which context it belongs to.
/// Deliberately data only: the rows themselves are fetched through
/// [`ContextWindow`], never through a closure the model carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualContext {
    count: usize,
    identity: Option<VirtualContextIdentity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtualContextIdentity {
    sequence: (u64, u64),
    start: usize,
}

impl VirtualContext {
    pub fn new(count: usize) -> Self {
        Self {
            count,
            identity: None,
        }
    }

    pub fn identified(count: usize, sequence: (u64, u64), start: usize) -> Self {
        Self {
            count,
            identity: Some(VirtualContextIdentity { sequence, start }),
        }
    }
}

/// Supplies the context rows a [`QueueViewModel`] describes but does not
/// hold. The GTK side implements this over the windowed query; a future
/// Android side implements it over the same query behind UniFFI.
pub trait ContextWindow {
    fn rows(&self, offset: usize, limit: usize) -> Vec<QueueItem>;
}

impl ContextWindow for Vec<i64> {
    fn rows(&self, offset: usize, limit: usize) -> Vec<QueueItem> {
        let end = offset.saturating_add(limit).min(self.len());
        self.get(offset..end)
            .unwrap_or_default()
            .iter()
            .copied()
            .map(QueueItem::Track)
            .collect()
    }
}

impl ContextWindow for Vec<QueueItem> {
    fn rows(&self, offset: usize, limit: usize) -> Vec<QueueItem> {
        let end = offset.saturating_add(limit).min(self.len());
        self.get(offset..end).unwrap_or_default().to_vec()
    }
}

impl QueueViewModel {
    /// The shared queue projection used by the compact panel: the exact same
    /// model with the optional Now Playing prefix removed and section offsets
    /// rebased. No queue composition is repeated in the panel.
    pub fn upcoming(&self) -> Self {
        let now_playing_len = self
            .sections
            .first()
            .filter(|section| section.kind == QueueSectionKind::NowPlaying)
            .map_or(0, |section| section.len);
        let skip = usize::try_from(now_playing_len).unwrap_or(self.items.len());
        let items = self.items.get(skip..).unwrap_or_default().to_vec();
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
            items,
            sections,
            context: self.context.clone(),
        }
    }

    pub fn sidebar_count(&self) -> usize {
        self.sections
            .iter()
            .find(|section| section.kind == QueueSectionKind::PlayNext)
            .map_or(0, |section| section.len as usize)
    }

    pub fn total_len(&self) -> usize {
        self.items.len() + self.context.as_ref().map_or(0, |tail| tail.count)
    }

    pub fn items_window(
        &self,
        offset: usize,
        limit: usize,
        tail: &dyn ContextWindow,
    ) -> Vec<QueueItem> {
        if limit == 0 || offset >= self.total_len() {
            return Vec::new();
        }
        let mut items = Vec::with_capacity(limit.min(self.total_len() - offset));
        if offset < self.items.len() {
            let end = offset.saturating_add(limit).min(self.items.len());
            items.extend_from_slice(&self.items[offset..end]);
        }
        let remaining = limit.saturating_sub(items.len());
        if remaining == 0 {
            return items;
        }
        let Some(context) = &self.context else {
            return items;
        };
        let context_offset = offset.saturating_sub(self.items.len());
        let context_limit = remaining.min(context.count.saturating_sub(context_offset));
        items.extend(tail.rows(context_offset, context_limit));
        items
    }

    pub fn all_items(&self, tail: &dyn ContextWindow) -> Vec<QueueItem> {
        self.items_window(0, self.total_len(), tail)
    }

    /// Exact O(1) model delta for the two normal forward-playback shapes:
    /// consuming the first materialized Play Next row, or advancing through
    /// an unchanged virtual context. The tail's frozen sequence/start
    /// identity is the proof for the virtual case.
    pub fn leading_removal_change_from(&self, old: &Self) -> Option<(u32, u32, u32)> {
        let material_removed = old.items.len().checked_sub(self.items.len())?;
        let context_unchanged = match (&old.context, &self.context) {
            (None, None) => true,
            (Some(old), Some(new)) => {
                old.identity.is_some() && old.identity == new.identity && old.count == new.count
            }
            _ => false,
        };
        if material_removed > 0
            && context_unchanged
            && old.items.get(material_removed..) == Some(self.items.as_slice())
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
            u32::try_from(self.items.len()).unwrap_or(u32::MAX),
            u32::try_from(removed).unwrap_or(u32::MAX),
            0,
        ))
    }
}

/// Composes the three queue parts into display order (QUE-1). Sections are
/// emitted only when non-empty; an entirely empty composition (nothing
/// playing, nothing pending) yields the empty model that routes the view to
/// the QUE-4 StatusPage.
///
/// **A surface should call [`compose_virtual`] instead.** This entry point
/// builds an *unidentified* [`VirtualContext`], so
/// [`QueueViewModel::leading_removal_change_from`] cannot recognise a shifted
/// context and falls back to a full replacement rather than the O(1) diff.
/// It exists because it is the shape the tests want, and it is `pub` rather
/// than `#[cfg(test)]` only because a library's test-gated items are invisible
/// to a dependent crate's tests — the same reason [`VirtualContext::new`] and
/// the `ContextWindow` impls for `Vec` are unconditional here.
pub fn compose(
    now_playing: Option<QueueItem>,
    play_next: &[QueueItem],
    up_next_rest: &[i64],
    origin_label: Option<&str>,
    rendered_fallback: &str,
) -> QueueViewModel {
    let context = (!up_next_rest.is_empty()).then(|| VirtualContext::new(up_next_rest.len()));
    compose_virtual(
        now_playing,
        play_next,
        context,
        origin_label,
        rendered_fallback,
    )
}

pub fn compose_virtual(
    now_playing: Option<QueueItem>,
    play_next: &[QueueItem],
    context: Option<VirtualContext>,
    origin_label: Option<&str>,
    rendered_fallback: &str,
) -> QueueViewModel {
    let mut items = Vec::with_capacity(usize::from(now_playing.is_some()) + play_next.len());
    let mut sections = Vec::new();

    if let Some(current) = now_playing {
        sections.push(QueueSection {
            start: 0,
            len: 1,
            kind: QueueSectionKind::NowPlaying,
        });
        items.push(current);
    }
    if !play_next.is_empty() {
        sections.push(QueueSection {
            start: u32::try_from(items.len()).unwrap_or(u32::MAX),
            len: u32::try_from(play_next.len()).unwrap_or(u32::MAX),
            kind: QueueSectionKind::PlayNext,
        });
        items.extend_from_slice(play_next);
    }
    let context_count = context.as_ref().map_or(0, |tail| tail.count);
    if context_count > 0 {
        sections.push(QueueSection {
            start: u32::try_from(items.len()).unwrap_or(u32::MAX),
            len: u32::try_from(context_count).unwrap_or(u32::MAX),
            kind: QueueSectionKind::UpNext {
                source_label: origin_label.unwrap_or(rendered_fallback).to_owned(),
            },
        });
    }

    QueueViewModel {
        items,
        sections,
        context,
    }
}

#[cfg(test)]
struct SliceContextWindow<'a>(&'a [i64]);

#[cfg(test)]
impl ContextWindow for SliceContextWindow<'_> {
    fn rows(&self, offset: usize, limit: usize) -> Vec<QueueItem> {
        let end = offset.saturating_add(limit).min(self.0.len());
        self.0
            .get(offset..end)
            .unwrap_or_default()
            .iter()
            .copied()
            .map(QueueItem::Track)
            .collect()
    }
}

#[cfg(test)]
mod que_7_tests {
    use super::*;

    fn tracks(ids: &[i64]) -> Vec<QueueItem> {
        ids.iter().copied().map(QueueItem::Track).collect()
    }

    #[test]
    fn que_7_sidebar_counts_only_the_manual_queue() {
        let context = VirtualContext::new(1_638);
        let model = compose_virtual(
            Some(QueueItem::Track(1)),
            &tracks(&[10, 11]),
            Some(context),
            Some("Music"),
            "Music",
        );

        assert_eq!(model.sidebar_count(), 2);
    }
}

#[cfg(test)]
mod que_10_tests {
    use super::*;

    #[test]
    fn typed_context_windows_across_the_manual_boundary() {
        let context_items = vec![QueueItem::Episode(20), QueueItem::Episode(21)];
        let model = compose_virtual(
            Some(QueueItem::Episode(19)),
            &[QueueItem::Track(10)],
            Some(VirtualContext::identified(2, (42, 7), 0)),
            Some("VOID PREACHER"),
            "Music",
        );

        assert_eq!(model.total_len(), 4);
        assert_eq!(
            model.items_window(1, 3, &context_items),
            vec![
                QueueItem::Track(10),
                QueueItem::Episode(20),
                QueueItem::Episode(21),
            ]
        );
        assert_eq!(model.sidebar_count(), 1);
    }
}

/// The `(start, end)` ranges `gtk::SectionModel::section(position)` answers
/// from — half-open, in model coordinates.
pub fn section_ranges(sections: &[QueueSection]) -> Vec<(u32, u32)> {
    sections
        .iter()
        .map(|section| (section.start, section.start.saturating_add(section.len)))
        .collect()
}

/// Selects the translatable title for the section starting at `start`.
/// Rendering belongs to the consuming surface.
pub fn header_title(sections: &[QueueSection], start: u32) -> Option<Message> {
    let section = sections.iter().find(|section| section.start == start)?;
    match section {
        QueueSection {
            kind: QueueSectionKind::NowPlaying,
            ..
        } => Some(Message {
            id: QUEUE_SECTION_NOW_PLAYING,
            plural: None,
            args: vec![],
        }),
        QueueSection {
            kind: QueueSectionKind::PlayNext,
            ..
        } => Some(Message {
            id: QUEUE_SECTION_PLAY_NEXT,
            plural: None,
            args: vec![],
        }),
        QueueSection {
            len,
            kind: QueueSectionKind::UpNext { source_label },
            ..
        } => {
            let count = u64::from(*len);
            let count_text = reprise_core::format::format_thousands(i64::from(*len));
            Some(Message {
                id: QUEUE_CONTEXT_TAIL.0,
                plural: Some(Plural {
                    id: QUEUE_CONTEXT_TAIL.1,
                    count,
                }),
                args: vec![("source", source_label.clone()), ("count", count_text)],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strings::{Message, Plural};

    fn tracks(ids: &[i64]) -> Vec<QueueItem> {
        ids.iter().copied().map(QueueItem::Track).collect()
    }

    fn all_items(model: &QueueViewModel, context: &[i64]) -> Vec<QueueItem> {
        model.all_items(&SliceContextWindow(context))
    }

    #[test]
    fn header_titles_select_the_shared_catalog_messages() {
        let model = compose(
            Some(QueueItem::Track(1)),
            &tracks(&[2, 3]),
            &[4, 5],
            Some("Neverbloom"),
            "Music",
        );

        assert_eq!(
            header_title(&model.sections, 0),
            Some(Message {
                id: "Now Playing",
                plural: None,
                args: vec![],
            })
        );
        assert_eq!(
            header_title(&model.sections, 1),
            Some(Message {
                id: "Play Next",
                plural: None,
                args: vec![],
            })
        );
        assert_eq!(
            header_title(&model.sections, 3),
            Some(Message {
                id: "Playing from {source} · {count} track",
                plural: Some(Plural {
                    id: "Playing from {source} · {count} tracks",
                    count: 2,
                }),
                args: vec![
                    ("source", "Neverbloom".to_owned()),
                    ("count", "2".to_owned()),
                ],
            })
        );
        assert_eq!(header_title(&model.sections, 99), None);
    }

    #[test]
    fn composition_uses_the_surface_rendered_context_fallback() {
        let model = compose_virtual(None, &[], Some(VirtualContext::new(1)), None, "Bibliothek");

        assert_eq!(
            model.sections[0].kind,
            QueueSectionKind::UpNext {
                source_label: "Bibliothek".to_owned(),
            }
        );
    }

    #[test]
    fn mixed_manual_queue_preserves_item_kind_when_numeric_ids_collide() {
        let model = compose(
            Some(reprise_core::up_next::QueueItem::Track(1)),
            &[
                reprise_core::up_next::QueueItem::Track(7),
                reprise_core::up_next::QueueItem::Episode(7),
            ],
            &[],
            Some("Late Night"),
            "Music",
        );

        assert_eq!(
            all_items(&model, &[]),
            vec![
                reprise_core::up_next::QueueItem::Track(1),
                reprise_core::up_next::QueueItem::Track(7),
                reprise_core::up_next::QueueItem::Episode(7),
            ]
        );
    }

    #[test]
    fn compose_builds_three_sections_in_display_order() {
        let model = compose(
            Some(QueueItem::Track(1)),
            &tracks(&[2, 3]),
            &[4, 5, 6],
            Some("Late Night"),
            "Music",
        );
        assert_eq!(
            model.items,
            [1, 2, 3]
                .into_iter()
                .map(QueueItem::Track)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            all_items(&model, &[4, 5, 6]),
            [1, 2, 3, 4, 5, 6]
                .into_iter()
                .map(QueueItem::Track)
                .collect::<Vec<_>>()
        );
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
        let model = compose(
            Some(QueueItem::Track(1)),
            &tracks(&[2, 3]),
            &[4, 5],
            Some("Late Night"),
            "Music",
        )
        .upcoming();

        assert_eq!(
            model.items,
            [2, 3].into_iter().map(QueueItem::Track).collect::<Vec<_>>()
        );
        assert_eq!(
            all_items(&model, &[4, 5]),
            [2, 3, 4, 5]
                .into_iter()
                .map(QueueItem::Track)
                .collect::<Vec<_>>()
        );
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
        let model = compose(
            Some(QueueItem::Track(9)),
            &[],
            &[10],
            Some("Neverbloom"),
            "Music",
        );
        assert_eq!(model.items, vec![QueueItem::Track(9)]);
        assert_eq!(
            all_items(&model, &[10]),
            vec![QueueItem::Track(9), QueueItem::Track(10)]
        );
        assert_eq!(model.sections.len(), 2);
        assert_eq!(model.sections[1].start, 1);
    }

    #[test]
    fn compose_without_playback_still_lists_pending_play_next() {
        // Stopped but manually queued tracks exist: no Now Playing row, the
        // pending section still shows (QUE-4's StatusPage is only for the
        // fully empty case).
        let model = compose(None, &tracks(&[7]), &[], None, "Music");
        assert_eq!(model.items, vec![QueueItem::Track(7)]);
        assert_eq!(model.sections.len(), 1);
        assert_eq!(model.sections[0].kind, QueueSectionKind::PlayNext);
    }

    #[test]
    fn compose_fully_empty_yields_the_empty_model() {
        let model = compose(None, &[], &[], None, "Music");
        assert!(model.items.is_empty());
        assert!(model.sections.is_empty());
    }
}
