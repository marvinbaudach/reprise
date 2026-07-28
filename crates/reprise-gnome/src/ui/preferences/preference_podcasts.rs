//! Podcast plugin preferences.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::podcasts::config::{self, CleanupPolicy};
use rusqlite::Connection;

use crate::ui::{one_shot_task, strings};

#[derive(Clone, Debug, PartialEq, Eq)]
struct YtDlpDisplayState {
    youtube_enabled: bool,
    subtitle: String,
    update_sensitive: bool,
}

fn ytdlp_display_state(youtube_enabled: bool, probe: Result<String, String>) -> YtDlpDisplayState {
    match probe {
        Ok(version) => YtDlpDisplayState {
            youtube_enabled,
            subtitle: version,
            update_sensitive: true,
        },
        Err(error) => YtDlpDisplayState {
            youtube_enabled,
            subtitle: if error.to_ascii_lowercase().contains("not installed") {
                strings::text(strings::PODCAST_YTDLP_MISSING)
            } else {
                error
            },
            update_sensitive: false,
        },
    }
}

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
        youtube_enabled: true,
        ytdlp_path: None,
        refresh_hours: config::DEFAULT_REFRESH_HOURS,
    });

    let import_count = adw::SpinRow::with_range(5.0, 100.0, 1.0);
    import_count.set_title(&strings::text(strings::PODCAST_PREFERENCES_IMPORT_COUNT));
    import_count.set_value(config.import_count as f64);
    {
        let conn = conn.clone();
        import_count.connect_value_notify(move |row| {
            save_or_warn(save_import_count(
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
            save_or_warn(save_auto_download(&conn.borrow(), row.is_active()));
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
            save_or_warn(save_cleanup(&conn.borrow(), cleanup_policy(row.selected())));
        });
    }

    let ytdlp = adw::ActionRow::builder()
        .title(strings::text(strings::PODCAST_YTDLP))
        .subtitle(strings::text(strings::PODCAST_YTDLP_CHECKING))
        .build();
    let update = gtk4::Button::builder()
        .label(strings::text(strings::PODCAST_YTDLP_UPDATE))
        .valign(gtk4::Align::Center)
        .sensitive(false)
        .build();
    ytdlp.add_suffix(&update);

    let youtube = adw::SwitchRow::builder()
        .title(strings::text(strings::PODCAST_YOUTUBE_SOURCES))
        .active(config.youtube_enabled)
        .build();
    {
        let conn = conn.clone();
        youtube.connect_active_notify(move |row| {
            save_or_warn(save_youtube_enabled(&conn.borrow(), row.is_active()));
        });
    }

    let refresh = adw::SpinRow::with_range(1.0, 24.0, 1.0);
    refresh.set_title(&strings::text(strings::PODCAST_REFRESH_INTERVAL));
    refresh.set_value(config.refresh_hours as f64);
    {
        let conn = conn.clone();
        refresh.connect_value_notify(move |row| {
            save_or_warn(save_refresh_hours(
                &conn.borrow(),
                row.value().round() as i64,
            ));
        });
    }

    probe_ytdlp(&ytdlp, &update, &youtube, config.ytdlp_path.as_deref());
    wire_update(&ytdlp, &update, &youtube, config.ytdlp_path.as_deref());

    let rows = PodcastPreferenceRows {
        inner: Rc::new(PodcastPreferenceRowsInner {
            rows: vec![
                import_count.upcast(),
                auto_download.upcast(),
                cleanup.upcast(),
                ytdlp.upcast(),
                youtube.upcast(),
                refresh.upcast(),
            ],
        }),
    };
    rows.set_sensitive(enabled);
    rows
}

fn probe_ytdlp(
    row: &adw::ActionRow,
    update: &gtk4::Button,
    youtube: &adw::SwitchRow,
    setting_path: Option<&str>,
) {
    let path = setting_path.map(str::to_owned);
    let receiver = one_shot_task::spawn("reprise-ytdlp-probe", move || {
        reprise_core::podcasts::ytdlp::YtDlp::discover(path.as_deref())
            .probe_version()
            .map_err(|error| error.to_string())
    });
    receive_ytdlp(receiver, row.clone(), update.clone(), youtube.clone());
}

fn wire_update(
    row: &adw::ActionRow,
    update: &gtk4::Button,
    youtube: &adw::SwitchRow,
    setting_path: Option<&str>,
) {
    let path = setting_path.map(str::to_owned);
    let row = row.clone();
    let youtube = youtube.clone();
    let pending = Rc::new(Cell::new(false));
    update.connect_clicked(move |button| {
        if pending.replace(true) {
            return;
        }
        button.set_sensitive(false);
        let path = path.clone();
        let receiver = one_shot_task::spawn("reprise-ytdlp-update", move || {
            reprise_core::podcasts::ytdlp::YtDlp::discover(path.as_deref())
                .update()
                .map_err(|error| error.to_string())
        });
        let row = row.clone();
        let button = button.clone();
        let youtube = youtube.clone();
        let pending = pending.clone();
        gtk4::glib::spawn_future_local(async move {
            let result = receive_result(receiver).await;
            pending.set(false);
            apply_ytdlp_state(&row, &button, &youtube, result);
        });
    });
}

fn receive_ytdlp(
    receiver: std::io::Result<async_channel::Receiver<Result<String, String>>>,
    row: adw::ActionRow,
    update: gtk4::Button,
    youtube: adw::SwitchRow,
) {
    gtk4::glib::spawn_future_local(async move {
        apply_ytdlp_state(&row, &update, &youtube, receive_result(receiver).await);
    });
}

async fn receive_result(
    receiver: std::io::Result<async_channel::Receiver<Result<String, String>>>,
) -> Result<String, String> {
    match receiver {
        Ok(receiver) => receiver.recv().await.map_err(|error| error.to_string())?,
        Err(error) => Err(error.to_string()),
    }
}

fn apply_ytdlp_state(
    row: &adw::ActionRow,
    update: &gtk4::Button,
    youtube: &adw::SwitchRow,
    result: Result<String, String>,
) {
    let state = ytdlp_display_state(youtube.is_active(), result);
    row.set_subtitle(&state.subtitle);
    update.set_sensitive(state.update_sensitive);
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

fn save_import_count(conn: &Connection, value: usize) -> Result<(), rusqlite::Error> {
    reprise_core::library::settings::set_setting(conn, config::IMPORT_COUNT_KEY, &value.to_string())
}

fn save_auto_download(conn: &Connection, value: bool) -> Result<(), rusqlite::Error> {
    reprise_core::library::settings::set_bool(conn, config::AUTO_DOWNLOAD_DEFAULT_KEY, value)
}

fn save_cleanup(conn: &Connection, value: CleanupPolicy) -> Result<(), rusqlite::Error> {
    reprise_core::library::settings::set_setting(
        conn,
        config::CLEANUP_POLICY_KEY,
        value.as_setting(),
    )
}

fn save_youtube_enabled(conn: &Connection, value: bool) -> Result<(), rusqlite::Error> {
    reprise_core::library::settings::set_bool(conn, config::YOUTUBE_ENABLED_KEY, value)
}

fn save_refresh_hours(conn: &Connection, value: i64) -> Result<(), rusqlite::Error> {
    reprise_core::library::settings::set_setting(
        conn,
        config::REFRESH_HOURS_KEY,
        &value.to_string(),
    )
}

fn save_or_warn(result: Result<(), rusqlite::Error>) {
    if let Err(error) = result {
        tracing::warn!(%error, "could not save podcast preference");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ytdlp_probe_state_never_changes_the_youtube_setting() {
        let available = ytdlp_display_state(true, Ok("2026.07.26".to_owned()));
        assert!(available.youtube_enabled);
        assert!(available.update_sensitive);
        assert!(available.subtitle.contains("2026.07.26"));

        let missing = ytdlp_display_state(
            true,
            Err("YouTube component is unavailable — reinstall or repair Reprise".to_owned()),
        );
        assert!(missing.youtube_enabled);
        assert!(!missing.update_sensitive);
        assert!(missing.subtitle.contains("repair Reprise"));

        let disabled = ytdlp_display_state(false, Err("missing".to_owned()));
        assert!(!disabled.youtube_enabled);
    }

    #[test]
    fn podcast_preference_values_round_trip_through_core_config() {
        let conn = reprise_core::db::open_migrated(None).unwrap();
        save_import_count(&conn, 42).unwrap();
        save_auto_download(&conn, true).unwrap();
        save_cleanup(
            &conn,
            reprise_core::podcasts::config::CleanupPolicy::KeepLast5,
        )
        .unwrap();
        save_youtube_enabled(&conn, false).unwrap();
        save_refresh_hours(&conn, 12).unwrap();

        let config = reprise_core::podcasts::config::load(&conn).unwrap();
        assert_eq!(config.import_count, 42);
        assert!(config.auto_download_default);
        assert_eq!(
            config.cleanup_policy,
            reprise_core::podcasts::config::CleanupPolicy::KeepLast5
        );
        assert!(!config.youtube_enabled);
        assert_eq!(config.refresh_hours, 12);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn podcast_preference_rows_build_with_every_source_control() {
        gtk4::init().unwrap();
        let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
        let rows = build(&conn, true);
        assert_eq!(rows.inner.rows.len(), 6);
    }
}
