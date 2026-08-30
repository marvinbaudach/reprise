use reprise_core::library_doctor::{DoctorScopeRequest, DoctorViewSnapshot};
use reprise_core::view_source::ViewSource;

use crate::ui::track_list::TrackList;

pub(super) fn current_view_snapshot(track_list: &TrackList) -> DoctorViewSnapshot {
    let shared = &track_list.shared;
    let source = shared.source.borrow().clone();
    let sort = shared.sort.borrow().clone();
    let queue_ids = if source == ViewSource::Queue {
        shared.current_view_ids()
    } else {
        Vec::new()
    };
    DoctorViewSnapshot {
        source,
        sort_field: sort.field,
        sort_dir: sort.dir,
        filter: shared.filter.borrow().clone(),
        browse: shared.browse_filter.borrow().clone(),
        queue_ids,
    }
}

pub(super) fn suggested_scope(view: &DoctorViewSnapshot) -> u32 {
    if view.filter.is_empty() && view.browse.is_empty() {
        0
    } else {
        1
    }
}

pub(super) fn scope_choice(scope_kind: &str) -> u32 {
    match scope_kind {
        "current_view" => 1,
        "selection" => 2,
        _ => 0,
    }
}

pub(super) fn scope_request(
    choice: u32,
    current_view: DoctorViewSnapshot,
    selection: Vec<i64>,
) -> DoctorScopeRequest {
    match choice {
        1 => DoctorScopeRequest::CurrentView(Box::new(current_view)),
        2 => DoctorScopeRequest::Selection {
            track_ids: selection,
        },
        _ => DoctorScopeRequest::WholeLibrary,
    }
}
