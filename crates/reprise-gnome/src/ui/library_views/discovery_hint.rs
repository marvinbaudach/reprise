use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk4::prelude::*;
use rusqlite::Connection;

use crate::ui::strings;

const EVIDENCE_THRESHOLD: usize = 3;
const COVER_HINT_KEY: &str = "hint.cover_download.shown";
const PORTRAIT_HINT_KEY: &str = "hint.artist_portraits.shown";
const NEW_RELEASES_HINT_KEY: &str = "hint.new_releases.shown";

const COVER_TARGETS: &[&str] = &["cover_download"];
const PORTRAIT_TARGETS: &[&str] = &["artist_portraits"];
const NEW_RELEASES_TARGETS: &[&str] = &["new_releases"];
const ARTIST_TARGETS: &[&str] = &["artist_portraits", "new_releases"];

pub(in crate::ui) type OpenPlugins = Rc<RefCell<Option<Rc<dyn Fn(&'static [&'static str])>>>>;
type VoidCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct HintPresentation {
    pub message: &'static str,
    pub targets: &'static [&'static str],
}

#[derive(Debug)]
pub(in crate::ui) struct HintLatch {
    visible_items: usize,
    latched: bool,
    dismissed: bool,
}

impl HintLatch {
    pub(in crate::ui) fn new(suppressed: bool) -> Self {
        Self {
            visible_items: 0,
            latched: false,
            dismissed: suppressed,
        }
    }

    pub(in crate::ui) fn visible_item_added(&mut self) -> bool {
        self.visible_items = self.visible_items.saturating_add(1);
        let was_latched = self.latched;
        if !self.dismissed && self.visible_items >= EVIDENCE_THRESHOLD {
            self.latched = true;
        }
        !was_latched && self.latched
    }

    pub(in crate::ui) fn visible_item_removed(&mut self) {
        self.visible_items = self.visible_items.saturating_sub(1);
    }

    #[cfg(test)]
    pub(in crate::ui) fn should_show(&self) -> bool {
        self.latched && !self.dismissed
    }

    pub(in crate::ui) fn dismiss(&mut self) {
        self.dismissed = true;
    }
}

struct EvidenceInner {
    latch: RefCell<HintLatch>,
    on_latched: RefCell<Option<Rc<dyn Fn()>>>,
}

#[derive(Clone)]
pub(in crate::ui) struct EvidenceTracker {
    inner: Rc<EvidenceInner>,
}

impl EvidenceTracker {
    pub(in crate::ui) fn new(suppressed: bool) -> Self {
        Self {
            inner: Rc::new(EvidenceInner {
                latch: RefCell::new(HintLatch::new(suppressed)),
                on_latched: RefCell::new(None),
            }),
        }
    }

    pub(in crate::ui) fn item(&self) -> EvidenceItem {
        EvidenceItem {
            inner: Rc::new(EvidenceItemInner {
                visible: Cell::new(false),
                tracker: self.clone(),
            }),
        }
    }

    pub(in crate::ui) fn visible_item(&self) -> VisibleEvidence {
        VisibleEvidence {
            inner: Rc::new(VisibleEvidenceInner {
                item: self.item(),
                mapped: Cell::new(false),
                fallback: Cell::new(false),
            }),
        }
    }

    fn set_on_latched(&self, callback: impl Fn() + 'static) {
        *self.inner.on_latched.borrow_mut() = Some(Rc::new(callback));
    }

    fn downgrade(&self) -> Weak<EvidenceInner> {
        Rc::downgrade(&self.inner)
    }
}

struct EvidenceItemInner {
    visible: Cell<bool>,
    tracker: EvidenceTracker,
}

#[derive(Clone)]
pub(in crate::ui) struct EvidenceItem {
    inner: Rc<EvidenceItemInner>,
}

impl EvidenceItem {
    pub(in crate::ui) fn set_visible(&self, visible: bool) {
        if self.inner.visible.replace(visible) == visible {
            return;
        }
        if visible {
            let became_latched = self
                .inner
                .tracker
                .inner
                .latch
                .borrow_mut()
                .visible_item_added();
            if became_latched {
                let callback = self.inner.tracker.inner.on_latched.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            }
        } else {
            self.inner
                .tracker
                .inner
                .latch
                .borrow_mut()
                .visible_item_removed();
        }
    }
}

struct VisibleEvidenceInner {
    item: EvidenceItem,
    mapped: Cell<bool>,
    fallback: Cell<bool>,
}

#[derive(Clone)]
pub(in crate::ui) struct VisibleEvidence {
    inner: Rc<VisibleEvidenceInner>,
}

impl VisibleEvidence {
    pub(in crate::ui) fn set_mapped(&self, mapped: bool) {
        self.inner.mapped.set(mapped);
        self.refresh();
    }

    pub(in crate::ui) fn set_fallback(&self, fallback: bool) {
        self.inner.fallback.set(fallback);
        self.refresh();
    }

    fn refresh(&self) {
        self.inner
            .item
            .set_visible(self.inner.mapped.get() && self.inner.fallback.get());
    }
}

struct HintRow {
    root: gtk4::Box,
    action_label: gtk4::Label,
    targets: Rc<RefCell<&'static [&'static str]>>,
    on_dismiss: VoidCallback,
}

impl HintRow {
    fn new(open_plugins: &OpenPlugins) -> Rc<Self> {
        let action_label = gtk4::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        let action = gtk4::Button::builder()
            .child(&action_label)
            .css_classes(["flat"])
            .hexpand(true)
            .build();
        let dismiss = gtk4::Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text(strings::text(strings::DISMISS))
            .css_classes(["flat"])
            .build();
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        root.set_margin_start(6);
        root.set_margin_end(6);
        root.set_margin_top(3);
        root.set_margin_bottom(3);
        root.append(&action);
        root.append(&dismiss);
        root.set_visible(false);

        let targets = Rc::new(RefCell::new(&[][..]));
        let open = open_plugins.clone();
        let action_targets = targets.clone();
        action.connect_clicked(move |_| {
            let callback = open.borrow().clone();
            if let Some(callback) = callback {
                callback(*action_targets.borrow());
            }
        });
        let on_dismiss: VoidCallback = Rc::new(RefCell::new(None));
        let dismiss_callback = on_dismiss.clone();
        dismiss.connect_clicked(move |_| {
            let callback = dismiss_callback.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        });

        Rc::new(Self {
            root,
            action_label,
            targets,
            on_dismiss,
        })
    }

    fn set_presentation(&self, presentation: Option<HintPresentation>) {
        let Some(presentation) = presentation else {
            self.root.set_visible(false);
            return;
        };
        self.action_label
            .set_text(&strings::text(presentation.message));
        *self.targets.borrow_mut() = presentation.targets;
        self.root.set_visible(true);
    }

    fn set_on_dismiss(&self, callback: impl Fn() + 'static) {
        *self.on_dismiss.borrow_mut() = Some(Rc::new(callback));
    }
}

pub(in crate::ui) struct AlbumDiscovery {
    row: Rc<HintRow>,
    evidence: EvidenceTracker,
    open_plugins: OpenPlugins,
}

impl AlbumDiscovery {
    pub(in crate::ui) fn new(conn: &Rc<RefCell<Connection>>, module_enabled: bool) -> Self {
        let shown = hint_was_shown(conn, COVER_HINT_KEY);
        let evidence = EvidenceTracker::new(module_enabled || shown);
        let open_plugins: OpenPlugins = Rc::new(RefCell::new(None));
        let row = HintRow::new(&open_plugins);

        let row_weak = Rc::downgrade(&row);
        let conn_for_latch = conn.clone();
        evidence.set_on_latched(move || {
            mark_hint_shown(&conn_for_latch, COVER_HINT_KEY);
            if let Some(row) = row_weak.upgrade() {
                row.set_presentation(Some(HintPresentation {
                    message: strings::ENABLE_ALBUM_COVERS,
                    targets: COVER_TARGETS,
                }));
            }
        });
        let evidence_weak = evidence.downgrade();
        let row_weak = Rc::downgrade(&row);
        let conn_for_dismiss = conn.clone();
        row.set_on_dismiss(move || {
            if let Some(inner) = evidence_weak.upgrade() {
                inner.latch.borrow_mut().dismiss();
            }
            mark_hint_shown(&conn_for_dismiss, COVER_HINT_KEY);
            if let Some(row) = row_weak.upgrade() {
                row.set_presentation(None);
            }
        });

        Self {
            row,
            evidence,
            open_plugins,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.row.root
    }

    pub(in crate::ui) fn evidence(&self) -> EvidenceTracker {
        self.evidence.clone()
    }

    pub(in crate::ui) fn set_on_open_plugins(
        &self,
        callback: impl Fn(&'static [&'static str]) + 'static,
    ) {
        *self.open_plugins.borrow_mut() = Some(Rc::new(callback));
    }
}

#[derive(Default)]
struct ArtistFeatures {
    portraits: bool,
    new_releases: bool,
}

pub(in crate::ui) struct ArtistDiscovery {
    row: Rc<HintRow>,
    portrait_evidence: EvidenceTracker,
    open_plugins: OpenPlugins,
}

impl ArtistDiscovery {
    pub(in crate::ui) fn new(
        conn: &Rc<RefCell<Connection>>,
        portraits_enabled: bool,
        new_releases_enabled: bool,
    ) -> Self {
        let portrait_shown = hint_was_shown(conn, PORTRAIT_HINT_KEY);
        let new_releases_shown = hint_was_shown(conn, NEW_RELEASES_HINT_KEY);
        let portrait_evidence = EvidenceTracker::new(portraits_enabled || portrait_shown);
        let open_plugins: OpenPlugins = Rc::new(RefCell::new(None));
        let row = HintRow::new(&open_plugins);
        let features = Rc::new(RefCell::new(ArtistFeatures {
            portraits: false,
            new_releases: !new_releases_enabled && !new_releases_shown,
        }));

        if features.borrow().new_releases {
            row.set_presentation(artist_hint_presentations(false, true).into_iter().next());
        }

        let features_for_map = features.clone();
        let conn_for_map = conn.clone();
        row.root.connect_map(move |_| {
            let features = features_for_map.borrow();
            if features.portraits {
                mark_hint_shown(&conn_for_map, PORTRAIT_HINT_KEY);
            }
            if features.new_releases {
                mark_hint_shown(&conn_for_map, NEW_RELEASES_HINT_KEY);
            }
        });

        let row_weak = Rc::downgrade(&row);
        let features_for_latch = features.clone();
        let conn_for_latch = conn.clone();
        portrait_evidence.set_on_latched(move || {
            features_for_latch.borrow_mut().portraits = true;
            mark_hint_shown(&conn_for_latch, PORTRAIT_HINT_KEY);
            if let Some(row) = row_weak.upgrade() {
                let features = features_for_latch.borrow();
                row.set_presentation(
                    artist_hint_presentations(features.portraits, features.new_releases)
                        .into_iter()
                        .next(),
                );
            }
        });

        let evidence_weak = portrait_evidence.downgrade();
        let row_weak = Rc::downgrade(&row);
        let features_for_dismiss = features;
        let conn_for_dismiss = conn.clone();
        row.set_on_dismiss(move || {
            let mut features = features_for_dismiss.borrow_mut();
            if features.portraits {
                mark_hint_shown(&conn_for_dismiss, PORTRAIT_HINT_KEY);
            }
            if features.new_releases {
                mark_hint_shown(&conn_for_dismiss, NEW_RELEASES_HINT_KEY);
            }
            features.portraits = false;
            features.new_releases = false;
            if let Some(inner) = evidence_weak.upgrade() {
                inner.latch.borrow_mut().dismiss();
            }
            if let Some(row) = row_weak.upgrade() {
                row.set_presentation(None);
            }
        });

        Self {
            row,
            portrait_evidence,
            open_plugins,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.row.root
    }

    pub(in crate::ui) fn portrait_evidence(&self) -> EvidenceTracker {
        self.portrait_evidence.clone()
    }

    pub(in crate::ui) fn set_on_open_plugins(
        &self,
        callback: impl Fn(&'static [&'static str]) + 'static,
    ) {
        *self.open_plugins.borrow_mut() = Some(Rc::new(callback));
    }
}

pub(in crate::ui) fn artist_hint_presentations(
    portraits: bool,
    new_releases: bool,
) -> Vec<HintPresentation> {
    let presentation = match (portraits, new_releases) {
        (true, true) => Some(HintPresentation {
            message: strings::ENABLE_ARTIST_NETWORK_FEATURES,
            targets: ARTIST_TARGETS,
        }),
        (true, false) => Some(HintPresentation {
            message: strings::ENABLE_ARTIST_PORTRAITS,
            targets: PORTRAIT_TARGETS,
        }),
        (false, true) => Some(HintPresentation {
            message: strings::ENABLE_NEW_RELEASES,
            targets: NEW_RELEASES_TARGETS,
        }),
        (false, false) => None,
    };
    presentation.into_iter().collect()
}

fn hint_was_shown(conn: &Rc<RefCell<Connection>>, key: &str) -> bool {
    reprise_core::library::settings::get_bool(&conn.borrow(), key, false).unwrap_or_else(|error| {
        tracing::warn!(%error, key, "could not read discovery-hint state; suppressing the hint");
        true
    })
}

fn mark_hint_shown(conn: &Rc<RefCell<Connection>>, key: &str) {
    if let Err(error) = reprise_core::library::settings::set_bool(&conn.borrow(), key, true) {
        tracing::warn!(%error, key, "could not persist discovery-hint state");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_1_hint_needs_visible_evidence() {
        let mut hint = HintLatch::new(false);
        hint.visible_item_added();
        hint.visible_item_added();
        assert!(!hint.should_show());

        hint.visible_item_added();
        assert!(hint.should_show());
    }

    #[test]
    fn discover_1_hint_latches_and_never_returns() {
        let mut hint = HintLatch::new(false);
        for _ in 0..3 {
            hint.visible_item_added();
        }
        for _ in 0..3 {
            hint.visible_item_removed();
        }
        assert!(hint.should_show(), "scrolling must not clear the latch");

        hint.dismiss();
        assert!(!hint.should_show());
        assert!(!HintLatch::new(true).should_show());
    }

    #[test]
    fn discover_2_combined_line_when_both_apply() {
        let lines = artist_hint_presentations(true, true);

        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0],
            HintPresentation {
                message: "Enable network features for artists (images & new releases) →",
                targets: &["artist_portraits", "new_releases"],
            }
        );
    }
}
