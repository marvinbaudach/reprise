//! The "My Stats" screen: a scrollable page showing headline listening
//! totals, top artists/albums/tracks, and a 12-month activity chart.
//!
//! Data comes from `reprise_core::library::stats_screen` (read-only queries
//! against the existing `tracks` and `listen_events` tables).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk4::prelude::*;
use rusqlite::Connection;

use reprise_core::cover::ThumbnailSize;
use reprise_core::format::format_thousands;
use reprise_core::library::stats_screen;

use super::stats_chart::StatsChart;
use crate::ui::cover_loader::CoverLoader;

const TOP_LIMIT: usize = 5;
const CONTENT_MAX_WIDTH: i32 = 680;
const SECTION_SPACING: i32 = 28;
const LIST_SPACING: i32 = 6;

/// Pixel size for small cover thumbnails in list rows (artists, tracks).
const ROW_COVER_SIZE: i32 = 28;

pub(in crate::ui) struct StatsView {
    root: gtk4::ScrolledWindow,
    chart: StatsChart,
    headline_hours: gtk4::Label,
    headline_subtitle: gtk4::Label,
    year_dropdown: gtk4::DropDown,
    top_artists_box: gtk4::Box,
    top_albums_box: gtk4::Box,
    top_tracks_box: gtk4::Box,
    cover_loader: Rc<CoverLoader>,
    /// Generation token for artist cover loads; incremented on refresh.
    artist_cover_gen: Rc<Cell<u64>>,
    /// Generation token for album cover loads; incremented on refresh.
    album_cover_gen: Rc<Cell<u64>>,
    /// Generation token for track cover loads; incremented on refresh.
    track_cover_gen: Rc<Cell<u64>>,
    /// The year model backing the dropdown (display strings).
    year_model: gtk4::StringList,
    /// The actual year values corresponding to each dropdown entry.
    /// Index 0 = "All time" (None), then years newest-first (Some(y)).
    year_values: Rc<RefCell<Vec<Option<i32>>>>,
}

impl StatsView {
    pub(in crate::ui) fn new(cover_loader: Rc<CoverLoader>) -> Self {
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

        // Year selector dropdown
        let year_model = gtk4::StringList::new(&[]);
        let year_dropdown = gtk4::DropDown::builder().model(&year_model).build();
        year_dropdown.add_css_class("stats-year-dropdown");

        let year_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        year_row.set_valign(gtk4::Align::Center);
        year_row.append(&headline_box);
        year_row.append(&year_dropdown);
        // Push dropdown to the right
        headline_box.set_hexpand(true);

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

        content.append(&year_row);

        let chart_card = card_wrapper(chart.widget().upcast_ref());
        content.append(&chart_card);

        content.append(&section("TOP ARTISTS", &top_artists_box));
        content.append(&section("TOP TRACKS", &top_tracks_box));
        content.append(&section("TOP ALBUMS", &top_albums_box));

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
            year_dropdown,
            top_artists_box,
            top_albums_box,
            top_tracks_box,
            cover_loader,
            artist_cover_gen: Rc::new(Cell::new(0)),
            album_cover_gen: Rc::new(Cell::new(0)),
            track_cover_gen: Rc::new(Cell::new(0)),
            year_model,
            year_values: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    /// Populates the year dropdown and wires the change callback.
    /// Must be called once after construction, when the connection is available.
    pub(in crate::ui) fn wire_year_selector(&self, conn: &Rc<RefCell<Connection>>) {
        self.populate_year_model(&conn.borrow());

        let conn = conn.clone();
        let headline_hours = self.headline_hours.clone();
        let headline_subtitle = self.headline_subtitle.clone();
        let chart = self.chart.clone();
        let top_artists_box = self.top_artists_box.clone();
        let top_albums_box = self.top_albums_box.clone();
        let top_tracks_box = self.top_tracks_box.clone();
        let cover_loader = self.cover_loader.clone();
        let artist_gen = self.artist_cover_gen.clone();
        let album_gen = self.album_cover_gen.clone();
        let track_gen = self.track_cover_gen.clone();
        let year_values = self.year_values.clone();
        self.year_dropdown.connect_selected_notify(move |dropdown| {
            let idx = dropdown.selected() as usize;
            let year = year_values.borrow().get(idx).copied().flatten();
            let conn = conn.borrow();
            refresh_headline(&conn, year, &headline_hours, &headline_subtitle);
            refresh_top_artists(&conn, year, &top_artists_box, &cover_loader, &artist_gen);
            refresh_top_albums(&conn, year, &top_albums_box, &cover_loader, &album_gen);
            refresh_top_tracks(&conn, year, &top_tracks_box, &cover_loader, &track_gen);
            refresh_chart(&conn, &chart);
        });
    }

    /// Returns the currently selected year from the dropdown.
    fn selected_year(&self) -> Option<i32> {
        let idx = self.year_dropdown.selected() as usize;
        self.year_values.borrow().get(idx).copied().flatten()
    }

    /// Populates the year model with "All time" + available years.
    fn populate_year_model(&self, conn: &Connection) {
        let current_year = current_calendar_year();
        let mut years = stats_screen::available_years(conn).unwrap_or_default();
        // Ensure current year is always present.
        if !years.contains(&current_year) {
            years.insert(0, current_year);
        }
        years.sort_unstable_by(|a, b| b.cmp(a));
        years.dedup();

        // Clear existing model
        let count = self.year_model.n_items();
        if count > 0 {
            self.year_model.splice(0, count, &[] as &[&str]);
        }

        let mut values = Vec::with_capacity(years.len() + 1);

        // "All time" entry
        self.year_model.append("All time");
        values.push(None);

        for &y in &years {
            let label = if y == current_year {
                format!("{y} so far")
            } else {
                y.to_string()
            };
            self.year_model.append(&label);
            values.push(Some(y));
        }

        *self.year_values.borrow_mut() = values;

        // Default selection: current year (index 1, since 0 = All time and
        // years are newest-first).
        self.year_dropdown.set_selected(1);
    }

    /// Fetches all stats from the database and updates every section.
    pub(in crate::ui) fn refresh(&self, conn: &Rc<RefCell<Connection>>) {
        let conn = conn.borrow();
        let year = self.selected_year();
        refresh_headline(&conn, year, &self.headline_hours, &self.headline_subtitle);
        refresh_top_artists(
            &conn,
            year,
            &self.top_artists_box,
            &self.cover_loader,
            &self.artist_cover_gen,
        );
        refresh_top_albums(
            &conn,
            year,
            &self.top_albums_box,
            &self.cover_loader,
            &self.album_cover_gen,
        );
        refresh_top_tracks(
            &conn,
            year,
            &self.top_tracks_box,
            &self.cover_loader,
            &self.track_cover_gen,
        );
        refresh_chart(&conn, &self.chart);
    }
}

fn refresh_headline(
    conn: &Connection,
    year: Option<i32>,
    headline_hours: &gtk4::Label,
    headline_subtitle: &gtk4::Label,
) {
    match stats_screen::headline_totals(conn, year) {
        Ok(totals) => {
            let hours = super::stats_chart_math::ms_to_hours(totals.total_ms);
            headline_hours.set_text(&format!("{} hours", format_thousands(hours)));
            let scope = match year {
                Some(y) => format!("{y}"),
                None => "all time".to_string(),
            };
            headline_subtitle.set_text(&format!(
                "{} plays \u{00b7} {}",
                format_thousands(totals.total_plays),
                scope,
            ));
        }
        Err(error) => {
            tracing::error!(%error, "stats: failed to load headline totals");
            headline_hours.set_text("\u{2014} hours");
            headline_subtitle.set_text("\u{2014}");
        }
    }
}

fn refresh_top_artists(
    conn: &Connection,
    year: Option<i32>,
    container: &gtk4::Box,
    cover_loader: &Rc<CoverLoader>,
    gen: &Rc<Cell<u64>>,
) {
    clear_box(container);
    let generation = gen.get().wrapping_add(1);
    gen.set(generation);
    match stats_screen::top_artists(conn, TOP_LIMIT, year) {
        Ok(artists) => {
            if artists.is_empty() {
                container.append(&empty_label());
                return;
            }
            for (i, artist) in artists.iter().enumerate() {
                let image = cover_image(ROW_COVER_SIZE);
                cover_loader.load_into(
                    &image,
                    &artist.representative_track_path,
                    ThumbnailSize::List,
                    generation,
                    gen,
                );
                let hours = super::stats_chart_math::ms_to_hours(artist.total_ms);
                let count_text = format!(
                    "{} plays \u{00b7} {}h",
                    format_thousands(artist.plays),
                    format_thousands(hours),
                );
                let row = list_row_with_cover(i + 1, &image, &artist.artist, None, &count_text);
                container.append(&row);
            }
        }
        Err(error) => {
            tracing::error!(%error, "stats: failed to load top artists");
        }
    }
}

fn refresh_top_albums(
    conn: &Connection,
    year: Option<i32>,
    container: &gtk4::Box,
    cover_loader: &Rc<CoverLoader>,
    gen: &Rc<Cell<u64>>,
) {
    clear_box(container);
    let generation = gen.get().wrapping_add(1);
    gen.set(generation);
    match stats_screen::top_albums(conn, TOP_LIMIT, year) {
        Ok(albums) => {
            if albums.is_empty() {
                container.append(&empty_label());
                return;
            }
            for (i, album) in albums.iter().enumerate() {
                let image = cover_image(ROW_COVER_SIZE);
                cover_loader.load_into(
                    &image,
                    &album.track_path,
                    ThumbnailSize::List,
                    generation,
                    gen,
                );
                let hours = super::stats_chart_math::ms_to_hours(album.total_ms);
                let count_text = format!(
                    "{} plays \u{00b7} {}h",
                    format_thousands(album.plays),
                    format_thousands(hours),
                );
                let row = list_row_with_cover(
                    i + 1,
                    &image,
                    &album.album,
                    Some(&album.album_artist),
                    &count_text,
                );
                container.append(&row);
            }
        }
        Err(error) => {
            tracing::error!(%error, "stats: failed to load top albums");
        }
    }
}

fn refresh_top_tracks(
    conn: &Connection,
    year: Option<i32>,
    container: &gtk4::Box,
    cover_loader: &Rc<CoverLoader>,
    gen: &Rc<Cell<u64>>,
) {
    clear_box(container);
    let generation = gen.get().wrapping_add(1);
    gen.set(generation);
    match stats_screen::top_tracks(conn, TOP_LIMIT, year) {
        Ok(tracks) => {
            if tracks.is_empty() {
                container.append(&empty_label());
                return;
            }
            for (i, track) in tracks.iter().enumerate() {
                let image = cover_image(ROW_COVER_SIZE);
                cover_loader.load_into(
                    &image,
                    &track.track_path,
                    ThumbnailSize::List,
                    generation,
                    gen,
                );
                let hours = super::stats_chart_math::ms_to_hours(track.total_ms);
                let count_text = format!(
                    "{} plays \u{00b7} {}h",
                    format_thousands(track.play_count),
                    format_thousands(hours),
                );
                let row = list_row_with_cover(
                    i + 1,
                    &image,
                    &track.title,
                    Some(&track.artist),
                    &count_text,
                );
                container.append(&row);
            }
        }
        Err(error) => {
            tracing::error!(%error, "stats: failed to load top tracks");
        }
    }
}

fn refresh_chart(conn: &Connection, chart: &StatsChart) {
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    match stats_screen::monthly_listen_timeseries(conn, now_unix) {
        Ok(series) => {
            let labels: Vec<String> = series.iter().map(|b| b.year_month.clone()).collect();
            let values: Vec<i64> = series.iter().map(|b| b.total_ms).collect();
            chart.set_data(&labels, &values);
        }
        Err(error) => {
            tracing::error!(%error, "stats: failed to load monthly timeseries");
        }
    }
}

/// Returns the current calendar year.
fn current_calendar_year() -> i32 {
    chrono::Local::now()
        .format("%Y")
        .to_string()
        .parse()
        .unwrap_or(2026)
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

/// Creates a cover image widget at the given pixel size with rounded corners.
fn cover_image(size: i32) -> gtk4::Image {
    let image = gtk4::Image::builder()
        .pixel_size(size)
        .width_request(size)
        .height_request(size)
        .build();
    image.add_css_class("stats-cover-thumb");
    CoverLoader::set_placeholder(&image);
    image
}

/// Builds one row in a top-N list with a cover image:
/// rank | cover | title (+ optional subtitle) | play count + hours.
fn list_row_with_cover(
    rank: usize,
    image: &gtk4::Image,
    title: &str,
    subtitle: Option<&str>,
    count_text: &str,
) -> gtk4::Box {
    let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    hbox.set_margin_top(4);
    hbox.set_margin_bottom(4);
    hbox.set_valign(gtk4::Align::Center);

    let rank_label = gtk4::Label::new(Some(&rank.to_string()));
    rank_label.add_css_class("stats-rank");
    rank_label.set_xalign(1.0);
    hbox.append(&rank_label);

    hbox.append(image);

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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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
