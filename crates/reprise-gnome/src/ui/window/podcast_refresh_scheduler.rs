//! Window-level scheduling for automatic podcast refreshes.

use std::path::Path;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::db::Db;

use crate::ui::podcasts::{scope_status, RefreshWindow, ScopeStatus};

const REFRESH_TIMER_SECONDS: u32 = 60 * 60;

pub(in crate::ui) fn arm(
    conn: &Rc<Db>,
    db_path: &Path,
    runtime: &Rc<crate::ui::podcasts::PodcastsRuntime>,
    view: &Rc<crate::ui::podcasts::PodcastsView>,
) {
    let trigger = {
        let conn = conn.clone();
        let db_path = db_path.to_path_buf();
        let runtime = runtime.clone();
        let view = Rc::downgrade(view);
        Rc::new(move || {
            let Some(view) = view.upgrade() else {
                return false;
            };
            let metered = gio::NetworkMonitor::default().is_network_metered();
            let status = decision_inputs(&conn, &db_path);
            if runtime.automatic_refresh_allowed(status.count, metered, status.due) {
                view.request_refresh(false);
            }
            true
        })
    };

    {
        let trigger = trigger.clone();
        glib::idle_add_local_once(move || {
            trigger();
        });
    }
    glib::timeout_add_seconds_local(REFRESH_TIMER_SECONDS, move || {
        if trigger() {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}

fn decision_inputs(db: &Db, db_path: &Path) -> ScopeStatus {
    let subscriptions = match reprise_core::podcasts::store::active_subscriptions(db) {
        Ok(subscriptions) => subscriptions,
        Err(error) => {
            tracing::warn!(%error, "could not inspect podcast refresh schedule");
            return ScopeStatus::default();
        }
    };
    let config = match reprise_core::podcasts::config::load(db) {
        Ok(config) => config,
        Err(error) => {
            tracing::warn!(%error, "could not read podcast refresh interval");
            return ScopeStatus {
                count: subscriptions.len(),
                due: false,
            };
        }
    };
    let jitter = reprise_core::podcasts::refresh::jitter_seconds(&db_path.to_string_lossy());
    scope_status(
        &subscriptions,
        None,
        RefreshWindow::Hours {
            refresh_hours: config.refresh_hours,
            jitter_seconds: jitter,
        },
        chrono::Utc::now().timestamp(),
    )
}
