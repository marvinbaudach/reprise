use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library_doctor::{
    DoctorReviewFilter, DoctorReviewRowId, DoctorReviewRowState, DoctorReviewSession, DoctorScan,
};
use rusqlite::Connection;

use super::review_model::{rows_for, ReviewRowModel};
use crate::ui::preferences::preference_library_doctor;
use crate::ui::strings;

struct ReviewState {
    scan: DoctorScan,
    session: RefCell<DoctorReviewSession>,
    store: gio::ListStore,
    selection: gtk4::SingleSelection,
    content: gtk4::Stack,
    groups: gtk4::Box,
    apply_summary: gtk4::Label,
    change_summary: gtk4::Label,
}

impl ReviewState {
    fn refresh(self: &Rc<Self>) {
        let selected = self.selection.selected();
        self.store.remove_all();
        let session = self.session.borrow();
        for row in rows_for(&self.scan, &session) {
            self.store.append(&glib::BoxedAnyObject::new(row));
        }
        let count = self.store.n_items();
        self.content
            .set_visible_child_name(if count == 0 { "empty" } else { "rows" });
        if count > 0 && selected != gtk4::INVALID_LIST_POSITION {
            self.selection.set_selected(selected.min(count - 1));
        }
        let summary = session.summary();
        self.apply_summary
            .set_label(&strings::doctor_apply_tracks(summary.track_count));
        self.change_summary
            .set_label(&strings::doctor_apply_summary(
                summary.tag_change_count,
                summary.file_count,
            ));
        self.refresh_groups();
    }

    fn set_selected(self: &Rc<Self>, row_id: DoctorReviewRowId, selected: bool) {
        if let Err(error) = self.session.borrow_mut().set_selected(row_id, selected) {
            tracing::warn!(%error, "could not update Library Doctor review selection");
        }
        self.refresh();
    }

    fn toggle_position(self: &Rc<Self>, position: u32) {
        let Some(boxed) = self
            .store
            .item(position)
            .and_then(|object| object.downcast::<glib::BoxedAnyObject>().ok())
        else {
            return;
        };
        let model = boxed.borrow::<ReviewRowModel>();
        self.set_selected(model.row.id, !model.row.selected);
    }

    fn set_remote_visible(self: &Rc<Self>, visible: bool) {
        self.session.borrow_mut().set_remote_visible(visible);
        self.refresh();
    }

    fn mark_paths_stale(self: &Rc<Self>, paths: &[PathBuf]) {
        let track_ids = self
            .scan
            .tracks
            .iter()
            .filter(|track| paths.contains(&track.reference.path))
            .map(|track| track.reference.track_id)
            .collect::<Vec<_>>();
        let ids = self
            .session
            .borrow()
            .rows()
            .iter()
            .filter(|row| track_ids.contains(&row.track_id))
            .map(|row| row.id)
            .collect::<Vec<_>>();
        let mut session = self.session.borrow_mut();
        for id in ids {
            let _ = session.mark_state(id, DoctorReviewRowState::Stale);
        }
        drop(session);
        self.refresh();
    }

    fn refresh_groups(self: &Rc<Self>) {
        while let Some(child) = self.groups.first_child() {
            self.groups.remove(&child);
        }
        let groups = self.session.borrow().groups().to_vec();
        self.groups.set_visible(!groups.is_empty());
        if groups.is_empty() {
            return;
        }
        let heading = gtk4::Label::builder()
            .label(strings::text(strings::DOCTOR_UNRESOLVED_GROUPS))
            .xalign(0.0)
            .css_classes(["heading"])
            .build();
        self.groups.append(&heading);
        for group in groups {
            let labels = group
                .candidates
                .iter()
                .map(super::review_model::candidate_description)
                .collect::<Vec<_>>();
            let model =
                gtk4::StringList::new(&labels.iter().map(String::as_str).collect::<Vec<_>>());
            let row = adw::ComboRow::builder()
                .title(strings::doctor_unresolved_spellings(group.candidates.len()))
                .subtitle(strings::text(strings::DOCTOR_PICK_ONE))
                .model(&model)
                .selected(gtk4::INVALID_LIST_POSITION)
                .build();
            row.set_tooltip_text(Some(&labels.join("\n")));
            let weak = Rc::downgrade(self);
            let candidates = group.candidates.clone();
            row.connect_selected_notify(move |row| {
                let position = row.selected();
                if position == gtk4::INVALID_LIST_POSITION {
                    return;
                }
                let Some(state) = weak.upgrade() else {
                    return;
                };
                let Some(candidate) = candidates.get(position as usize) else {
                    return;
                };
                if let Err(error) = state
                    .session
                    .borrow_mut()
                    .choose_candidate(group.id, &candidate.value)
                {
                    tracing::warn!(%error, "could not choose Library Doctor spelling");
                    return;
                }
                state.refresh();
            });
            self.groups.append(&row);
        }
    }
}

pub(super) struct LibraryDoctorReviewPage {
    navigation_page: adw::NavigationPage,
    filter: DoctorReviewFilter,
    state: Rc<ReviewState>,
    remote: adw::SwitchRow,
}

impl LibraryDoctorReviewPage {
    pub(super) fn new(
        conn: &Rc<RefCell<Connection>>,
        parent: &adw::ApplicationWindow,
        scan: DoctorScan,
        filter: DoctorReviewFilter,
        on_remote_changed: Rc<dyn Fn(bool)>,
        on_edit: &Rc<dyn Fn(i64)>,
    ) -> Rc<Self> {
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let selection = gtk4::SingleSelection::builder()
            .model(&store)
            .autoselect(false)
            .can_unselect(true)
            .build();
        let rows = gtk4::ListView::builder()
            .model(&selection)
            .single_click_activate(false)
            .build();
        let empty = adw::StatusPage::builder()
            .icon_name("emblem-ok-symbolic")
            .title(strings::text(strings::DOCTOR_NO_CHANGES))
            .description(strings::text(strings::DOCTOR_NO_CHANGES_DESCRIPTION))
            .build();
        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .child(&rows)
            .build();
        let content = gtk4::Stack::new();
        content.set_vexpand(true);
        content.add_named(&scrolled, Some("rows"));
        content.add_named(&empty, Some("empty"));
        let groups = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        groups.set_margin_start(18);
        groups.set_margin_end(18);

        let apply_summary = gtk4::Label::builder()
            .xalign(0.0)
            .css_classes(["heading"])
            .build();
        let change_summary = gtk4::Label::builder()
            .xalign(0.0)
            .css_classes(["caption", "dim-label"])
            .build();
        let footer_copy = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        footer_copy.append(&apply_summary);
        footer_copy.append(&change_summary);
        let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        footer.set_margin_top(12);
        footer.set_margin_bottom(12);
        footer.set_margin_start(18);
        footer.set_margin_end(18);
        footer.append(&footer_copy);

        let state = Rc::new(ReviewState {
            scan: scan.clone(),
            session: RefCell::new(DoctorReviewSession::from_scan(scan, filter)),
            store,
            selection,
            content,
            groups,
            apply_summary,
            change_summary,
        });
        let on_select = {
            let state = state.clone();
            Rc::new(move |row_id, selected| state.set_selected(row_id, selected))
                as Rc<dyn Fn(DoctorReviewRowId, bool)>
        };
        rows.set_factory(Some(&super::review_row::factory(&on_select, on_edit)));
        {
            let state = state.clone();
            rows.connect_activate(move |_, position| state.toggle_position(position));
        }

        let all_safe = gtk4::Button::with_label(&strings::text(strings::DOCTOR_ALL_SAFE));
        let none = gtk4::Button::with_label(&strings::text(strings::DOCTOR_NONE));
        {
            let state = state.clone();
            all_safe.connect_clicked(move |_| {
                state.session.borrow_mut().all_safe();
                state.refresh();
            });
        }
        {
            let state = state.clone();
            none.connect_clicked(move |_| {
                state.session.borrow_mut().none();
                state.refresh();
            });
        }
        let presets = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        presets.append(&all_safe);
        presets.append(&none);
        let header = adw::HeaderBar::new();
        header.pack_end(&presets);

        let state_for_remote = state.clone();
        let remote = preference_library_doctor::remote_suggestions_row_for(
            conn,
            parent,
            true,
            Rc::new(move |visible| {
                state_for_remote.set_remote_visible(visible);
                on_remote_changed(visible);
            }),
        );
        state.set_remote_visible(remote.is_active());
        let options = adw::PreferencesGroup::new();
        options.add(&remote);
        let options_clamp = adw::Clamp::builder()
            .maximum_size(760)
            .child(&options)
            .build();
        let page_content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        page_content.set_margin_top(12);
        page_content.append(&options_clamp);
        page_content.append(&state.groups);
        page_content.append(&state.content);
        page_content.append(&footer);
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&page_content));
        let navigation_page = adw::NavigationPage::builder()
            .title(strings::text(strings::DOCTOR_REVIEW_TITLE))
            .tag("library-doctor-review")
            .child(&toolbar)
            .build();
        let page = Rc::new(Self {
            navigation_page,
            filter,
            state,
            remote,
        });
        page.state.refresh();
        page
    }

    pub(super) fn navigation_page(&self) -> &adw::NavigationPage {
        &self.navigation_page
    }

    pub(super) const fn filter(&self) -> DoctorReviewFilter {
        self.filter
    }

    pub(super) fn mark_paths_stale(&self, paths: &[PathBuf]) {
        self.state.mark_paths_stale(paths);
    }

    pub(super) fn set_remote_active(&self, active: bool) {
        if self.remote.is_active() != active {
            self.remote.set_active(active);
        } else {
            self.state.set_remote_visible(active);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use reprise_core::library::tag_edit::EditableTags;
    use reprise_core::library_doctor::{
        DoctorField, DoctorProposal, DoctorScanOptions, DoctorTrackRef, DoctorTrackSnapshot,
        DoctorValue, ProblemClass, ProposalSource,
    };

    use super::*;

    fn scan() -> DoctorScan {
        DoctorScan {
            id: 1,
            scope_kind: "whole_library".into(),
            created_at: 2,
            options: DoctorScanOptions::local_only(),
            checked_tracks: 1,
            skipped_tracks: 0,
            track_ids: vec![7],
            tracks: vec![DoctorTrackSnapshot {
                reference: DoctorTrackRef {
                    track_id: 7,
                    path: PathBuf::from("/tmp/doctor-review.flac"),
                    file_mtime: 1,
                    file_size: 2,
                    device: None,
                    inode: None,
                },
                tags: Some(EditableTags {
                    title: "Review track".into(),
                    artist: "Artist".into(),
                    album: "Album".into(),
                    album_artist: "Artist".into(),
                    year: Some(2020),
                    track_no: Some(1),
                    genre: "Rock".into(),
                }),
                stale: false,
            }],
            proposals: vec![DoctorProposal {
                track_id: 7,
                field: DoctorField::Genre,
                current: DoctorValue::Text(" rock ".into()),
                proposed: DoctorValue::Text("rock".into()),
                source: ProposalSource::Local,
                confidence: 100,
                preselected: true,
                problem_class: ProblemClass::CasingWhitespace,
                evidence: Vec::new(),
                local_fallback: None,
            }],
            unresolved_groups: Vec::new(),
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn doc_3b_review_page_virtualizes_rows_without_horizontal_scroll() {
        gtk4::init().unwrap();
        let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
        let parent = adw::ApplicationWindow::builder().build();
        let on_edit = Rc::new(|_| {}) as Rc<dyn Fn(i64)>;

        let page = LibraryDoctorReviewPage::new(
            &conn,
            &parent,
            scan(),
            DoctorReviewFilter::AllChanges,
            Rc::new(|_| {}),
            &on_edit,
        );

        assert_eq!(page.state.store.n_items(), 1);
        assert_eq!(
            page.state.content.visible_child_name().as_deref(),
            Some("rows")
        );
        let scrolled = page
            .state
            .content
            .child_by_name("rows")
            .and_downcast::<gtk4::ScrolledWindow>()
            .unwrap();
        assert_eq!(scrolled.hscrollbar_policy(), gtk4::PolicyType::Never);
        assert!(scrolled.child().unwrap().is::<gtk4::ListView>());

        page.mark_paths_stale(&[PathBuf::from("/tmp/doctor-review.flac")]);

        let session = page.state.session.borrow();
        assert_eq!(session.rows()[0].state, DoctorReviewRowState::Stale);
        assert!(!session.rows()[0].selected);
    }
}
