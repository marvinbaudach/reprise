//! The experimental "Create instrumental" context-menu trigger (INST-1),
//! extracted from `track_list_context_menu` so both that adapter and the pure
//! `track_menu` decision module stay under the file-size budget. It owns the
//! gate decision, the menu section, the `"tracklist"` action, and the enqueue
//! handler.

use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;

use super::track_list_context_menu::current_selection_tracks;
use super::track_menu::SelectionSummary;
use crate::ui::strings;
use crate::ui::track_list::{show_toast, Shared};

/// The bare action name in the `"tracklist"` group.
const ACTION: &str = "create-instrumental";

/// INST-1: whether the trigger should appear for this selection. Gated on the
/// experimental switch (INST-11) and only offered when at least one selected
/// track is present — a missing file cannot be separated. Pure so it is
/// testable without a display.
pub(in crate::ui) fn create_instrumental_visible(
    instrumental_enabled: bool,
    production_backend_available: bool,
    selection: &SelectionSummary,
) -> bool {
    instrumental_enabled
        && production_backend_available
        && selection.count > 0
        && !selection.all_missing
}

/// Appends the "Create instrumental" section to the context menu when the gate
/// permits it.
pub(super) fn append_section(menu: &gio::Menu, shared: &Rc<Shared>, summary: &SelectionSummary) {
    let instrumental_enabled = crate::ui::instrumental::experimental_enabled(&shared.conn.borrow());
    if !create_instrumental_visible(
        instrumental_enabled,
        crate::ui::instrumental::production_backend_compiled(),
        summary,
    ) {
        return;
    }
    let section = gio::Menu::new();
    section.append(
        Some(&strings::text(strings::CONTEXT_MENU_CREATE_INSTRUMENTAL)),
        Some(&format!("tracklist.{ACTION}")),
    );
    menu.append_section(None, &section);
}

/// Registers the `create-instrumental` action on the tracklist action group.
pub(super) fn wire_action(action_group: &gio::SimpleActionGroup, shared: &Rc<Shared>) {
    let action = gio::SimpleAction::new(ACTION, None);
    let shared = shared.clone();
    action.connect_activate(move |_, _| handle(&shared));
    action_group.add_action(&action);
}

/// Enqueues one instrumental job per present selected track under a shared batch
/// (`ai_conversion::add_batch_to_conversion` — dedup skips already-converted
/// tracks with a reference, INST-9), nudges the worker, and toasts the outcome.
fn handle(shared: &Rc<Shared>) {
    let ids: Vec<i64> = current_selection_tracks(shared)
        .into_iter()
        .filter(|track| !track.is_missing())
        .map(|track| track.id)
        .collect();
    if ids.is_empty() {
        tracing::debug!("context menu: create-instrumental with no present selection; ignoring");
        return;
    }
    let staging = reprise_core::ai_staging::StagingStore::with_default_dir();
    let Some(model_id) = crate::ui::instrumental::app_model_id() else {
        tracing::error!("context menu: production stem backend is unavailable");
        show_toast(shared, &strings::create_instrumental_failed_toast());
        return;
    };
    let now = crate::ui::instrumental::now_unix();
    let outcome = {
        let conn = shared.conn.borrow();
        // `auto_promote = false`: the context-menu/drop path stages every render
        // for a manual save decision and never auto-promotes (decision 15; see
        // `ai_jobs::enqueue_instrumental`). Only the MCP/CLI batch path saves by
        // default.
        reprise_core::ai_conversion::add_batch_to_conversion(
            &conn, &staging, &ids, &model_id, false, now,
        )
    };
    match outcome {
        Ok(batch) => {
            crate::ui::instrumental::wake_worker();
            let created = batch
                .jobs
                .iter()
                .filter(|outcome| {
                    matches!(
                        outcome,
                        reprise_core::ai_jobs::EnqueueOutcome::Created { .. }
                    )
                })
                .count();
            let deduped = batch.jobs.len() - created;
            tracing::info!(
                created,
                deduped,
                "context menu: instrumental conversions queued"
            );
            show_toast(
                shared,
                &strings::create_instrumental_toast(created, deduped),
            );
        }
        Err(error) => {
            tracing::error!(%error, "context menu: create instrumental failed");
            show_toast(shared, &strings::create_instrumental_failed_toast());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(count: usize) -> SelectionSummary {
        SelectionSummary {
            count,
            any_missing: false,
            all_missing: false,
            same_album: true,
            same_artist: true,
            same_folder: true,
        }
    }

    // UX INST-1: the "Create instrumental" trigger appears only when the
    // experimental switch is on and at least one selected track is present.
    #[test]
    fn inst_1_create_instrumental_visible_iff_experimental_and_present_selection() {
        let present = selection(2);
        assert!(
            create_instrumental_visible(true, true, &present),
            "shown for a present selection when experimental is on"
        );
        assert!(
            !create_instrumental_visible(false, true, &present),
            "hidden while the experimental switch is off (INST-11)"
        );
        assert!(
            !create_instrumental_visible(true, false, &present),
            "a user build without a production backend never offers a fake conversion"
        );
        let all_missing = SelectionSummary {
            all_missing: true,
            ..present
        };
        assert!(
            !create_instrumental_visible(true, true, &all_missing),
            "a missing file cannot be separated"
        );
        let empty = SelectionSummary {
            count: 0,
            ..present
        };
        assert!(!create_instrumental_visible(true, true, &empty));
    }
}
