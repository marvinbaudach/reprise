use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library_doctor::DoctorReviewRowId;

use super::review_model::ReviewRowModel;
use crate::ui::strings;

pub(super) type OnSelect = Rc<dyn Fn(&[DoctorReviewRowId], bool)>;

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
    pub(super) groups: ReviewColumnGroups,
}

impl ReviewHeader {
    pub(super) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.add_css_class("dim-label");
        let groups = ReviewColumnGroups::new();
        for (group, text) in [
            (&groups.selection, ""),
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
            root.append(&label);
        }
        Self { root, groups }
    }
}

pub(super) fn album_header_factory(
    model: &gtk4::SortListModel,
    on_select: &OnSelect,
) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    let model = model.clone();
    let on_select = on_select.clone();
    factory.connect_bind(move |_, object| {
        let Some(header) = object.downcast_ref::<gtk4::ListHeader>() else {
            return;
        };
        let Some(first) = row_at(&model, header.start()) else {
            header.set_child(gtk4::Widget::NONE);
            return;
        };
        let rows = (header.start()..header.end())
            .filter_map(|position| row_at(&model, position))
            .collect::<Vec<_>>();
        let row_ids = rows
            .iter()
            .flat_map(|row| row.selectable_row_ids.iter().copied())
            .collect::<Vec<_>>();
        let selected = rows
            .iter()
            .map(|row| row.selected_change_count)
            .sum::<usize>();
        let checkbox = gtk4::CheckButton::new();
        checkbox.set_active(selected == row_ids.len() && !row_ids.is_empty());
        checkbox.set_inconsistent(selected > 0 && selected < row_ids.len());
        checkbox.update_property(&[gtk4::accessible::Property::Label(
            &strings::doctor_change_count(row_ids.len()),
        )]);
        // a11y-semantics: role=checkbox name=album-changes state=selected action=toggle
        checkbox.set_focusable(true);
        let callback = on_select.clone();
        checkbox.connect_toggled(move |button| callback(&row_ids, button.is_active()));

        let cover = gtk4::Image::from_icon_name("audio-x-generic-symbolic");
        cover.set_size_request(38, 38);
        let title = if first.album_title.trim().is_empty() {
            strings::text(strings::DOCTOR_NO_ALBUM)
        } else {
            first.album_title.clone()
        };
        let title_label = gtk4::Label::builder()
            .label(&title)
            .xalign(0.0)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .css_classes(["heading"])
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
            .css_classes(["caption", "dim-label"])
            .build();
        let copy = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        copy.set_hexpand(true);
        copy.append(&title_label);
        copy.append(&detail);
        let pill = gtk4::Label::builder()
            .label(strings::doctor_change_count(selected))
            .css_classes(["caption", "pill"])
            .build();
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        root.set_margin_top(12);
        root.set_margin_bottom(6);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.append(&checkbox);
        root.append(&cover);
        root.append(&copy);
        root.append(&pill);
        root.update_property(&[gtk4::accessible::Property::Label(&format!(
            "{} · {} · {}",
            title,
            first.album_artist,
            strings::doctor_change_count(selected)
        ))]);
        header.set_child(Some(&root));
    });
    factory.connect_unbind(|_, object| {
        if let Some(header) = object.downcast_ref::<gtk4::ListHeader>() {
            header.set_child(gtk4::Widget::NONE);
        }
    });
    factory
}

fn row_at(model: &gtk4::SortListModel, position: u32) -> Option<ReviewRowModel> {
    let object = model
        .item(position)?
        .downcast::<glib::BoxedAnyObject>()
        .ok()?;
    let row = object.borrow::<ReviewRowModel>().clone();
    Some(row)
}
