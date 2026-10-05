//! Quick-open shortcut and one-snapshot-per-open search lifecycle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::prelude::GtkApplicationExt;
use reprise_view::quick_open::{rank_and_group, QuickOpenRecents, QuickOpenRow};

use super::quick_open_actions::{dispatch_item, show_all, RuntimeDispatch};
use super::quick_open_data::{load_candidates, CandidateSnapshot};
use super::{quick_open::QuickOpenPanel, RuntimeWiring, WiringScratch};

type QueryCallback = Rc<dyn Fn(&str, u64, bool)>;
type ActivateCallback = Rc<dyn Fn(QuickOpenRow, bool)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuerySource {
    Recents,
    Snapshot,
    Waiting,
    Error,
}

#[derive(Clone)]
struct SnapshotLoadState {
    snapshot: Rc<RefCell<Option<CandidateSnapshot>>>,
    generation: Rc<Cell<u64>>,
    loading: Rc<Cell<bool>>,
    failed: Rc<Cell<bool>>,
}

impl SnapshotLoadState {
    fn new() -> Self {
        Self {
            snapshot: Rc::new(RefCell::new(None)),
            generation: Rc::new(Cell::new(0)),
            loading: Rc::new(Cell::new(false)),
            failed: Rc::new(Cell::new(false)),
        }
    }

    fn clear(&self) {
        self.snapshot.borrow_mut().take();
        self.generation.set(self.generation.get().wrapping_add(1));
        self.loading.set(false);
        self.failed.set(false);
    }
}

fn query_source(query: &str, has_snapshot: bool, load_failed: bool) -> QuerySource {
    if query.trim().is_empty() {
        QuerySource::Recents
    } else if has_snapshot {
        QuerySource::Snapshot
    } else if load_failed {
        QuerySource::Error
    } else {
        QuerySource::Waiting
    }
}

pub(super) fn wire_quick_open_shortcut(
    app: &libadwaita::Application,
    window: &libadwaita::ApplicationWindow,
    panel: &Rc<QuickOpenPanel>,
    search: QueryCallback,
    activate: ActivateCallback,
) {
    panel.connect_query(move |query, generation, new_session| {
        search(&query, generation, new_session);
    });
    panel.connect_activate(move |row, play_next| activate(row, play_next));
    let action = gio::SimpleAction::new("quick-open", None);
    action.connect_activate({
        let panel = panel.clone();
        let window = window.downgrade();
        move |_, _| {
            if let Some(window) = window.upgrade() {
                panel.present(&window);
            }
        }
    });
    window.add_action(&action);
    app.set_accels_for_action("win.quick-open", &["<Control>k"]);
}

pub(super) fn wire_quick_open(w: &RuntimeWiring<'_>, scratch: &WiringScratch) {
    let panel = Rc::new(QuickOpenPanel::new());
    let load = SnapshotLoadState::new();
    let pending_query = Rc::new(RefCell::new((String::new(), 0u64)));
    let recents = Rc::new(RefCell::new(QuickOpenRecents::default()));
    let database_path = w.db_path.to_path_buf();

    let search: QueryCallback = Rc::new({
        let panel = panel.clone();
        let load = load.clone();
        let pending_query = pending_query.clone();
        let recents = recents.clone();
        move |query, generation, new_session| {
            let query = query.to_owned();
            pending_query.replace((query.clone(), generation));
            if new_session {
                load.clear();
                apply_query(&panel, generation, &query, None, &recents);
                start_snapshot_load(
                    &panel,
                    &load,
                    database_path.clone(),
                    pending_query.clone(),
                    recents.clone(),
                );
                return;
            }
            let source = query_source(&query, load.snapshot.borrow().is_some(), load.failed.get());
            match source {
                QuerySource::Recents => apply_query(&panel, generation, &query, None, &recents),
                QuerySource::Snapshot => apply_query(
                    &panel,
                    generation,
                    &query,
                    load.snapshot.borrow().as_ref(),
                    &recents,
                ),
                QuerySource::Error => panel.set_error(generation),
                QuerySource::Waiting => {}
            }
        }
    });
    panel.connect_closed({
        let load = load.clone();
        move || load.clear()
    });

    let player = w.player.clone();
    let conn = w.conn.clone();
    let navigator = w.metadata_navigator.clone();
    let section_search = scratch.section_search().clone();
    let activate: ActivateCallback = Rc::new(move |row, play_next| match row {
        QuickOpenRow::Item(item) => {
            recents.borrow_mut().remember(item.clone());
            let mut target = RuntimeDispatch::new(
                player.as_ref(),
                &conn,
                &navigator,
                super::super::source_connectivity::connectivity_for(
                    gio::NetworkMonitor::default().is_network_available(),
                ),
            );
            dispatch_item(&item, play_next, &mut target);
        }
        QuickOpenRow::ShowAll { kind, query, .. } => {
            show_all(kind, &query, &navigator, &section_search);
        }
    });
    wire_quick_open_shortcut(w.app, w.window, &panel, search, activate);
}

fn start_snapshot_load(
    panel: &Rc<QuickOpenPanel>,
    load: &SnapshotLoadState,
    database_path: std::path::PathBuf,
    pending_query: Rc<RefCell<(String, u64)>>,
    recents: Rc<RefCell<QuickOpenRecents>>,
) {
    let expected_load = load.generation.get().wrapping_add(1);
    load.generation.set(expected_load);
    load.loading.set(true);
    let receiver = match crate::ui::one_shot_task::spawn("reprise-quick-open", move || {
        load_candidates(&database_path)
    }) {
        Ok(receiver) => receiver,
        Err(error) => {
            load.loading.set(false);
            load.failed.set(true);
            tracing::error!(%error, "quick-open snapshot worker could not start");
            let generation = pending_query.borrow().1;
            if !pending_query.borrow().0.trim().is_empty() {
                panel.set_error(generation);
            }
            return;
        }
    };
    let panel = panel.clone();
    let load = load.clone();
    gtk4::glib::spawn_future_local(async move {
        let result = receiver.recv().await;
        if load.generation.get() != expected_load {
            return;
        }
        load.loading.set(false);
        let (query, result_generation) = pending_query.borrow().clone();
        if !panel.accepts_generation(result_generation) {
            return;
        }
        match result {
            Ok(Ok(loaded)) => {
                load.failed.set(false);
                tracing::debug!(
                    change_id = loaded.change_id,
                    candidates = loaded.candidates.len(),
                    "quick-open snapshot loaded"
                );
                load.snapshot.replace(Some(loaded));
                apply_query(
                    &panel,
                    result_generation,
                    &query,
                    load.snapshot.borrow().as_ref(),
                    &recents,
                );
            }
            Ok(Err(error)) => {
                load.failed.set(true);
                tracing::error!(detail = %error, "quick-open snapshot load failed");
                if !query.trim().is_empty() {
                    panel.set_error(result_generation);
                }
            }
            Err(error) => {
                load.failed.set(true);
                tracing::error!(%error, "quick-open snapshot worker stopped without a result");
                if !query.trim().is_empty() {
                    panel.set_error(result_generation);
                }
            }
        }
    });
}

fn apply_query(
    panel: &QuickOpenPanel,
    generation: u64,
    query: &str,
    snapshot: Option<&CandidateSnapshot>,
    recents: &RefCell<QuickOpenRecents>,
) {
    let rows = if query.trim().is_empty() {
        recents
            .borrow()
            .items()
            .into_iter()
            .map(QuickOpenRow::Item)
            .collect()
    } else {
        let Some(snapshot) = snapshot else {
            return;
        };
        rank_and_group(&snapshot.candidates, query)
            .into_iter()
            .flat_map(|group| group.rows)
            .collect()
    };
    panel.set_results(generation, rows);
}

#[cfg(test)]
mod tests {
    use super::{query_source, QuerySource};

    #[test]
    fn search_17_recents_do_not_wait_for_or_get_replaced_by_a_snapshot_error() {
        assert_eq!(query_source("", false, false), QuerySource::Recents);
        assert_eq!(query_source("", false, true), QuerySource::Recents);
        assert_eq!(query_source("blue", false, false), QuerySource::Waiting);
        assert_eq!(query_source("blue", false, true), QuerySource::Error);
        assert_eq!(query_source("blue", true, true), QuerySource::Snapshot);
    }
}
