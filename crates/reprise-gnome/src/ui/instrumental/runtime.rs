//! Live lifetime management for the experimental instrumental surface.
//!
//! The preference gate can change while the main window is running. This
//! runtime makes that transition idempotent: enable creates one view and one
//! idle worker supervisor; disable removes every visible surface, disconnects
//! enqueue wakeups, and stops supervising new work.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::ai_staging::StagingStore;
use rusqlite::Connection;

use super::conversion_view::ConversionView;
use super::conversion_wiring::{
    clear_ensure_page_hook, install_conversions_page, remove_conversions_page, saved_job_count,
    set_ensure_page_hook, wire_callbacks, ConversionWiring,
};
use super::worker_host::InstrumentalWorker;
use crate::ui::player_controller::PlayerController;
use crate::ui::track_list::TrackList;

struct RuntimeState {
    active: bool,
    progress_generation: u64,
    view: Option<Rc<ConversionView>>,
    worker: Option<InstrumentalWorker>,
}

struct InstrumentalRuntime {
    conn: Rc<RefCell<Connection>>,
    db_path: PathBuf,
    window: glib::WeakRef<adw::ApplicationWindow>,
    content_stack: glib::WeakRef<gtk4::Stack>,
    toast_overlay: glib::WeakRef<adw::ToastOverlay>,
    track_list: Weak<TrackList>,
    player: Option<Weak<PlayerController>>,
    staging: StagingStore,
    state: RefCell<RuntimeState>,
}

impl InstrumentalRuntime {
    fn new(deps: &ConversionWiring<'_>) -> Rc<Self> {
        Rc::new(Self {
            conn: deps.conn.clone(),
            db_path: deps.db_path.to_path_buf(),
            window: deps.window.downgrade(),
            content_stack: deps.content_stack.downgrade(),
            toast_overlay: deps.toast_overlay.downgrade(),
            track_list: Rc::downgrade(deps.track_list),
            player: deps.player.as_ref().map(Rc::downgrade),
            staging: StagingStore::with_default_dir(),
            state: RefCell::new(RuntimeState {
                active: false,
                progress_generation: 0,
                view: None,
                worker: None,
            }),
        })
    }

    fn set_enabled(self: &Rc<Self>, enabled: bool) {
        let changed = {
            let mut state = self.state.borrow_mut();
            if state.active == enabled {
                false
            } else {
                state.active = enabled;
                true
            }
        };
        if !changed {
            return;
        }
        if enabled {
            self.enable();
        } else {
            self.disable();
        }
    }

    fn enable(self: &Rc<Self>) {
        if let Err(error) = self.staging.ensure_dir() {
            tracing::warn!(%error, "instrumental: could not create staging dir");
        }
        self.ensure_surface();

        if !super::production_backend_compiled() {
            tracing::warn!("instrumental: packaged stem worker is not compiled in");
            return;
        }
        let worker = match InstrumentalWorker::new(&self.db_path, &self.staging) {
            Ok(worker) => worker,
            Err(error) => {
                tracing::error!(%error, "instrumental: could not start worker supervisor");
                return;
            }
        };
        super::set_wake_hook({
            let worker = worker.clone();
            Rc::new(move || worker.wake())
        });
        self.start_progress_monitor(&worker);
        self.state.borrow_mut().worker = Some(worker);
    }

    fn ensure_surface(&self) {
        if !super::experimental_enabled(&self.conn.borrow()) || self.state.borrow().view.is_some() {
            return;
        }
        let (Some(window), Some(content_stack), Some(toast_overlay), Some(track_list)) = (
            self.window.upgrade(),
            self.content_stack.upgrade(),
            self.toast_overlay.upgrade(),
            self.track_list.upgrade(),
        ) else {
            return;
        };
        let Some(view) = install_conversions_page(&content_stack, &self.conn, &self.staging) else {
            return;
        };
        let player = self.player.as_ref().and_then(Weak::upgrade);
        let deps = ConversionWiring {
            conn: &self.conn,
            db_path: &self.db_path,
            window: &window,
            content_stack: &content_stack,
            toast_overlay: &toast_overlay,
            track_list: &track_list,
            player: &player,
        };
        wire_callbacks(&view, &self.staging, &deps);
        self.state.borrow_mut().view = Some(view);
    }

    fn start_progress_monitor(self: &Rc<Self>, worker: &InstrumentalWorker) {
        let generation = {
            let mut state = self.state.borrow_mut();
            state.progress_generation = state.progress_generation.wrapping_add(1);
            state.progress_generation
        };
        let receiver = worker.progress_receiver();
        let runtime = Rc::downgrade(self);
        let saved_baseline = Cell::new(saved_job_count(&self.conn));
        glib::spawn_future_local(async move {
            while receiver.recv().await.is_ok() {
                let Some(runtime) = runtime.upgrade() else {
                    return;
                };
                let view = {
                    let state = runtime.state.borrow();
                    if !state.active || state.progress_generation != generation {
                        return;
                    }
                    state.view.clone()
                };
                if let Some(view) = view {
                    view.refresh();
                }
                let saved_now = saved_job_count(&runtime.conn);
                if saved_now > saved_baseline.get() {
                    saved_baseline.set(saved_now);
                    if let Some(track_list) = runtime.track_list.upgrade() {
                        track_list.reload();
                    }
                }
            }
        });
    }

    fn disable(&self) {
        super::clear_wake_hook();
        let (worker, view) = {
            let mut state = self.state.borrow_mut();
            state.progress_generation = state.progress_generation.wrapping_add(1);
            (state.worker.take(), state.view.take())
        };
        if let Some(worker) = worker {
            // A render already accepted by the finite child is left to finish;
            // no later enqueue can reach this stopped supervisor.
            worker.shutdown();
        }
        if let (Some(_view), Some(content_stack)) = (view, self.content_stack.upgrade()) {
            remove_conversions_page(&content_stack);
        }
    }

    fn shutdown(&self) {
        {
            let mut state = self.state.borrow_mut();
            state.active = false;
        }
        self.disable();
        super::clear_enabled_hook();
        clear_ensure_page_hook();
    }
}

pub(super) fn install(deps: &ConversionWiring<'_>) {
    let runtime = InstrumentalRuntime::new(deps);
    super::set_enabled_hook({
        let runtime = Rc::downgrade(&runtime);
        Rc::new(move |enabled| {
            if let Some(runtime) = runtime.upgrade() {
                runtime.set_enabled(enabled);
            }
        })
    });
    set_ensure_page_hook({
        let runtime = Rc::downgrade(&runtime);
        Rc::new(move || {
            if let Some(runtime) = runtime.upgrade() {
                runtime.ensure_surface();
            }
        })
    });
    deps.window.connect_close_request({
        let runtime = runtime.clone();
        move |_| {
            runtime.shutdown();
            glib::Propagation::Proceed
        }
    });

    let enabled = super::experimental_enabled(&deps.conn.borrow());
    runtime.set_enabled(enabled);
}
