//! The "My Stats" screen: a card-based dashboard showing headline listening
//! totals, top artists/genres, a 24-hour activity chart, and a horizontal
//! top-albums strip.
//!
//! Data comes from `reprise_core::library::stats_screen` (read-only queries
//! against the existing `tracks` and `listen_events` tables).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk4::prelude::*;
use rusqlite::Connection;

use reprise_core::format::format_thousands;
use reprise_core::library::stats_screen;

use super::hourly_chart::HourlyChart;
use super::stats_chart_math::ms_to_hours;

const TOP_LIMIT: usize = 5;
const ALBUMS_LIMIT: usize = 10;
const CONTENT_MAX_WIDTH: i32 = 1100;
const SECTION_SPACING: i32 = 24;
const LIST_SPACING: i32 = 6;
const CARD_GAP: i32 = 16;

pub(in crate::ui) struct StatsView {
    root: gtk4::ScrolledWindow,
    // Headline widgets
    year_label: gtk4::Label,
    headline_hours: gtk4::Label,
    plays_card_value: gtk4::Label,
    artists_card_value: gtk4::Label,
    weekday_card_value: gtk4::Label,
    // Top artists
    top_artists_box: gtk4::Box,
    // Top genres
    top_genres_box: gtk4::Box,
    // Hourly chart
    hourly_chart: HourlyChart,
    // Top albums strip
    top_albums_box: gtk4::Box,
}

impl StatsView {
    pub(in crate::ui) fn new() -> Self {
        let hourly_chart = HourlyChart::new();

        // --- Year label ---
        let year_label = gtk4::Label::new(None);
        year_label.add_css_class("stats-year-label");
        year_label.set_xalign(0.0);

        // --- Headline hours ---
        let headline_hours = gtk4::Label::new(None);
        headline_hours.add_css_class("stats-headline-hours");
        headline_hours.set_xalign(0.0);
        headline_hours.set_wrap(true);

        let headline_left = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        headline_left.set_hexpand(true);
        headline_left.set_valign(gtk4::Align::Center);
        headline_left.append(&year_label);
        headline_left.append(&headline_hours);

        // --- Mini stat cards ---
        let (plays_card, plays_card_value) = mini_card("plays");
        let (artists_card, artists_card_value) = mini_card("new artists");
        let (weekday_card, weekday_card_value) = mini_card("most active day");

        let mini_cards_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        mini_cards_box.set_valign(gtk4::Align::Center);
        mini_cards_box.append(&plays_card);
        mini_cards_box.append(&artists_card);
        mini_cards_box.append(&weekday_card);

        // --- Headline row ---
        let headline_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 24);
        headline_row.set_margin_bottom(8);
        headline_row.append(&headline_left);
        headline_row.append(&mini_cards_box);

        // --- Top Artists card (left column) ---
        let top_artists_box = gtk4::Box::new(gtk4::Orientation::Vertical, LIST_SPACING);
        let artists_card = titled_card("Top Artists", top_artists_box.upcast_ref());
        artists_card.set_vexpand(true);

        // --- Top Genres card (right column, top) ---
        let top_genres_box = gtk4::Box::new(gtk4::Orientation::Vertical, LIST_SPACING);
        let genres_card = titled_card("Top Genres", top_genres_box.upcast_ref());

        // --- Listening by hour card (right column, bottom) ---
        let hourly_card = titled_card("Listening by hour", hourly_chart.widget().upcast_ref());

        // --- Two-column grid ---
        let right_column = gtk4::Box::new(gtk4::Orientation::Vertical, CARD_GAP);
        right_column.set_hexpand(true);
        right_column.append(&genres_card);
        right_column.append(&hourly_card);

        let columns = gtk4::Box::new(gtk4::Orientation::Horizontal, CARD_GAP);
        columns.set_homogeneous(true);
        columns.append(&artists_card);
        columns.append(&right_column);

        // --- Top Albums horizontal strip ---
        let top_albums_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        top_albums_box.add_css_class("stats-albums-strip");

        let albums_scroll = gtk4::ScrolledWindow::builder()
            .child(&top_albums_box)
            .hscrollbar_policy(gtk4::PolicyType::Automatic)
            .vscrollbar_policy(gtk4::PolicyType::Never)
            .build();

        let albums_section = titled_card("Top Albums", albums_scroll.upcast_ref());

        // --- Main content assembly ---
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_SPACING);
        content.set_margin_top(32);
        content.set_margin_bottom(32);
        content.set_margin_start(24);
        content.set_margin_end(24);
        content.set_halign(gtk4::Align::Fill);
        content.set_valign(gtk4::Align::Start);
        content.set_hexpand(true);

        content.append(&headline_row);
        content.append(&columns);
        content.append(&albums_section);

        let clamp = adw_clamp(&content, CONTENT_MAX_WIDTH);

        let root = gtk4::ScrolledWindow::builder()
            .child(&clamp)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .hexpand(true)
            .build();

        Self {
            root,
            year_label,
            headline_hours,
            plays_card_value,
            artists_card_value,
            weekday_card_value,
            top_artists_box,
            top_genres_box,
            hourly_chart,
            top_albums_box,
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
        self.refresh_top_genres(&conn);
        self.refresh_hourly_chart(&conn);
        self.refresh_top_albums(&conn);
    }

    fn refresh_headline(&self, conn: &Connection) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(2026, |d| {
                // Approximate year from unix seconds.
                1970 + d.as_secs() / 31_557_600
            });
        self.year_label.set_text(&format!("{now} SO FAR"));

        match stats_screen::headline_totals(conn) {
            Ok(totals) => {
                let hours = ms_to_hours(totals.total_ms);
                self.headline_hours
                    .set_text(&format!("{} hours of listening", format_thousands(hours)));
                self.plays_card_value
                    .set_text(&format_thousands(totals.total_plays));
            }
            Err(error) => {
                tracing::error!(%error, "stats: failed to load headline totals");
                self.headline_hours.set_text("-- hours of listening");
                self.plays_card_value.set_text("--");
            }
        }

        match stats_screen::distinct_artists_played(conn) {
            Ok(count) => self.artists_card_value.set_text(&format_thousands(count)),
            Err(error) => {
                tracing::error!(%error, "stats: failed to load distinct artists");
                self.artists_card_value.set_text("--");
            }
        }

        match stats_screen::most_active_weekday(conn) {
            Ok(Some(day)) => self.weekday_card_value.set_text(&day),
            Ok(None) => self.weekday_card_value.set_text("--"),
            Err(error) => {
                tracing::error!(%error, "stats: failed to load most active weekday");
                self.weekday_card_value.set_text("--");
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
                let max_plays = artists.first().map_or(1, |a| a.plays.max(1));
                for (i, artist) in artists.iter().enumerate() {
                    let row = artist_row(i + 1, &artist.artist, artist.plays, max_plays);
                    self.top_artists_box.append(&row);
                }
            }
            Err(error) => {
                tracing::error!(%error, "stats: failed to load top artists");
            }
        }
    }

    fn refresh_top_genres(&self, conn: &Connection) {
        clear_box(&self.top_genres_box);
        match stats_screen::top_genres(conn, TOP_LIMIT) {
            Ok(genres) => {
                if genres.is_empty() {
                    self.top_genres_box.append(&empty_label());
                    return;
                }
                let total_plays: i64 = genres.iter().map(|g| g.plays).sum();
                let max_plays = genres.first().map_or(1, |g| g.plays.max(1));
                for genre in &genres {
                    let pct = if total_plays > 0 {
                        (genre.plays as f64 / total_plays as f64 * 100.0).round() as i64
                    } else {
                        0
                    };
                    let row = genre_row(&genre.genre, genre.plays, max_plays, pct);
                    self.top_genres_box.append(&row);
                }
            }
            Err(error) => {
                tracing::error!(%error, "stats: failed to load top genres");
            }
        }
    }

    fn refresh_hourly_chart(&self, conn: &Connection) {
        match stats_screen::listening_by_hour(conn) {
            Ok(hourly) => {
                let sparse: Vec<(u8, i64)> = hourly.iter().map(|h| (h.hour, h.listens)).collect();
                self.hourly_chart.set_data(&sparse);
            }
            Err(error) => {
                tracing::error!(%error, "stats: failed to load hourly listens");
            }
        }
    }

    fn refresh_top_albums(&self, conn: &Connection) {
        clear_box(&self.top_albums_box);
        match stats_screen::top_albums(conn, ALBUMS_LIMIT) {
            Ok(albums) => {
                if albums.is_empty() {
                    self.top_albums_box.append(&empty_label());
                    return;
                }
                for album in &albums {
                    let item = album_strip_item(&album.album, album.plays);
                    self.top_albums_box.append(&item);
                }
            }
            Err(error) => {
                tracing::error!(%error, "stats: failed to load top albums");
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

/// Builds a mini stat card: a large value label above a small description.
/// Returns (card widget, value label) so the caller can update the value.
fn mini_card(label_text: &str) -> (gtk4::Box, gtk4::Label) {
    let value_label = gtk4::Label::new(Some("--"));
    value_label.add_css_class("stats-mini-card-value");
    value_label.set_xalign(0.0);

    let desc_label = gtk4::Label::new(Some(label_text));
    desc_label.add_css_class("stats-mini-card-label");
    desc_label.set_xalign(0.0);

    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    card.add_css_class("stats-mini-card");
    card.append(&value_label);
    card.append(&desc_label);

    (card, value_label)
}

/// Wraps content in a card with a section title above it.
fn titled_card(title: &str, content: &gtk4::Widget) -> gtk4::Box {
    let title_label = gtk4::Label::new(Some(title));
    title_label.add_css_class("stats-section-title");
    title_label.set_xalign(0.0);
    title_label.set_margin_bottom(8);

    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    card.add_css_class("stats-card");
    card.append(&title_label);
    card.append(content);
    card
}

/// Builds one row in the Top Artists list: rank, placeholder circle, name,
/// progress bar, play count.
fn artist_row(rank: usize, name: &str, plays: i64, max_plays: i64) -> gtk4::Box {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    hbox.set_margin_top(4);
    hbox.set_margin_bottom(4);
    hbox.set_valign(gtk4::Align::Center);

    let rank_label = gtk4::Label::new(Some(&rank.to_string()));
    rank_label.add_css_class("stats-rank");
    rank_label.set_xalign(1.0);
    hbox.append(&rank_label);

    // Circular cover placeholder
    let cover = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    cover.set_size_request(28, 28);
    cover.add_css_class("stats-album-thumb");
    // Force circular shape via inline CSS-friendly approach: use a frame.
    cover.set_halign(gtk4::Align::Center);
    cover.set_valign(gtk4::Align::Center);
    hbox.append(&cover);

    let name_label = gtk4::Label::new(Some(name));
    name_label.add_css_class("stats-item-title");
    name_label.set_xalign(0.0);
    name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    name_label.set_hexpand(false);
    name_label.set_width_chars(12);
    name_label.set_max_width_chars(18);

    let bar = progress_bar(plays, max_plays);

    let count_label = gtk4::Label::new(Some(&format_thousands(plays)));
    count_label.add_css_class("stats-play-count");
    count_label.set_valign(gtk4::Align::Center);

    hbox.append(&name_label);
    hbox.append(&bar);
    hbox.append(&count_label);

    hbox
}

/// Builds one row in the Top Genres list: genre name, progress bar, percentage.
fn genre_row(name: &str, plays: i64, max_plays: i64, pct: i64) -> gtk4::Box {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    hbox.set_margin_top(4);
    hbox.set_margin_bottom(4);
    hbox.set_valign(gtk4::Align::Center);

    let name_label = gtk4::Label::new(Some(name));
    name_label.add_css_class("stats-genre-name");
    name_label.set_xalign(0.0);
    name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);

    let bar = progress_bar(plays, max_plays);

    let pct_label = gtk4::Label::new(Some(&format!("{pct}%")));
    pct_label.add_css_class("stats-genre-pct");
    pct_label.set_xalign(1.0);

    hbox.append(&name_label);
    hbox.append(&bar);
    hbox.append(&pct_label);

    hbox
}

/// Creates a horizontal progress bar proportional to `value / max_value`.
fn progress_bar(value: i64, max_value: i64) -> gtk4::LevelBar {
    let bar = gtk4::LevelBar::new();
    bar.add_css_class("stats-progress-bar");
    bar.set_min_value(0.0);
    bar.set_max_value(1.0);
    let fraction = if max_value > 0 {
        value as f64 / max_value as f64
    } else {
        0.0
    };
    bar.set_value(fraction);
    bar.set_hexpand(true);
    bar.set_valign(gtk4::Align::Center);
    // Remove default offset markers that add unwanted color bands.
    bar.remove_offset_value(Some("low"));
    bar.remove_offset_value(Some("high"));
    bar.remove_offset_value(Some("full"));
    bar
}

/// Builds one item in the horizontal Top Albums strip: a placeholder cover
/// square with album name and play count below it.
fn album_strip_item(album_name: &str, plays: i64) -> gtk4::Box {
    let cover = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    cover.add_css_class("stats-album-thumb");
    cover.set_size_request(96, 96);

    let name_label = gtk4::Label::new(Some(album_name));
    name_label.add_css_class("stats-item-title");
    name_label.set_xalign(0.0);
    name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    name_label.set_max_width_chars(14);

    let plays_label = gtk4::Label::new(Some(&format!("{} plays", format_thousands(plays))));
    plays_label.add_css_class("stats-item-subtitle");
    plays_label.set_xalign(0.0);

    let item = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    item.append(&cover);
    item.append(&name_label);
    item.append(&plays_label);

    item
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
