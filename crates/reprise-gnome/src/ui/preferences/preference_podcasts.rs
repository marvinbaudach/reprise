//! Podcasts child rows for the Plugins-page expander (`SET-10`).
//!
//! YouTube's rows live in the sibling `preference_youtube` module — YouTube
//! is a peer source with its own module (issue #96), not a Podcasts
//! sub-setting.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::podcasts::config::{self, CleanupPolicy};

use crate::ui::strings;

const IMPORT_ALL_EPISODES: &str = "Import all episodes";
const IMPORT_COUNT_MIN_VISIBLE: f64 = 5.0;

struct PodcastPreferenceRowsInner {
    rows: Vec<gtk4::Widget>,
    import_all: adw::SwitchRow,
    import_count: adw::SpinRow,
}

#[derive(Clone)]
pub(in crate::ui) struct PodcastPreferenceRows {
    inner: Rc<PodcastPreferenceRowsInner>,
}

impl PodcastPreferenceRows {
    pub(in crate::ui) fn add_to(&self, expander: &adw::ExpanderRow) {
        for row in &self.inner.rows {
            expander.add_row(row);
        }
    }

    pub(in crate::ui) fn set_sensitive(&self, enabled: bool) {
        for row in &self.inner.rows {
            row.set_sensitive(enabled);
        }
        self.inner
            .import_count
            .set_sensitive(enabled && !self.inner.import_all.is_active());
    }
}

fn import_count_control_values(saved: usize) -> (bool, usize) {
    if saved == 0 {
        (true, config::DEFAULT_IMPORT_COUNT)
    } else {
        (
            false,
            saved.clamp(
                IMPORT_COUNT_MIN_VISIBLE as usize,
                config::IMPORT_COUNT_MAX as usize,
            ),
        )
    }
}

pub(in crate::ui) fn build(conn: &Rc<Db>, enabled: bool) -> PodcastPreferenceRows {
    let config = config::load(conn).unwrap_or(config::PodcastConfig {
        import_count: config::DEFAULT_IMPORT_COUNT,
        auto_download_default: false,
        cleanup_policy: CleanupPolicy::KeepAll,
        youtube_import_count: config::DEFAULT_YOUTUBE_IMPORT_COUNT,
        youtube_hide_shorts_default: true,
        youtube_browser: None,
        ytdlp_path: None,
        refresh_hours: config::DEFAULT_REFRESH_HOURS,
        latest_per_channel_default: config::DEFAULT_LATEST_PER_CHANNEL,
        keep_downloaded_default: config::DEFAULT_KEEP_DOWNLOADED,
    });

    let (imports_all, visible_import_count) = import_count_control_values(config.import_count);
    let import_all = adw::SwitchRow::builder()
        .title(strings::text(IMPORT_ALL_EPISODES))
        .active(imports_all)
        .build();
    let import_count = adw::SpinRow::with_range(
        IMPORT_COUNT_MIN_VISIBLE,
        config::IMPORT_COUNT_MAX as f64,
        1.0,
    );
    import_count.set_title(&strings::text(strings::PODCAST_EPISODES_PER_SHOW));
    import_count.set_value(visible_import_count as f64);
    {
        let conn = conn.clone();
        import_count.connect_value_notify(move |row| {
            if row.is_sensitive() {
                save_or_warn(config::set_import_count(
                    &conn,
                    row.value().round() as usize,
                ));
            }
        });
    }
    {
        let conn = conn.clone();
        let import_count = import_count.clone();
        import_all.connect_active_notify(move |row| {
            let imports_all = row.is_active();
            import_count.set_sensitive(row.is_sensitive() && !imports_all);
            let value = if imports_all {
                0
            } else {
                import_count.value().round() as usize
            };
            save_or_warn(config::set_import_count(&conn, value));
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
                &conn,
                cleanup_policy(row.selected()),
            ));
        });
    }

    let rows = PodcastPreferenceRows {
        inner: Rc::new(PodcastPreferenceRowsInner {
            rows: vec![
                import_all.clone().upcast(),
                import_count.clone().upcast(),
                cleanup.upcast(),
            ],
            import_all,
            import_count,
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

    #[test]
    fn unlimited_import_uses_a_switch_without_rendering_zero_in_the_spin_row() {
        assert_eq!(import_count_control_values(0), (true, 25));
        assert_eq!(import_count_control_values(42), (false, 42));
    }

    // Kept as an integration check that this page writes the settings it
    // reads back. The clamping and key spelling are core's to prove, and are
    // covered by `podcasts::config`'s own tests.
    #[test]
    fn podcast_preference_values_round_trip_through_core_config() {
        let conn = crate::test_db::open().unwrap();
        config::set_import_count(&conn, 42).unwrap();
        config::set_cleanup_policy(&conn, CleanupPolicy::KeepLast5).unwrap();

        let config = reprise_core::podcasts::config::load(&conn).unwrap();
        assert_eq!(config.import_count, 42);
        assert_eq!(
            config.cleanup_policy,
            reprise_core::podcasts::config::CleanupPolicy::KeepLast5
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn podcast_preference_rows_build_with_every_source_control() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let rows = build(&conn, true);
        assert_eq!(rows.inner.rows.len(), 3);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn unlimited_import_switch_disables_a_nonzero_count_and_restores_it_when_switched_off() {
        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        config::set_import_count(&conn, 42).unwrap();

        let rows = build(&conn, true);
        let import_all = rows.inner.rows[0].downcast_ref::<adw::SwitchRow>().unwrap();
        let import_count = rows.inner.rows[1].downcast_ref::<adw::SpinRow>().unwrap();

        assert!(!import_all.is_active());
        assert!(import_count.is_sensitive());
        assert_eq!(import_count.value(), 42.0);

        import_all.set_active(true);
        assert!(import_all.is_active());
        assert!(!import_count.is_sensitive());
        assert_eq!(config::load(&conn).unwrap().import_count, 0);

        import_all.set_active(false);
        assert!(import_count.is_sensitive());
        assert_eq!(import_count.value(), 42.0);
        assert_eq!(config::load(&conn).unwrap().import_count, 42);

        config::set_import_count(&conn, 0).unwrap();
        let reloaded = build(&conn, true);
        let reloaded_count = reloaded.inner.rows[1]
            .downcast_ref::<adw::SpinRow>()
            .unwrap();
        assert_eq!(reloaded_count.value(), config::DEFAULT_IMPORT_COUNT as f64);
    }
}
