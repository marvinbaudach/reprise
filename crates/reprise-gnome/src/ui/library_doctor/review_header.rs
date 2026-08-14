use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library_doctor::{DoctorReviewRowId, DoctorReviewRowState};

use super::review_model::{row_state_reason, ReviewRowModel};
use crate::ui::strings;

pub(super) type OnSelect = Rc<dyn Fn(&[DoctorReviewRowId], bool)>;

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
    let blocked_by = blocked_by.unwrap_or(DoctorReviewRowState::Stale);
    let pill = match blocked_by {
        DoctorReviewRowState::Conflict => strings::doctor_change_count_unresolved(changes),
        DoctorReviewRowState::Ready | DoctorReviewRowState::Stale => {
            strings::doctor_change_count_out_of_date(changes)
        }
    };
    AlbumHeaderState {
        check,
        pill,
        reason: row_state_reason(blocked_by).map(strings::text),
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
) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    {
        let model = model.clone();
        let on_select = on_select.clone();
        factory.connect_setup(move |_, object| {
            let Some(header) = object.downcast_ref::<gtk4::ListHeader>() else {
                return;
            };
            for property in ["start", "end"] {
                let model = model.clone();
                let on_select = on_select.clone();
                header.connect_notify_local(Some(property), move |header, _| {
                    bind_album_header(header, &model, &on_select);
                });
            }
        });
    }
    {
        let model = model.clone();
        let on_select = on_select.clone();
        factory.connect_bind(move |_, object| {
            let Some(header) = object.downcast_ref::<gtk4::ListHeader>() else {
                return;
            };
            bind_album_header(header, &model, &on_select);
        });
    }
    factory
}

fn bind_album_header(header: &gtk4::ListHeader, model: &gtk4::SortListModel, on_select: &OnSelect) {
    let Some(first) = row_at(model, header.start()) else {
        tracing::warn!(
            start = header.start(),
            end = header.end(),
            "DOC-9b kept the last known header while its section row was unavailable"
        );
        return;
    };
    let rows = (header.start()..header.end())
        .filter_map(|position| row_at(model, position))
        .collect::<Vec<_>>();
    let row_ids = rows
        .iter()
        .flat_map(|row| row.selectable_row_ids.iter().copied())
        .collect::<Vec<_>>();
    let selected = rows
        .iter()
        .map(|row| row.selected_change_count)
        .sum::<usize>();
    let changes = rows.iter().map(|row| row.row_ids.len()).sum::<usize>();
    let blocked_by = if rows
        .iter()
        .any(|row| row.row.state == DoctorReviewRowState::Stale)
    {
        Some(DoctorReviewRowState::Stale)
    } else if rows
        .iter()
        .any(|row| row.row.state == DoctorReviewRowState::Conflict)
    {
        Some(DoctorReviewRowState::Conflict)
    } else {
        None
    };
    let total = row_ids.len();
    let checkbox = gtk4::CheckButton::new();
    checkbox.set_size_request(16, 16);
    checkbox.add_css_class("doctor-album-check");
    let state = album_header_state(selected, total, changes, blocked_by);
    checkbox.set_active(state.check.active);
    checkbox.set_inconsistent(state.check.inconsistent);
    checkbox.set_sensitive(state.check.sensitive);
    checkbox.update_property(&[gtk4::accessible::Property::Label(&state.pill)]);
    // a11y-semantics: role=checkbox name=change-count state=selection action=toggle
    checkbox.set_focusable(true);
    let callback = on_select.clone();
    checkbox.connect_toggled(move |button| callback(&row_ids, button.is_active()));

    let cover = gtk4::Image::from_icon_name("audio-x-generic-symbolic");
    cover.set_size_request(38, 38);
    cover.add_css_class("doctor-album-cover");
    let title = if first.album_title.trim().is_empty() {
        strings::text(strings::DOCTOR_NO_ALBUM)
    } else {
        first.album_title.clone()
    };
    let title_label = gtk4::Label::builder()
        .label(&title)
        .xalign(0.0)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .css_classes(["heading", "doctor-album-title"])
        .build();
    let track_count = strings::drag_tracks_label(first.album_track_count);
    let detail_text = if first.album_artist.trim().is_empty() {
        track_count
    } else {
        format!("{} · {track_count}", first.album_artist)
    };
    let detail = gtk4::Label::builder()
        .label(detail_text)
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
        .label(&state.pill)
        .halign(gtk4::Align::End)
        .css_classes(["tag"])
        .build();
    let caret = gtk4::Image::builder()
        .icon_name("pan-end-symbolic")
        .pixel_size(16)
        .css_classes(["doctor-album-caret"])
        .build();
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
    root.set_margin_top(if first.album_position == 0 { 16 } else { 8 });
    root.set_margin_bottom(10);
    root.set_margin_start(28);
    root.set_margin_end(28);
    root.add_css_class(if first.album_position == 0 {
        "doctor-album-header-first"
    } else {
        "doctor-album-header-later"
    });
    root.append(&checkbox);
    root.append(&cover);
    root.append(&copy);
    root.append(&pill);
    root.append(&caret);
    root.set_tooltip_text(state.reason.as_deref());
    let accessible_label = state.reason.as_ref().map_or_else(
        || format!("{} · {} · {}", title, first.album_artist, state.pill),
        |reason| {
            format!(
                "{} · {} · {} · {reason}",
                title, first.album_artist, state.pill
            )
        },
    );
    root.update_property(&[gtk4::accessible::Property::Label(&accessible_label)]);
    header.set_child(Some(&root));
}

fn album_change_count(total: usize, selected: usize) -> String {
    if selected == 0 && total > 0 {
        strings::doctor_change_count_none_selected(total)
    } else {
        strings::doctor_change_count(selected)
    }
}

fn row_at(model: &gtk4::SortListModel, position: u32) -> Option<ReviewRowModel> {
    let object = model
        .item(position)?
        .downcast::<glib::BoxedAnyObject>()
        .ok()?;
    let row = object.borrow::<ReviewRowModel>().clone();
    Some(row)
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
    fn doc_3c_album_header_state_names_the_reason_at_zero() {
        use reprise_core::library_doctor::DoctorReviewRowState;

        let stale = super::album_header_state(0, 0, 3, Some(DoctorReviewRowState::Stale));
        assert_eq!(stale.pill, "3 changes · out of date");
        assert_eq!(
            stale.reason.as_deref(),
            Some("This file changed after the scan — scan again to include this fix.")
        );
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
