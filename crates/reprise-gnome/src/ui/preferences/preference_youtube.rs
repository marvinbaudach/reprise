//! YouTube child rows for the Plugins-page expander (`SET-10`).
//!
//! YouTube is a peer of Podcasts and Radio (issue #96): its own module
//! (`reprise_core::modules::YOUTUBE_MODULE`), its own rows here, no longer a
//! switch buried inside the Podcasts block.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;
use reprise_core::podcasts::config;

use crate::ui::{one_shot_task, strings};

const YOUTUBE_SIGN_IN_URL: &str = "https://www.youtube.com/";

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

fn browser_index(browser: Option<config::YoutubeBrowser>) -> u32 {
    browser
        .and_then(|browser| {
            config::YoutubeBrowser::ALL
                .iter()
                .position(|candidate| *candidate == browser)
        })
        .and_then(|index| u32::try_from(index + 1).ok())
        .unwrap_or_default()
}

fn browser_from_index(index: u32) -> Option<config::YoutubeBrowser> {
    let index = usize::try_from(index.checked_sub(1)?).ok()?;
    config::YoutubeBrowser::ALL.get(index).copied()
}

fn browser_app_matches(
    browser: config::YoutubeBrowser,
    app_id: Option<&str>,
    executable: &str,
) -> bool {
    let identity = format!("{} {executable}", app_id.unwrap_or_default()).to_ascii_lowercase();
    let tokens: &[&str] = match browser {
        config::YoutubeBrowser::Brave => &["brave"],
        config::YoutubeBrowser::Firefox => &["firefox"],
        config::YoutubeBrowser::Chrome => &["google-chrome", "com.google.chrome"],
        config::YoutubeBrowser::Chromium => &["chromium"],
        config::YoutubeBrowser::Edge => &["microsoft-edge", "microsoft.edge"],
        config::YoutubeBrowser::Opera => &["opera"],
        config::YoutubeBrowser::Vivaldi => &["vivaldi"],
    };
    tokens.iter().any(|token| identity.contains(token))
}

fn selected_browser_app(browser: config::YoutubeBrowser) -> Option<gio::AppInfo> {
    gio::AppInfo::all_for_type("x-scheme-handler/https")
        .into_iter()
        .find(|app| {
            let id = app.id();
            browser_app_matches(browser, id.as_deref(), &app.executable().to_string_lossy())
        })
}

struct YoutubePreferenceRowsInner {
    rows: Vec<gtk4::Widget>,
}

#[derive(Clone)]
pub(in crate::ui) struct YoutubePreferenceRows {
    inner: Rc<YoutubePreferenceRowsInner>,
}

impl YoutubePreferenceRows {
    pub(in crate::ui) fn add_to(&self, expander: &adw::ExpanderRow) {
        for row in &self.inner.rows {
            super::preference_plugin_chrome::add_nested_row(expander, row);
        }
    }

    pub(in crate::ui) fn set_sensitive(&self, enabled: bool) {
        for row in &self.inner.rows {
            row.set_sensitive(enabled);
        }
    }
}

pub(in crate::ui) fn build(conn: &Rc<Db>, enabled: bool) -> YoutubePreferenceRows {
    let cfg = config::load(conn).unwrap_or(config::PodcastConfig {
        import_count: config::DEFAULT_IMPORT_COUNT,
        auto_download_default: false,
        cleanup_policy: config::CleanupPolicy::KeepAll,
        youtube_import_count: config::DEFAULT_YOUTUBE_IMPORT_COUNT,
        youtube_hide_shorts_default: true,
        youtube_browser: None,
        ytdlp_path: None,
        refresh_hours: config::DEFAULT_REFRESH_HOURS,
        latest_per_channel_default: config::DEFAULT_LATEST_PER_CHANNEL,
        keep_downloaded_default: config::DEFAULT_KEEP_DOWNLOADED,
    });

    let episode_count = adw::SpinRow::with_range(3.0, 50.0, 1.0);
    episode_count.set_title(&strings::text(strings::YOUTUBE_EPISODES_PER_CHANNEL));
    episode_count.set_value(cfg.youtube_import_count as f64);
    {
        let conn = conn.clone();
        episode_count.connect_value_notify(move |row| {
            save_or_warn(config::set_youtube_import_count(
                &conn,
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
            save_or_warn(config::set_youtube_hide_shorts_default(
                &conn,
                row.is_active(),
            ));
        });
    }

    let browser_labels = [
        strings::text(strings::YOUTUBE_BROWSER_NONE),
        "Brave".to_owned(),
        "Firefox".to_owned(),
        "Google Chrome".to_owned(),
        "Chromium".to_owned(),
        "Microsoft Edge".to_owned(),
        "Opera".to_owned(),
        "Vivaldi".to_owned(),
    ];
    let browser_label_refs: Vec<&str> = browser_labels.iter().map(String::as_str).collect();
    let browser_model = gtk4::StringList::new(&browser_label_refs);
    let browser = adw::ComboRow::builder()
        .title(strings::text(strings::YOUTUBE_BROWSER))
        .subtitle(strings::text(strings::YOUTUBE_BROWSER_DESCRIPTION))
        .model(&browser_model)
        .selected(browser_index(cfg.youtube_browser))
        .build();
    {
        let conn = conn.clone();
        browser.connect_selected_notify(move |row| {
            save_or_warn(config::set_youtube_browser(
                &conn,
                browser_from_index(row.selected()),
            ));
        });
    }

    let sign_in = adw::ActionRow::builder()
        .title(strings::text(strings::YOUTUBE_SIGN_IN))
        .subtitle(strings::text(strings::YOUTUBE_SIGN_IN_DESCRIPTION))
        .build();
    let open_youtube = gtk4::Button::builder()
        .label(strings::text(strings::YOUTUBE_OPEN_SIGN_IN))
        .valign(gtk4::Align::Center)
        .sensitive(cfg.youtube_browser.is_some())
        .build();
    {
        let browser = browser.clone();
        let sign_in = sign_in.clone();
        open_youtube.connect_clicked(move |_| {
            let launched = browser_from_index(browser.selected())
                .and_then(selected_browser_app)
                .is_some_and(|app| {
                    crate::ui::external_link::launch_in_app(
                        YOUTUBE_SIGN_IN_URL,
                        "YouTube sign-in",
                        &app,
                    )
                });
            if !launched {
                sign_in.set_subtitle(&strings::text(strings::YOUTUBE_BROWSER_OPEN_FAILED));
            }
        });
    }
    {
        let open_youtube = open_youtube.clone();
        let sign_in = sign_in.clone();
        browser.connect_selected_notify(move |row| {
            open_youtube.set_sensitive(browser_from_index(row.selected()).is_some());
            sign_in.set_subtitle(&strings::text(strings::YOUTUBE_SIGN_IN_DESCRIPTION));
        });
    }
    sign_in.add_suffix(&open_youtube);

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
            rows: vec![
                episode_count.upcast(),
                hide_shorts.upcast(),
                browser.upcast(),
                sign_in.upcast(),
                ytdlp.upcast(),
            ],
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

/// Generic over the error so this page never has to name the database's error
/// type just to log that a write failed.
fn save_or_warn<E: std::fmt::Display>(result: Result<(), E>) {
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

    // Kept as an integration check that this page writes the settings it
    // reads back. The clamping and key spelling are core's to prove, and are
    // covered by `podcasts::config`'s own tests.
    #[test]
    fn youtube_preference_values_round_trip_through_core_config() {
        let conn = crate::test_db::open().unwrap();
        config::set_youtube_import_count(&conn, 20).unwrap();
        config::set_youtube_hide_shorts_default(&conn, false).unwrap();
        config::set_youtube_browser(&conn, Some(config::YoutubeBrowser::Firefox)).unwrap();

        let cfg = reprise_core::podcasts::config::load(&conn).unwrap();
        assert_eq!(cfg.youtube_import_count, 20);
        assert!(!cfg.youtube_hide_shorts_default);
        assert_eq!(cfg.youtube_browser, Some(config::YoutubeBrowser::Firefox));
    }

    #[test]
    fn pod_22_browser_selector_round_trips_every_supported_choice() {
        assert_eq!(browser_from_index(0), None);
        for (offset, browser) in config::YoutubeBrowser::ALL.into_iter().enumerate() {
            let index = u32::try_from(offset + 1).unwrap();
            assert_eq!(browser_index(Some(browser)), index);
            assert_eq!(browser_from_index(index), Some(browser));
        }
        assert_eq!(browser_from_index(u32::MAX), None);
    }

    #[test]
    fn pod_22_selected_browser_launcher_matches_the_chosen_application() {
        use config::YoutubeBrowser::{Brave, Chrome, Chromium, Edge, Firefox, Opera, Vivaldi};

        assert!(browser_app_matches(
            Brave,
            Some("com.brave.Browser.desktop"),
            "/usr/bin/flatpak"
        ));
        assert!(browser_app_matches(Firefox, None, "/usr/bin/firefox"));
        assert!(browser_app_matches(
            Chrome,
            Some("google-chrome.desktop"),
            "/usr/bin/google-chrome-stable"
        ));
        assert!(browser_app_matches(
            Chromium,
            Some("org.chromium.Chromium.desktop"),
            "/usr/bin/flatpak"
        ));
        assert!(browser_app_matches(
            Edge,
            Some("microsoft-edge.desktop"),
            "/usr/bin/microsoft-edge-stable"
        ));
        assert!(browser_app_matches(
            Opera,
            Some("opera.desktop"),
            "/usr/bin/opera"
        ));
        assert!(browser_app_matches(
            Vivaldi,
            Some("vivaldi-stable.desktop"),
            "/usr/bin/vivaldi-stable"
        ));
        assert!(!browser_app_matches(
            Chrome,
            Some("org.chromium.Chromium.desktop"),
            "/usr/bin/chromium"
        ));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn pod_22_youtube_preference_rows_build_with_every_recovery_control() {
        fn find_button(widget: &gtk4::Widget) -> Option<gtk4::Button> {
            if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
                return Some(button);
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                if let Some(button) = find_button(&current) {
                    return Some(button);
                }
                child = current.next_sibling();
            }
            None
        }

        gtk4::init().unwrap();
        let conn = Rc::new(crate::test_db::open().unwrap());
        let rows = build(&conn, true);
        assert_eq!(rows.inner.rows.len(), 5);
        let browser = rows.inner.rows[2]
            .clone()
            .downcast::<adw::ComboRow>()
            .unwrap();
        assert_eq!(browser.title(), strings::text(strings::YOUTUBE_BROWSER));
        assert_eq!(browser.selected(), 0);
        assert_eq!(
            browser.model().as_ref().map(gtk4::gio::ListModel::n_items),
            Some(8)
        );

        let sign_in = rows.inner.rows[3]
            .clone()
            .downcast::<adw::ActionRow>()
            .unwrap();
        assert_eq!(sign_in.title(), strings::text(strings::YOUTUBE_SIGN_IN));
        let open_youtube = find_button(sign_in.upcast_ref()).expect("Open YouTube button");
        assert_eq!(
            open_youtube.label().as_deref(),
            Some(strings::text(strings::YOUTUBE_OPEN_SIGN_IN).as_str())
        );
        assert!(!open_youtube.is_sensitive());
        browser.set_selected(1);
        assert!(open_youtube.is_sensitive());
    }
}
