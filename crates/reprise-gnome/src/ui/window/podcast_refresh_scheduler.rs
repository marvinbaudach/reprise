//! Window-level scheduling for automatic podcast refreshes.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use rusqlite::Connection;

const REFRESH_TIMER_SECONDS: u32 = 60 * 60;

pub(in crate::ui) fn arm(
    conn: &Rc<RefCell<Connection>>,
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
            let (subscription_count, due) = decision_inputs(&conn.borrow(), &db_path);
            if runtime.automatic_refresh_allowed(subscription_count, metered, due) {
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

fn decision_inputs(conn: &Connection, db_path: &Path) -> (usize, bool) {
    let subscriptions = match reprise_core::podcasts::store::active_subscriptions(conn) {
        Ok(subscriptions) => subscriptions,
        Err(error) => {
            tracing::warn!(%error, "could not inspect podcast refresh schedule");
            return (0, false);
        }
    };
    let config = match reprise_core::podcasts::config::load(conn) {
        Ok(config) => config,
        Err(error) => {
            tracing::warn!(%error, "could not read podcast refresh interval");
            return (subscriptions.len(), false);
        }
    };
    let now = chrono::Utc::now().timestamp();
    let seed = db_path.to_string_lossy();
    let jitter = reprise_core::podcasts::refresh::jitter_seconds(&seed);
    let due = subscriptions.iter().any(|subscription| {
        reprise_core::podcasts::refresh::refresh_due_with_hours(
            subscription.last_fetch_at,
            now,
            config.refresh_hours,
            jitter,
        )
    });
    (subscriptions.len(), due)
}
