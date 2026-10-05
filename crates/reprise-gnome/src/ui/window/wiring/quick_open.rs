//! Quick-open shortcut and one-snapshot-per-open search lifecycle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::prelude::GtkApplicationExt;
use reprise_view::quick_open::{rank_and_group, QuickOpenRecents, QuickOpenRow};

use super::quick_open_actions::{dispatch_item, show_all};
use super::quick_open_data::{load_candidates, CandidateSnapshot};
use super::{quick_open::QuickOpenPanel, RuntimeWiring, WiringScratch};

type QueryCallback = Rc<dyn Fn(&str, u64, bool)>;
type ActivateCallback = Rc<dyn Fn(QuickOpenRow, bool)>;

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
    let snapshot = Rc::new(RefCell::new(None::<CandidateSnapshot>));
    let load_generation = Rc::new(Cell::new(0u64));
    let loading = Rc::new(Cell::new(false));
    let pending_query = Rc::new(RefCell::new((String::new(), 0u64)));
    let recents = Rc::new(RefCell::new(QuickOpenRecents::default()));
    let database_path = w.db_path.to_path_buf();
    let live_db = w.conn.clone();

    let search: QueryCallback = Rc::new({
        let panel = panel.clone();
        let snapshot = snapshot.clone();
        let load_generation = load_generation.clone();
        let loading = loading.clone();
        let pending_query = pending_query.clone();
        let recents = recents.clone();
        move |query, generation, new_session| {
            let query = query.to_owned();
            pending_query.replace((query.clone(), generation));
            if new_session {
                snapshot.borrow_mut().take();
                load_generation.set(load_generation.get().wrapping_add(1));
                loading.set(false);
            }
            let latest_change = reprise_core::events::latest_id(&live_db).unwrap_or_default();
            let snapshot_is_current = snapshot
                .borrow()
                .as_ref()
                .is_some_and(|snapshot| snapshot.change_id == latest_change);
            if snapshot_is_current {
                apply_query(
                    &panel,
                    generation,
                    &query,
                    snapshot.borrow().as_ref(),
                    &recents,
                );
                return;
            }
            snapshot.borrow_mut().take();
            if loading.get() {
                return;
            }
            start_snapshot_load(
                &panel,
                &snapshot,
                &load_generation,
                &loading,
                database_path.clone(),
                pending_query.clone(),
                recents.clone(),
            );
        }
    });

    let player = w.player.clone();
    let conn = w.conn.clone();
    let navigator = w.metadata_navigator.clone();
    let section_search = scratch.section_search().clone();
    let activate: ActivateCallback = Rc::new(move |row, play_next| match row {
        QuickOpenRow::Item(item) => {
            recents.borrow_mut().remember(item.clone());
            dispatch_item(&item, play_next, player.as_ref(), &conn, &navigator);
        }
        QuickOpenRow::ShowAll { kind, query, .. } => {
            show_all(kind, &query, &navigator, &section_search);
        }
    });
    wire_quick_open_shortcut(w.app, w.window, &panel, search, activate);
}

fn start_snapshot_load(
    panel: &Rc<QuickOpenPanel>,
    snapshot: &Rc<RefCell<Option<CandidateSnapshot>>>,
    load_generation: &Rc<Cell<u64>>,
    loading: &Rc<Cell<bool>>,
    database_path: std::path::PathBuf,
    pending_query: Rc<RefCell<(String, u64)>>,
    recents: Rc<RefCell<QuickOpenRecents>>,
) {
    let expected_load = load_generation.get().wrapping_add(1);
    load_generation.set(expected_load);
    loading.set(true);
    let receiver = match crate::ui::one_shot_task::spawn("reprise-quick-open", move || {
        load_candidates(&database_path)
    }) {
        Ok(receiver) => receiver,
        Err(error) => {
            loading.set(false);
            let generation = pending_query.borrow().1;
            panel.set_error(generation, &error.to_string());
            return;
        }
    };
    let panel = panel.clone();
    let snapshot_slot = snapshot.clone();
    let load_generation = load_generation.clone();
    let loading = loading.clone();
    gtk4::glib::spawn_future_local(async move {
        let result = receiver.recv().await;
        if load_generation.get() != expected_load {
            return;
        }
        loading.set(false);
        let (query, result_generation) = pending_query.borrow().clone();
        if !panel.accepts_generation(result_generation) {
            return;
        }
        match result {
            Ok(Ok(loaded)) => {
                snapshot_slot.replace(Some(loaded));
                apply_query(
                    &panel,
                    result_generation,
                    &query,
                    snapshot_slot.borrow().as_ref(),
                    &recents,
                );
            }
            Ok(Err(error)) => panel.set_error(result_generation, &error),
            Err(error) => panel.set_error(result_generation, &error.to_string()),
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
        rank_and_group(snapshot.candidates.clone(), query)
            .into_iter()
            .flat_map(|group| group.rows)
            .collect()
    };
    panel.set_results(generation, rows);
}
