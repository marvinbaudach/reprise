use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library_doctor::{DoctorReviewRowId, DoctorReviewRowState};

use super::review_model::{row_state_reason, ReviewRowModel};
use super::review_snapshot::{AlbumCounts, ReviewSnapshot};
use crate::ui::strings;

pub(super) type OnSelect = Rc<dyn Fn(&[DoctorReviewRowId], bool)>;

struct HeaderWidgets {
    root: gtk4::Box,
    checkbox: gtk4::CheckButton,
    title: gtk4::Label,
    detail: gtk4::Label,
    pill: gtk4::Label,
    album_key: RefCell<String>,
    album_title: RefCell<String>,
    album_artist: RefCell<String>,
    row_ids: RefCell<Vec<DoctorReviewRowId>>,
    binding: Cell<bool>,
}

#[derive(Clone, Default)]
pub(super) struct AlbumHeaderRegistry {
    widgets: Rc<RefCell<HashMap<usize, Rc<HeaderWidgets>>>>,
    #[cfg(test)]
    pushes: Rc<Cell<u32>>,
}

impl AlbumHeaderRegistry {
    pub(super) fn push_selection(&self, albums: &HashMap<String, AlbumCounts>) {
        let updates = self
            .widgets
            .borrow()
            .values()
            .filter_map(|widgets| {
                let album_key = widgets.album_key.borrow().clone();
                let counts = albums.get(&album_key)?.clone();
                Some((widgets.clone(), counts))
            })
            .collect::<Vec<_>>();
        for (widgets, counts) in updates {
            apply_check_state(&widgets, &counts);
            #[cfg(test)]
            self.pushes.set(self.pushes.get() + 1);
        }
    }

    #[cfg(test)]
    pub(super) fn push_count(&self) -> u32 {
        self.pushes.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MasterCheckState {
    pub(super) active: bool,
    pub(super) inconsistent: bool,
    pub(super) sensitive: bool,
}

pub(super) fn master_check_state(selected: usize, selectable: usize) -> MasterCheckState {
    MasterCheckState {
        active: selectable > 0 && selected == selectable,
        inconsistent: selected > 0 && selected < selectable,
        sensitive: selectable > 0,
    }
}

pub(super) struct AlbumHeaderState {
    pub(super) check: MasterCheckState,
    pub(super) pill: String,
    /// `Some` exactly when the checkbox is insensitive: why it is.
    pub(super) reason: Option<String>,
}

pub(super) fn album_header_state(
    selected: usize,
    selectable: usize,
    changes: usize,
    blocked_by: Option<DoctorReviewRowState>,
) -> AlbumHeaderState {
    let check = master_check_state(selected, selectable);
    if selectable > 0 {
        return AlbumHeaderState {
            check,
            pill: album_change_count(selectable, selected),
            reason: None,
        };
    }
    let blocked_by = blocked_by.unwrap_or(DoctorReviewRowState::Ready);
    let (pill, reason) = match blocked_by {
        DoctorReviewRowState::Conflict => (
            strings::doctor_change_count_unresolved(changes),
            row_state_reason(blocked_by).map(strings::text),
        ),
        DoctorReviewRowState::Ready | DoctorReviewRowState::Stale => {
            (strings::doctor_change_count_none_selected(changes), None)
        }
    };
    AlbumHeaderState {
        check,
        pill,
        reason,
    }
}

#[derive(Clone)]
pub(super) struct ReviewColumnGroups {
    pub(super) selection: gtk4::SizeGroup,
    pub(super) track: gtk4::SizeGroup,
    pub(super) field: gtk4::SizeGroup,
    pub(super) current: gtk4::SizeGroup,
    pub(super) arrow: gtk4::SizeGroup,
    pub(super) proposed: gtk4::SizeGroup,
    pub(super) source: gtk4::SizeGroup,
    pub(super) edit: gtk4::SizeGroup,
}

impl ReviewColumnGroups {
    fn new() -> Self {
        let group = || gtk4::SizeGroup::new(gtk4::SizeGroupMode::Horizontal);
        Self {
            selection: group(),
            track: group(),
            field: group(),
            current: group(),
            arrow: group(),
            proposed: group(),
            source: group(),
            edit: group(),
        }
    }

    pub(super) fn set_wide(&self, wide: bool) {
        let mode = if wide {
            gtk4::SizeGroupMode::Horizontal
        } else {
            gtk4::SizeGroupMode::None
        };
        for group in [
            &self.selection,
            &self.track,
            &self.field,
            &self.current,
            &self.arrow,
            &self.proposed,
            &self.source,
            &self.edit,
        ] {
            group.set_mode(mode);
        }
    }
}

pub(super) struct ReviewHeader {
    pub(super) root: gtk4::Box,
    pub(super) labels: gtk4::Box,
    pub(super) select_all: gtk4::CheckButton,
    pub(super) select_all_label: gtk4::Label,
    pub(super) groups: ReviewColumnGroups,
}

impl ReviewHeader {
    pub(super) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        root.set_margin_start(28);
        root.set_margin_end(28);
        root.add_css_class("dim-label");
        let groups = ReviewColumnGroups::new();

        let select_all = gtk4::CheckButton::new();
        select_all.set_size_request(16, 16);
        select_all.add_css_class("doctor-album-check");
        select_all.add_css_class("doctor-review-select-all");
        select_all.set_tooltip_text(Some(&strings::text(strings::DOCTOR_SELECT_ALL_VISIBLE)));
        select_all.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::DOCTOR_SELECT_ALL_VISIBLE,
        ))]);
        // a11y-semantics: role=checkbox name=select-all-visible state=selection action=toggle
        select_all.set_focusable(true);
        groups.selection.add_widget(&select_all);
        root.append(&select_all);

        let select_all_label = gtk4::Label::builder()
            .label(strings::text(strings::DOCTOR_SELECT_ALL))
            .xalign(0.0)
            .visible(false)
            .css_classes(["caption"])
            .build();
        root.append(&select_all_label);

        let labels = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        labels.set_hexpand(true);
        for (group, text) in [
            (&groups.track, strings::DOCTOR_TRACK),
            (&groups.field, strings::DOCTOR_FIELD),
            (&groups.current, strings::DOCTOR_CURRENT),
            (&groups.arrow, ""),
            (&groups.proposed, strings::DOCTOR_PROPOSED),
            (&groups.source, strings::DOCTOR_SOURCE),
            (&groups.edit, ""),
        ] {
            let label = gtk4::Label::builder()
                .label(if text.is_empty() {
                    String::new()
                } else {
                    strings::text(text)
                })
                .xalign(0.0)
                .hexpand(!text.is_empty())
                .css_classes(["caption"])
                .build();
            group.add_widget(&label);
            labels.append(&label);
        }
        root.append(&labels);

        Self {
            root,
            labels,
            select_all,
            select_all_label,
            groups,
        }
    }
}

pub(super) fn album_header_factory(
    model: &gtk4::SortListModel,
    on_select: &OnSelect,
    snapshot: &Rc<RefCell<ReviewSnapshot>>,
    registry: &AlbumHeaderRegistry,
) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    {
        let model = model.clone();
        let on_select = on_select.clone();
        let snapshot = snapshot.clone();
        let registry = registry.clone();
        factory.connect_setup(move |_, object| {
            let Some(header) = object.downcast_ref::<gtk4::ListHeader>() else {
                return;
            };
            let widgets = Rc::new(build_album_header());
            {
                let widgets = widgets.clone();
                let on_select = on_select.clone();
                widgets.checkbox.clone().connect_toggled(move |button| {
                    if widgets.binding.get() {
                        return;
                    }
                    let row_ids = widgets.row_ids.borrow().clone();
                    on_select(&row_ids, button.is_active());
                });
            }
            registry
                .widgets
                .borrow_mut()
                .insert(header_key(header), widgets);
            for property in ["start", "end"] {
                let model = model.clone();
                let snapshot = snapshot.clone();
                let registry = registry.clone();
                header.connect_notify_local(Some(property), move |header, _| {
                    bind_album_header(header, &model, &snapshot, &registry);
                });
            }
        });
    }
    {
        let model = model.clone();
        let snapshot = snapshot.clone();
        let registry = registry.clone();
        factory.connect_bind(move |_, object| {
            let Some(header) = object.downcast_ref::<gtk4::ListHeader>() else {
                return;
            };
            bind_album_header(header, &model, &snapshot, &registry);
        });
    }
    {
        let registry = registry.clone();
        factory.connect_unbind(move |_, object| {
            let Some(header) = object.downcast_ref::<gtk4::ListHeader>() else {
                return;
            };
            if let Some(widgets) = registry.widgets.borrow().get(&header_key(header)).cloned() {
                widgets.album_key.borrow_mut().clear();
                widgets.row_ids.borrow_mut().clear();
            }
            header.set_child(gtk4::Widget::NONE);
        });
    }
    {
        let registry = registry.clone();
        factory.connect_teardown(move |_, object| {
            let Some(header) = object.downcast_ref::<gtk4::ListHeader>() else {
                return;
            };
            registry.widgets.borrow_mut().remove(&header_key(header));
        });
    }
    factory
}

fn bind_album_header(
    header: &gtk4::ListHeader,
    model: &gtk4::SortListModel,
    snapshot: &Rc<RefCell<ReviewSnapshot>>,
    registry: &AlbumHeaderRegistry,
) {
    // Not an anomaly: GTK rebinds a recycled `ListHeader` before it has assigned
    // the section range, so `start`/`end` are both `INVALID_LIST_POSITION` for a
    // moment during ordinary scrolling. Measured on the real library, this fired
    // 66 times in one session — at `warn!` it buried the two genuinely unexpected
    // cases below, which is the opposite of what a warning is for.
    if header.start() == gtk4::INVALID_LIST_POSITION || header.end() == gtk4::INVALID_LIST_POSITION
    {
        tracing::debug!(
            start = header.start(),
            end = header.end(),
            "DOC-9b kept the last known header while its section range was unassigned"
        );
        return;
    }
    let Some(widgets) = registry.widgets.borrow().get(&header_key(header)).cloned() else {
        return;
    };
    let Some(item) = model.item(header.start()) else {
        tracing::warn!(
            start = header.start(),
            end = header.end(),
            "DOC-9b kept the last known header while its section row was unavailable"
        );
        return;
    };
    let Ok(boxed) = item.downcast::<glib::BoxedAnyObject>() else {
        widgets.album_key.borrow_mut().clear();
        widgets.row_ids.borrow_mut().clear();
        header.set_child(gtk4::Widget::NONE);
        return;
    };
    let first = boxed.borrow::<ReviewRowModel>().clone();
    let counts = snapshot.borrow().albums.get(&first.album_key).cloned();
    let Some(counts) = counts else {
        tracing::warn!(
            album = first.album_key,
            "DOC-9b kept the last known header while its snapshot was unavailable"
        );
        return;
    };
    bind_header_widgets(&widgets, &first, &counts);
    if header.child().as_ref() != Some(widgets.root.upcast_ref()) {
        header.set_child(Some(&widgets.root));
    }
}

fn build_album_header() -> HeaderWidgets {
    let checkbox = gtk4::CheckButton::new();
    checkbox.set_size_request(16, 16);
    checkbox.add_css_class("doctor-album-check");
    // a11y-semantics: role=checkbox name=change-count state=selection action=toggle
    checkbox.set_focusable(true);

    let cover = gtk4::Image::from_icon_name("audio-x-generic-symbolic");
    cover.set_size_request(38, 38);
    cover.add_css_class("doctor-album-cover");
    let title_label = gtk4::Label::builder()
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .css_classes(["heading", "doctor-album-title"])
        .build();
    let detail = gtk4::Label::builder()
        .xalign(0.0)
        .css_classes(["doctor-album-detail"])
        .build();
    let copy = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .spacing(10)
        .baseline_position(gtk4::BaselinePosition::Center)
        .hexpand(true)
        .build();
    copy.append(&title_label);
    copy.append(&detail);
    let pill = gtk4::Label::builder()
        .halign(gtk4::Align::End)
        .css_classes(["tag"])
        .build();
    let caret = gtk4::Image::builder()
        .icon_name("pan-end-symbolic")
        .pixel_size(16)
        .css_classes(["doctor-album-caret"])
        .build();
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
    root.set_margin_bottom(10);
    root.set_margin_start(28);
    root.set_margin_end(28);
    root.append(&checkbox);
    root.append(&cover);
    root.append(&copy);
    root.append(&pill);
    root.append(&caret);
    HeaderWidgets {
        root,
        checkbox,
        title: title_label,
        detail,
        pill,
        album_key: RefCell::new(String::new()),
        album_title: RefCell::new(String::new()),
        album_artist: RefCell::new(String::new()),
        row_ids: RefCell::new(Vec::new()),
        binding: Cell::new(false),
    }
}

fn bind_header_widgets(widgets: &HeaderWidgets, first: &ReviewRowModel, counts: &AlbumCounts) {
    let title = if first.album_title.trim().is_empty() {
        strings::text(strings::DOCTOR_NO_ALBUM)
    } else {
        first.album_title.clone()
    };
    let track_count = strings::drag_tracks_label(first.album_track_count);
    let detail = if first.album_artist.trim().is_empty() {
        track_count
    } else {
        format!("{} · {track_count}", first.album_artist)
    };
    widgets.album_key.replace(first.album_key.clone());
    widgets.album_title.replace(title.clone());
    widgets.album_artist.replace(first.album_artist.clone());
    widgets.row_ids.replace(counts.selectable_row_ids.clone());
    widgets.title.set_label(&title);
    widgets.detail.set_label(&detail);
    widgets.root.remove_css_class("doctor-album-header-first");
    widgets.root.remove_css_class("doctor-album-header-later");
    widgets
        .root
        .set_margin_top(if first.album_position == 0 { 16 } else { 8 });
    widgets.root.add_css_class(if first.album_position == 0 {
        "doctor-album-header-first"
    } else {
        "doctor-album-header-later"
    });
    apply_check_state(widgets, counts);
}

fn apply_check_state(widgets: &HeaderWidgets, counts: &AlbumCounts) {
    let state = album_header_state(
        counts.selected,
        counts.selectable,
        counts.changes,
        counts.blocked_by,
    );
    widgets.binding.set(true);
    widgets.checkbox.set_active(state.check.active);
    widgets.checkbox.set_inconsistent(state.check.inconsistent);
    widgets.checkbox.set_sensitive(state.check.sensitive);
    widgets.binding.set(false);
    widgets
        .checkbox
        .update_property(&[gtk4::accessible::Property::Label(&state.pill)]);
    widgets.pill.set_label(&state.pill);
    widgets.root.set_tooltip_text(state.reason.as_deref());
    let title = widgets.album_title.borrow().clone();
    let artist = widgets.album_artist.borrow().clone();
    let accessible_label = state.reason.as_ref().map_or_else(
        || format!("{title} · {artist} · {}", state.pill),
        |reason| format!("{title} · {artist} · {} · {reason}", state.pill),
    );
    widgets
        .root
        .update_property(&[gtk4::accessible::Property::Label(&accessible_label)]);
}

fn album_change_count(total: usize, selected: usize) -> String {
    if selected == 0 && total > 0 {
        strings::doctor_change_count_none_selected(total)
    } else {
        strings::doctor_change_count(selected)
    }
}

fn header_key(header: &gtk4::ListHeader) -> usize {
    header.as_ptr() as usize
}

#[cfg(test)]
mod tests {
    #[test]
    fn doc_3c_the_master_check_mirrors_the_visible_selection() {
        use super::MasterCheckState;
        assert_eq!(
            super::master_check_state(0, 0),
            MasterCheckState {
                active: false,
                inconsistent: false,
                sensitive: false,
            }
        );
        assert_eq!(
            super::master_check_state(0, 4),
            MasterCheckState {
                active: false,
                inconsistent: false,
                sensitive: true,
            }
        );
        assert_eq!(
            super::master_check_state(2, 4),
            MasterCheckState {
                active: false,
                inconsistent: true,
                sensitive: true,
            }
        );
        assert_eq!(
            super::master_check_state(4, 4),
            MasterCheckState {
                active: true,
                inconsistent: false,
                sensitive: true,
            }
        );
    }

    #[test]
    fn doc_9b_a_fully_deselected_album_says_none_selected() {
        assert_eq!(super::album_change_count(2, 0), "2 changes · none selected");
        assert_eq!(super::album_change_count(2, 1), "1 change");
    }

    #[test]
    fn doc_3c_album_header_state_names_only_conflict_reason_at_zero() {
        use reprise_core::library_doctor::DoctorReviewRowState;

        let stale = super::album_header_state(0, 0, 3, Some(DoctorReviewRowState::Stale));
        assert_eq!(stale.pill, "3 changes · none selected");
        assert_eq!(stale.reason, None);
        assert!(!stale.check.sensitive);

        let conflict = super::album_header_state(0, 0, 2, Some(DoctorReviewRowState::Conflict));
        assert_eq!(conflict.pill, "2 changes · unresolved");
        assert_eq!(
            conflict.reason.as_deref(),
            Some("The spelling for this album is still unresolved — pick one below.")
        );
        assert!(!conflict.check.sensitive);

        let ready = super::album_header_state(1, 2, 3, None);
        assert_eq!(ready.pill, "1 change");
        assert_eq!(ready.reason, None);
        assert!(ready.check.sensitive);
    }
}
