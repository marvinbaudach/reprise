//! The "My Stats" screen: a scrollable page showing headline listening
//! totals, top artists/albums/tracks, and a 12-month activity chart.
//!
//! Data comes from `reprise_core::library::stats_screen` (read-only queries
//! against the existing `tracks` and `listen_events` tables — no migration,
//! no query changes).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk4::prelude::*;
use rusqlite::Connection;

use reprise_core::format::format_thousands;
use reprise_core::library::stats_screen;

use super::stats_chart::StatsChart;

const TOP_LIMIT: usize = 5;
const CONTENT_MAX_WIDTH: i32 = 680;
const SECTION_SPACING: i32 = 28;
const LIST_SPACING: i32 = 6;

pub(in crate::ui) struct StatsView {
    root: gtk4::ScrolledWindow,
    chart: StatsChart,
    headline_hours: gtk4::Label,
    headline_subtitle: gtk4::Label,
    top_artists_box: gtk4::Box,
    top_albums_box: gtk4::Box,
    top_tracks_box: gtk4::Box,
}

impl StatsView {
    pub(in crate::ui) fn new() -> Self {
        let chart = StatsChart::new();

        let headline_hours = gtk4::Label::new(None);
        headline_hours.add_css_class("stats-headline-hours");
        headline_hours.set_xalign(0.0);

        let headline_subtitle = gtk4::Label::new(None);
        headline_subtitle.add_css_class("stats-headline-subtitle");
        headline_subtitle.set_xalign(0.0);

        let headline_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        headline_box.append(&headline_hours);
        headline_box.append(&headline_subtitle);
        headline_box.set_margin_bottom(8);

        let top_artists_box = gtk4::Box::new(gtk4::Orientation::Vertical, LIST_SPACING);
        let top_albums_box = gtk4::Box::new(gtk4::Orientation::Vertical, LIST_SPACING);
        let top_tracks_box = gtk4::Box::new(gtk4::Orientation::Vertical, LIST_SPACING);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_SPACING);
        content.set_margin_top(32);
        content.set_margin_bottom(32);
        content.set_margin_start(24);
        content.set_margin_end(24);
        content.set_halign(gtk4::Align::Fill);
        content.set_valign(gtk4::Align::Start);
        content.set_hexpand(true);

        content.append(&headline_box);

        let chart_card = card_wrapper(chart.widget().upcast_ref());
        content.append(&chart_card);

        content.append(&section("TOP ARTISTS", &top_artists_box));
        content.append(&section("TOP ALBUMS", &top_albums_box));
        content.append(&section("TOP TRACKS", &top_tracks_box));

        let clamp = adw_clamp(&content, CONTENT_MAX_WIDTH);

        let root = gtk4::ScrolledWindow::builder()
            .child(&clamp)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .build();

        Self {
            root,
            chart,
            headline_hours,
            headline_subtitle,
            top_artists_box,
            top_albums_box,
            top_tracks_box,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    /// Fetches all stats from the database and updates every section.
    pub(in crate::ui) fn refresh(&self, conn: &Rc<RefCell<Connection>>) {
        let conn = conn.borrow();
        self.refresh_headline(&conn);
        self.refresh_top_artists(&conn);
        self.refresh_top_albums(&conn);
        self.refresh_top_tracks(&conn);
        self.refresh_chart(&conn);
    }

    fn refresh_headline(&self, conn: &Connection) {
        match stats_screen::headline_totals(conn) {
            Ok(totals) => {
                let hours = super::stats_chart_math::ms_to_hours(totals.total_ms);
                self.headline_hours
                    .set_text(&format!("{} hours", format_thousands(hours)));
                self.headline_subtitle.set_text(&format!(
                    "{} plays · all time",
                    format_thousands(totals.total_plays)
                ));
            }
            Err(error) => {
                tracing::error!(%error, "stats: failed to load headline totals");
                self.headline_hours.set_text("— hours");
                self.headline_subtitle.set_text("—");
            }
        }
    }

    fn refresh_top_artists(&self, conn: &Connection) {
        clear_box(&self.top_artists_box);
        match stats_screen::top_artists(conn, TOP_LIMIT) {
            Ok(artists) => {
                if artists.is_empty() {
                    self.top_artists_box.append(&empty_label());
                    return;
                }
                for (i, artist) in artists.iter().enumerate() {
                    let row = list_row(
                        i + 1,
                        &artist.artist,
                        None,
                        &format!("{} plays", format_thousands(artist.plays)),
                    );
                    self.top_artists_box.append(&row);
                }
            }
            Err(error) => {
                tracing::error!(%error, "stats: failed to load top artists");
            }
        }
    }

    fn refresh_top_albums(&self, conn: &Connection) {
        clear_box(&self.top_albums_box);
        match stats_screen::top_albums(conn, TOP_LIMIT) {
            Ok(albums) => {
                if albums.is_empty() {
                    self.top_albums_box.append(&empty_label());
                    return;
                }
                for (i, album) in albums.iter().enumerate() {
                    let row = list_row(
                        i + 1,
                        &album.album,
                        Some(&album.album_artist),
                        &format!("{} plays", format_thousands(album.plays)),
                    );
                    self.top_albums_box.append(&row);
                }
            }
            Err(error) => {
                tracing::error!(%error, "stats: failed to load top albums");
            }
        }
    }

    fn refresh_top_tracks(&self, conn: &Connection) {
        clear_box(&self.top_tracks_box);
        match stats_screen::top_tracks(conn, TOP_LIMIT) {
            Ok(tracks) => {
                if tracks.is_empty() {
                    self.top_tracks_box.append(&empty_label());
                    return;
                }
                for (i, track) in tracks.iter().enumerate() {
                    let row = list_row(
                        i + 1,
                        &track.title,
                        Some(&track.artist),
                        &format!("{} plays", format_thousands(track.play_count)),
                    );
                    self.top_tracks_box.append(&row);
                }
            }
            Err(error) => {
                tracing::error!(%error, "stats: failed to load top tracks");
            }
        }
    }

    fn refresh_chart(&self, conn: &Connection) {
        let now_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        match stats_screen::monthly_listen_timeseries(conn, now_unix) {
            Ok(series) => {
                let labels: Vec<String> = series.iter().map(|b| b.year_month.clone()).collect();
                let values: Vec<i64> = series.iter().map(|b| b.total_ms).collect();
                self.chart.set_data(&labels, &values);
            }
            Err(error) => {
                tracing::error!(%error, "stats: failed to load monthly timeseries");
            }
        }
    }
}

/// Wraps a child in an `adw::Clamp` for max-width centering.
fn adw_clamp(child: &impl IsA<gtk4::Widget>, max_width: i32) -> libadwaita::Clamp {
    let clamp = libadwaita::Clamp::new();
    clamp.set_maximum_size(max_width);
    clamp.set_child(Some(child));
    clamp
}

/// Builds a section: a title label above the content box.
fn section(title: &str, content: &gtk4::Box) -> gtk4::Box {
    let title_label = gtk4::Label::new(Some(title));
    title_label.add_css_class("stats-section-title");
    title_label.set_xalign(0.0);
    title_label.set_margin_bottom(8);

    let wrapper = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    wrapper.append(&title_label);
    wrapper.append(content);
    wrapper
}

/// Wraps a widget in a card-style container using `.stats-card`.
fn card_wrapper(child: &gtk4::Widget) -> gtk4::Box {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    card.add_css_class("stats-card");
    card.append(child);
    card
}

/// Builds one row in a top-N list: rank · title (+ optional subtitle) · play count.
fn list_row(rank: usize, title: &str, subtitle: Option<&str>, count_text: &str) -> gtk4::Box {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    hbox.set_margin_top(4);
    hbox.set_margin_bottom(4);
    hbox.set_valign(gtk4::Align::Center);

    let rank_label = gtk4::Label::new(Some(&rank.to_string()));
    rank_label.add_css_class("stats-rank");
    rank_label.set_xalign(1.0);
    hbox.append(&rank_label);

    let text_box = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
    text_box.set_hexpand(true);

    let title_label = gtk4::Label::new(Some(title));
    title_label.add_css_class("stats-item-title");
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    text_box.append(&title_label);

    if let Some(sub) = subtitle {
        let sub_label = gtk4::Label::new(Some(sub));
        sub_label.add_css_class("stats-item-subtitle");
        sub_label.set_xalign(0.0);
        sub_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        text_box.append(&sub_label);
    }

    hbox.append(&text_box);

    let count_label = gtk4::Label::new(Some(count_text));
    count_label.add_css_class("stats-play-count");
    count_label.set_valign(gtk4::Align::Center);
    hbox.append(&count_label);

    hbox
}

fn clear_box(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn empty_label() -> gtk4::Label {
    let label = gtk4::Label::new(Some("No listening data yet"));
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    label
}
