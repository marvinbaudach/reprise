//! YouTube block rows for the Online sources page (`SET-8`).
//!
//! YouTube is a peer of Podcasts and Radio (issue #96): its own module
//! (`reprise_core::modules::YOUTUBE_MODULE`), its own rows here, no longer a
//! switch buried inside the Podcasts block.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::podcasts::config;
use rusqlite::Connection;

use crate::ui::{one_shot_task, strings};

#[derive(Clone, Debug, PartialEq, Eq)]
struct YtDlpDisplayState {
    subtitle: String,
    update_sensitive: bool,
}

fn ytdlp_display_state(probe: Result<String, String>) -> YtDlpDisplayState {
    match probe {
        Ok(version) => YtDlpDisplayState {
            subtitle: version,
            update_sensitive: true,
        },
        Err(error) => YtDlpDisplayState {
            subtitle: if error.to_ascii_lowercase().contains("not installed") {
                strings::text(strings::PODCAST_YTDLP_MISSING)
            } else {
                error
            },
            update_sensitive: false,
        },
    }
}

struct YoutubePreferenceRowsInner {
    rows: Vec<gtk4::Widget>,
}

#[derive(Clone)]
pub(in crate::ui) struct YoutubePreferenceRows {
    inner: Rc<YoutubePreferenceRowsInner>,
}

impl YoutubePreferenceRows {
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

pub(in crate::ui) fn build(conn: &Rc<RefCell<Connection>>, enabled: bool) -> YoutubePreferenceRows {
    let cfg = config::load(&conn.borrow()).unwrap_or(config::PodcastConfig {
        import_count: config::DEFAULT_IMPORT_COUNT,
        auto_download_default: false,
        cleanup_policy: config::CleanupPolicy::KeepAll,
        youtube_import_count: config::DEFAULT_YOUTUBE_IMPORT_COUNT,
        youtube_hide_shorts_default: true,
        ytdlp_path: None,
        refresh_hours: config::DEFAULT_REFRESH_HOURS,
        latest_per_channel_default: config::DEFAULT_LATEST_PER_CHANNEL,
    });

    let episode_count = adw::SpinRow::with_range(3.0, 50.0, 1.0);
    episode_count.set_title(&strings::text(strings::YOUTUBE_EPISODES_PER_CHANNEL));
    episode_count.set_value(cfg.youtube_import_count as f64);
    {
        let conn = conn.clone();
        episode_count.connect_value_notify(move |row| {
            save_or_warn(save_youtube_import_count(
                &conn.borrow(),
                row.value().round() as usize,
            ));
        });
    }

    let hide_shorts = adw::SwitchRow::builder()
        .title(strings::text(strings::YOUTUBE_HIDE_SHORTS))
        .active(cfg.youtube_hide_shorts_default)
        .build();
    {
        let conn = conn.clone();
        hide_shorts.connect_active_notify(move |row| {
            save_or_warn(save_hide_shorts_default(&conn.borrow(), row.is_active()));
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

    probe_ytdlp(&ytdlp, &update, cfg.ytdlp_path.as_deref());
    wire_update(&ytdlp, &update, cfg.ytdlp_path.as_deref());

    let rows = YoutubePreferenceRows {
        inner: Rc::new(YoutubePreferenceRowsInner {
            rows: vec![episode_count.upcast(), hide_shorts.upcast(), ytdlp.upcast()],
        }),
    };
    rows.set_sensitive(enabled);
    rows
}

fn probe_ytdlp(row: &adw::ActionRow, update: &gtk4::Button, setting_path: Option<&str>) {
    let path = setting_path.map(str::to_owned);
    let receiver = one_shot_task::spawn("reprise-ytdlp-probe", move || {
        reprise_core::podcasts::ytdlp::YtDlp::discover(path.as_deref())
            .probe_version()
            .map_err(|error| error.to_string())
    });
    receive_ytdlp(receiver, row.clone(), update.clone());
}

fn wire_update(row: &adw::ActionRow, update: &gtk4::Button, setting_path: Option<&str>) {
    let path = setting_path.map(str::to_owned);
    let row = row.clone();
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
        let pending = pending.clone();
        gtk4::glib::spawn_future_local(async move {
            let result = receive_result(receiver).await;
            pending.set(false);
            apply_ytdlp_state(&row, &button, result);
        });
    });
}

fn receive_ytdlp(
    receiver: std::io::Result<async_channel::Receiver<Result<String, String>>>,
    row: adw::ActionRow,
    update: gtk4::Button,
) {
    gtk4::glib::spawn_future_local(async move {
        apply_ytdlp_state(&row, &update, receive_result(receiver).await);
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

fn apply_ytdlp_state(row: &adw::ActionRow, update: &gtk4::Button, result: Result<String, String>) {
    let state = ytdlp_display_state(result);
    row.set_subtitle(&state.subtitle);
    update.set_sensitive(state.update_sensitive);
}

fn save_youtube_import_count(conn: &Connection, value: usize) -> Result<(), rusqlite::Error> {
    reprise_core::library::settings::set_setting(
        conn,
        config::YOUTUBE_IMPORT_COUNT_KEY,
        &value.to_string(),
    )
}

fn save_hide_shorts_default(conn: &Connection, value: bool) -> Result<(), rusqlite::Error> {
    reprise_core::library::settings::set_bool(conn, config::YOUTUBE_HIDE_SHORTS_DEFAULT_KEY, value)
}

fn save_or_warn(result: Result<(), rusqlite::Error>) {
    if let Err(error) = result {
        tracing::warn!(%error, "could not save YouTube preference");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ytdlp_probe_state_reflects_the_probe_result() {
        let available = ytdlp_display_state(Ok("2026.07.26".to_owned()));
        assert!(available.update_sensitive);
        assert!(available.subtitle.contains("2026.07.26"));

        let missing = ytdlp_display_state(Err(
            "YouTube component is unavailable — reinstall or repair Reprise".to_owned(),
        ));
        assert!(!missing.update_sensitive);
        assert!(missing.subtitle.contains("repair Reprise"));
    }

    #[test]
    fn youtube_preference_values_round_trip_through_core_config() {
        let conn = reprise_core::db::open_migrated(None).unwrap();
        save_youtube_import_count(&conn, 20).unwrap();
        save_hide_shorts_default(&conn, false).unwrap();

        let cfg = reprise_core::podcasts::config::load(&conn).unwrap();
        assert_eq!(cfg.youtube_import_count, 20);
        assert!(!cfg.youtube_hide_shorts_default);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn youtube_preference_rows_build_with_every_control() {
        gtk4::init().unwrap();
        let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
        let rows = build(&conn, true);
        assert_eq!(rows.inner.rows.len(), 3);
    }
}
