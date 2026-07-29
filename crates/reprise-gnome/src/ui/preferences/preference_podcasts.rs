//! Podcasts block rows for the Online sources page (`SET-8`).
//!
//! YouTube's rows live in the sibling `preference_youtube` module — YouTube
//! is a peer source with its own module (issue #96), not a Podcasts
//! sub-setting.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::podcasts::config::{self, CleanupPolicy};
use rusqlite::Connection;

use crate::ui::strings;

struct PodcastPreferenceRowsInner {
    rows: Vec<gtk4::Widget>,
}

#[derive(Clone)]
pub(in crate::ui) struct PodcastPreferenceRows {
    inner: Rc<PodcastPreferenceRowsInner>,
}

impl PodcastPreferenceRows {
    pub(in crate::ui) fn add_to(&self, group: &adw::PreferencesGroup) {
        for row in &self.inner.rows {
            group.add(row);
        }
    }

    pub(in crate::ui) fn set_sensitive(&self, enabled: bool) {
        for row in &self.inner.rows {
            row.set_sensitive(enabled);
        }
    }
}

pub(in crate::ui) fn build(conn: &Rc<RefCell<Connection>>, enabled: bool) -> PodcastPreferenceRows {
    let config = config::load(&conn.borrow()).unwrap_or(config::PodcastConfig {
        import_count: config::DEFAULT_IMPORT_COUNT,
        auto_download_default: false,
        cleanup_policy: CleanupPolicy::KeepAll,
        youtube_import_count: config::DEFAULT_YOUTUBE_IMPORT_COUNT,
        youtube_hide_shorts_default: true,
        ytdlp_path: None,
        refresh_hours: config::DEFAULT_REFRESH_HOURS,
        latest_per_channel_default: config::DEFAULT_LATEST_PER_CHANNEL,
        keep_downloaded_default: config::DEFAULT_KEEP_DOWNLOADED,
    });

    let import_count = adw::SpinRow::with_range(5.0, 100.0, 1.0);
    import_count.set_title(&strings::text(strings::PODCAST_EPISODES_PER_SHOW));
    import_count.set_value(config.import_count as f64);
    {
        let conn = conn.clone();
        import_count.connect_value_notify(move |row| {
            save_or_warn(config::set_import_count(
                &conn.borrow(),
                row.value().round() as usize,
            ));
        });
    }

    let auto_download = adw::SwitchRow::builder()
        .title(strings::text(strings::PODCAST_PREFERENCES_AUTO_DOWNLOAD))
        .active(config.auto_download_default)
        .build();
    {
        let conn = conn.clone();
        auto_download.connect_active_notify(move |row| {
            save_or_warn(config::set_auto_download_default(
                &conn.borrow(),
                row.is_active(),
            ));
        });
    }

    let cleanup_model = gtk4::StringList::new(&[
        &strings::text(strings::PODCAST_CLEANUP_KEEP_ALL),
        &strings::text(strings::PODCAST_CLEANUP_DELETE_PLAYED),
        &strings::text(strings::PODCAST_CLEANUP_KEEP_LAST),
    ]);
    let cleanup = adw::ComboRow::builder()
        .title(strings::text(strings::PODCAST_PREFERENCES_CLEANUP))
        .model(&cleanup_model)
        .selected(cleanup_index(config.cleanup_policy))
        .build();
    {
        let conn = conn.clone();
        cleanup.connect_selected_notify(move |row| {
            save_or_warn(config::set_cleanup_policy(
                &conn.borrow(),
                cleanup_policy(row.selected()),
            ));
        });
    }

    let rows = PodcastPreferenceRows {
        inner: Rc::new(PodcastPreferenceRowsInner {
            rows: vec![
                import_count.upcast(),
                auto_download.upcast(),
                cleanup.upcast(),
            ],
        }),
    };
    rows.set_sensitive(enabled);
    rows
}

fn cleanup_index(policy: CleanupPolicy) -> u32 {
    match policy {
        CleanupPolicy::KeepAll => 0,
        CleanupPolicy::DeletePlayedAfter7Days => 1,
        CleanupPolicy::KeepLast5 => 2,
    }
}

fn cleanup_policy(index: u32) -> CleanupPolicy {
    match index {
        1 => CleanupPolicy::DeletePlayedAfter7Days,
        2 => CleanupPolicy::KeepLast5,
        _ => CleanupPolicy::KeepAll,
    }
}

/// Generic over the error so this page never has to name the database's error
/// type just to log that a write failed.
fn save_or_warn<E: std::fmt::Display>(result: Result<(), E>) {
    if let Err(error) = result {
        tracing::warn!(%error, "could not save podcast preference");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Kept as an integration check that this page writes the settings it
    // reads back. The clamping and key spelling are core's to prove, and are
    // covered by `podcasts::config`'s own tests.
    #[test]
    fn podcast_preference_values_round_trip_through_core_config() {
        let conn = reprise_core::db::open_migrated(None).unwrap();
        config::set_import_count(&conn, 42).unwrap();
        config::set_auto_download_default(&conn, true).unwrap();
        config::set_cleanup_policy(&conn, CleanupPolicy::KeepLast5).unwrap();

        let config = reprise_core::podcasts::config::load(&conn).unwrap();
        assert_eq!(config.import_count, 42);
        assert!(config.auto_download_default);
        assert_eq!(
            config.cleanup_policy,
            reprise_core::podcasts::config::CleanupPolicy::KeepLast5
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn podcast_preference_rows_build_with_every_source_control() {
        gtk4::init().unwrap();
        let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
        let rows = build(&conn, true);
        assert_eq!(rows.inner.rows.len(), 3);
    }
}
