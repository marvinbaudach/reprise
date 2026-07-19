//! Editorial My Stats composer and refresh orchestration.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Datelike;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::cover::ThumbnailSize;
use reprise_core::format::format_thousands;
use reprise_core::library::group_key::GroupKind;
use reprise_core::library::settings::{self, StatsLayout};
use reprise_core::library::stats_period::StatsPeriod;
use reprise_core::library::stats_screen::{group_track_ids, TopTrack};
use reprise_core::library::stats_snapshot::{self, SortBy, StatsSnapshot};
use rusqlite::Connection;

use super::hourly_chart::HourlyChart;
use super::stats_customize::StatsCustomize;
use super::stats_genre_bar::StatsGenreBar;
use super::stats_highlights::StatsHighlights;
use super::stats_ribbon::StatsRibbon;
use super::stats_spotlight::StatsSpotlight;
use crate::ui::cover_loader::CoverLoader;

const CONTENT_MAX_WIDTH: i32 = 1120;
const TOP_TRACK_LIMIT: usize = 10;
const SECTION_SPACING: i32 = 28;
const ASYMMETRIC_BREAKPOINT: f64 = 720.0;
#[cfg(test)]
const SECTION_ORDER: [&str; 6] = [
    "ribbon",
    "spotlight",
    "genres",
    "clock-highlights",
    "top-tracks",
    "end",
];

type StringCallback = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;
type PairCallback = Rc<RefCell<Option<Rc<dyn Fn(String, String)>>>>;
type StringsCallback = Rc<RefCell<Option<Rc<dyn Fn(Vec<String>)>>>>;
type IdsCallback = Rc<RefCell<Option<Rc<dyn Fn(Vec<i64>)>>>>;

pub(in crate::ui) struct StatsView {
    root: gtk4::ScrolledWindow,
    page_stack: gtk4::Stack,
    period_dropdown: gtk4::DropDown,
    period_model: gtk4::StringList,
    periods: Rc<RefCell<Vec<StatsPeriod>>>,
    wired: Cell<bool>,
    connection: Rc<RefCell<Option<Rc<RefCell<Connection>>>>>,
    hero_time: gtk4::Label,
    comparison_pill: gtk4::Label,
    hero_subline: gtk4::Label,
    ribbon: StatsRibbon,
    spotlight: StatsSpotlight,
    genres: StatsGenreBar,
    clock: HourlyChart,
    highlights: StatsHighlights,
    genres_section: gtk4::Box,
    clock_section: gtk4::Box,
    highlights_section: gtk4::Box,
    #[cfg_attr(not(test), allow(dead_code))]
    asymmetric_row: gtk4::Box,
    #[cfg_attr(not(test), allow(dead_code))]
    asymmetric_bin: adw::BreakpointBin,
    top_tracks_box: gtk4::Box,
    current_snapshot: Rc<RefCell<Option<StatsSnapshot>>>,
    sort_by: Rc<Cell<SortBy>>,
    cover_loader: Rc<CoverLoader>,
    top_track_generation: Rc<Cell<u64>>,
    customize: StatsCustomize,
    on_spotlight_play: PairCallback,
    on_go_to_artist: StringCallback,
    on_create_smart_mix: StringsCallback,
    on_unify_spellings: IdsCallback,
}

impl StatsView {
    pub(in crate::ui) fn new(cover_loader: Rc<CoverLoader>) -> Self {
        let hero_time = label("0 h", "stats-headline-hours");
        let comparison_pill = label("", "stats-pill");
        comparison_pill.set_visible(false);
        let hero_subline = label(
            "0 plays \u{00b7} \u{00d8} 0 min/day \u{00b7} 0 artists",
            "stats-headline-subtitle",
        );
        let hero_text = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        let time_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        time_row.append(&hero_time);
        time_row.append(&comparison_pill);
        hero_text.append(&time_row);
        hero_text.append(&hero_subline);
        hero_text.set_hexpand(true);

        let period_model = gtk4::StringList::new(&[]);
        let period_dropdown = gtk4::DropDown::builder().model(&period_model).build();
        period_dropdown.add_css_class("stats-period-dropdown");
        let customize = StatsCustomize::new();
        let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        controls.set_valign(gtk4::Align::Center);
        controls.append(&period_dropdown);
        controls.append(customize.widget());
        let hero = gtk4::Box::new(gtk4::Orientation::Horizontal, 18);
        hero.set_valign(gtk4::Align::Center);
        hero.append(&hero_text);
        hero.append(&controls);

        let ribbon = StatsRibbon::new();
        let spotlight = StatsSpotlight::new();
        spotlight.set_cover_loader(cover_loader.clone());
        let genres = StatsGenreBar::new();
        let clock = HourlyChart::new();
        let highlights = StatsHighlights::new();

        let genres_section = section("GENRE SPECTRUM", genres.widget());
        let clock_section = section("LISTENING CLOCK", clock.widget());
        let highlights_section = section("HIGHLIGHTS", highlights.widget());
        clock_section.set_hexpand(true);
        clock_section.set_width_request(405);
        highlights_section.set_hexpand(true);
        highlights_section.set_width_request(300);
        let asymmetric_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 20);
        asymmetric_row.append(&clock_section);
        asymmetric_row.append(&highlights_section);
        let asymmetric_bin = adw::BreakpointBin::new();
        asymmetric_bin.set_width_request(1);
        asymmetric_bin.set_height_request(1);
        asymmetric_bin.set_child(Some(&asymmetric_row));
        let condition = adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            ASYMMETRIC_BREAKPOINT,
            adw::LengthUnit::Px,
        );
        let breakpoint = adw::Breakpoint::new(condition);
        breakpoint.add_setter(
            &asymmetric_row,
            "orientation",
            Some(&gtk4::Orientation::Vertical.to_value()),
        );
        asymmetric_bin.add_breakpoint(breakpoint);

        let top_tracks_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let current_snapshot = Rc::new(RefCell::new(None::<StatsSnapshot>));
        let sort_by = Rc::new(Cell::new(SortBy::Plays));
        let top_track_generation = Rc::new(Cell::new(0_u64));
        let sort_controls = build_sort_controls(
            &top_tracks_box,
            &current_snapshot,
            &sort_by,
            &cover_loader,
            &top_track_generation,
        );
        let top_tracks_content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        top_tracks_content.append(&sort_controls);
        top_tracks_content.append(&top_tracks_box);

        let page = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_SPACING);
        page.set_margin_top(32);
        page.set_margin_bottom(32);
        page.set_margin_start(24);
        page.set_margin_end(24);
        page.append(&hero);
        page.append(&card(ribbon.widget()));
        page.append(&card(spotlight.widget()));
        page.append(&genres_section);
        page.append(&asymmetric_bin);
        page.append(&section("TOP TRACKS", &top_tracks_content));
        let clamp = adw::Clamp::builder()
            .maximum_size(CONTENT_MAX_WIDTH)
            .child(&page)
            .build();

        let empty = adw::StatusPage::builder()
            .title("Start listening to see your stats")
            .icon_name("audio-x-generic-symbolic")
            .build();
        let page_stack = gtk4::Stack::new();
        page_stack.add_named(&clamp, Some("sections"));
        page_stack.add_named(&empty, Some("empty"));
        page_stack.set_visible_child_name("empty");
        let root = gtk4::ScrolledWindow::builder()
            .child(&page_stack)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .hexpand(true)
            .vexpand(true)
            .build();

        let connection = Rc::new(RefCell::new(None::<Rc<RefCell<Connection>>>));
        let on_spotlight_play: PairCallback = Rc::new(RefCell::new(None));
        let on_go_to_artist: StringCallback = Rc::new(RefCell::new(None));
        let on_create_smart_mix: StringsCallback = Rc::new(RefCell::new(None));
        let on_unify_spellings: IdsCallback = Rc::new(RefCell::new(None));

        spotlight.set_on_play({
            let current_snapshot = current_snapshot.clone();
            let callback = on_spotlight_play.clone();
            move |key| {
                let label = current_snapshot
                    .borrow()
                    .as_ref()
                    .and_then(|snapshot| snapshot.spotlight.as_ref())
                    .map(|section| section.artist.group.label.clone());
                if let (Some(label), Some(callback)) = (label, callback.borrow().clone()) {
                    callback(label, key);
                }
            }
        });
        spotlight.set_on_go_to_artist({
            let callback = on_go_to_artist.clone();
            move |artist| {
                if let Some(callback) = callback.borrow().clone() {
                    callback(artist);
                }
            }
        });
        wire_unify(&spotlight, &genres, &connection, &on_unify_spellings);
        highlights.set_on_create_mix({
            let current_snapshot = current_snapshot.clone();
            let callback = on_create_smart_mix.clone();
            move || {
                let genres = current_snapshot.borrow().as_ref().map(|snapshot| {
                    snapshot
                        .genres
                        .segments
                        .iter()
                        .filter(|genre| genre.label != "Other")
                        .map(|genre| genre.label.clone())
                        .collect::<Vec<_>>()
                });
                if let (Some(genres), Some(callback)) = (genres, callback.borrow().clone()) {
                    callback(genres);
                }
            }
        });
        customize.set_on_changed({
            let connection = connection.clone();
            let clock_section = clock_section.clone();
            let genres_section = genres_section.clone();
            let highlights_section = highlights_section.clone();
            move |layout| {
                apply_layout_widgets(layout, &clock_section, &genres_section, &highlights_section);
                let conn = connection.borrow().clone();
                if let Some(conn) = conn {
                    if let Err(error) = settings::set_stats_layout(&conn.borrow(), layout) {
                        tracing::error!(%error, "failed to persist stats layout");
                    }
                }
            }
        });

        Self {
            root,
            page_stack,
            period_dropdown,
            period_model,
            periods: Rc::new(RefCell::new(Vec::new())),
            wired: Cell::new(false),
            connection,
            hero_time,
            comparison_pill,
            hero_subline,
            ribbon,
            spotlight,
            genres,
            clock,
            highlights,
            genres_section,
            clock_section,
            highlights_section,
            asymmetric_row,
            asymmetric_bin,
            top_tracks_box,
            current_snapshot,
            sort_by,
            cover_loader,
            top_track_generation,
            customize,
            on_spotlight_play,
            on_go_to_artist,
            on_create_smart_mix,
            on_unify_spellings,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    pub(in crate::ui) fn wire_year_selector(&self, conn: &Rc<RefCell<Connection>>) {
        *self.connection.borrow_mut() = Some(conn.clone());
        let now_year = chrono::Local::now().year();
        let periods = StatsPeriod::available(&conn.borrow(), now_year).unwrap_or_else(|error| {
            tracing::error!(%error, "failed to load stats periods");
            vec![StatsPeriod::YearToDate(now_year)]
        });
        let labels = periods
            .iter()
            .map(|period| period.label())
            .collect::<Vec<_>>();
        let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
        self.period_model
            .splice(0, self.period_model.n_items(), &label_refs);
        *self.periods.borrow_mut() = periods;
        self.period_dropdown.set_selected(0);

        if !self.wired.replace(true) {
            let connection = self.connection.clone();
            let periods = self.periods.clone();
            let page_stack = self.page_stack.clone();
            let current_snapshot = self.current_snapshot.clone();
            let render = self.render_parts();
            self.period_dropdown
                .connect_selected_notify(move |dropdown| {
                    let period = periods.borrow().get(dropdown.selected() as usize).copied();
                    let conn = connection.borrow().clone();
                    if let (Some(period), Some(conn)) = (period, conn) {
                        refresh_parts(&conn, period, &page_stack, &current_snapshot, &render);
                    }
                });
        }
        self.refresh(conn);
    }

    pub(in crate::ui) fn refresh(&self, conn: &Rc<RefCell<Connection>>) {
        *self.connection.borrow_mut() = Some(conn.clone());
        let period = self
            .periods
            .borrow()
            .get(self.period_dropdown.selected() as usize)
            .copied()
            .unwrap_or_else(|| StatsPeriod::YearToDate(chrono::Local::now().year()));
        let render = self.render_parts();
        refresh_parts(
            conn,
            period,
            &self.page_stack,
            &self.current_snapshot,
            &render,
        );
    }

    pub(in crate::ui) fn set_on_spotlight_play(&self, callback: impl Fn(String, String) + 'static) {
        *self.on_spotlight_play.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_go_to_artist(&self, callback: impl Fn(String) + 'static) {
        *self.on_go_to_artist.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_create_smart_mix(&self, callback: impl Fn(Vec<String>) + 'static) {
        *self.on_create_smart_mix.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_unify_spellings(&self, callback: impl Fn(Vec<i64>) + 'static) {
        *self.on_unify_spellings.borrow_mut() = Some(Rc::new(callback));
    }

    fn render_parts(&self) -> RenderParts {
        RenderParts {
            hero_time: self.hero_time.clone(),
            comparison_pill: self.comparison_pill.clone(),
            hero_subline: self.hero_subline.clone(),
            ribbon: self.ribbon.clone(),
            spotlight_section: self.spotlight.clone(),
            genres_section_data: self.genres.clone(),
            clock_section_data: self.clock.clone(),
            highlights_section_data: self.highlights.clone(),
            top_tracks_box: self.top_tracks_box.clone(),
            sort_by: self.sort_by.clone(),
            cover_loader: self.cover_loader.clone(),
            top_track_generation: self.top_track_generation.clone(),
            customize: self.customize.clone(),
            clock_section: self.clock_section.clone(),
            genres_section: self.genres_section.clone(),
            highlights_section: self.highlights_section.clone(),
        }
    }

    #[cfg(test)]
    fn apply_layout_and_persist(&self, conn: &Connection, layout: StatsLayout) {
        apply_layout_widgets(
            layout,
            &self.clock_section,
            &self.genres_section,
            &self.highlights_section,
        );
        self.customize.set_layout(layout);
        settings::set_stats_layout(conn, layout).unwrap();
    }
}

#[derive(Clone)]
struct RenderParts {
    hero_time: gtk4::Label,
    comparison_pill: gtk4::Label,
    hero_subline: gtk4::Label,
    ribbon: StatsRibbon,
    spotlight_section: StatsSpotlight,
    genres_section_data: StatsGenreBar,
    clock_section_data: HourlyChart,
    highlights_section_data: StatsHighlights,
    top_tracks_box: gtk4::Box,
    sort_by: Rc<Cell<SortBy>>,
    cover_loader: Rc<CoverLoader>,
    top_track_generation: Rc<Cell<u64>>,
    customize: StatsCustomize,
    clock_section: gtk4::Box,
    genres_section: gtk4::Box,
    highlights_section: gtk4::Box,
}

fn refresh_parts(
    conn: &Rc<RefCell<Connection>>,
    period: StatsPeriod,
    page_stack: &gtk4::Stack,
    current_snapshot: &Rc<RefCell<Option<StatsSnapshot>>>,
    render: &RenderParts,
) {
    let now_unix = now_unix();
    let result = {
        let conn = conn.borrow();
        let layout = settings::get_stats_layout(&conn);
        stats_snapshot::compute(&conn, period, now_unix, &chrono::Local)
            .map(|snapshot| (snapshot, layout))
    };
    match result {
        Ok((snapshot, layout)) => {
            apply_layout_widgets(
                layout,
                &render.clock_section,
                &render.genres_section,
                &render.highlights_section,
            );
            render.customize.set_layout(layout);
            if snapshot.is_empty() {
                render.ribbon.widget().set_visible(false);
                page_stack.set_visible_child_name("empty");
            } else {
                render.ribbon.widget().set_visible(true);
                render_snapshot(render, &snapshot);
                page_stack.set_visible_child_name("sections");
            }
            *current_snapshot.borrow_mut() = Some(snapshot);
        }
        Err(error) => {
            tracing::error!(%error, "failed to compute My Stats snapshot");
            render.ribbon.widget().set_visible(false);
            page_stack.set_visible_child_name("empty");
            *current_snapshot.borrow_mut() = None;
        }
    }
}

fn render_snapshot(render: &RenderParts, snapshot: &StatsSnapshot) {
    render
        .hero_time
        .set_label(&format_duration(snapshot.hero.total_ms));
    if let Some(percent) = snapshot.hero.comparison_percent {
        render.comparison_pill.set_label(&format!(
            "{} {}% vs previous period",
            if percent >= 0 { "\u{25b2}" } else { "\u{25bc}" },
            percent.abs()
        ));
        render.comparison_pill.set_visible(true);
    } else {
        render.comparison_pill.set_visible(false);
    }
    render.hero_subline.set_label(&format!(
        "{} plays \u{00b7} \u{00d8} {} min/day \u{00b7} {} artists",
        format_thousands(snapshot.hero.plays),
        snapshot.hero.average_ms_per_day / 60_000,
        format_thousands(snapshot.hero.artists)
    ));
    let ribbon_values = snapshot
        .ribbon
        .iter()
        .map(|point| point.total_ms)
        .collect::<Vec<_>>();
    render.ribbon.set_data(&snapshot.period, &ribbon_values);
    if let Some(spotlight) = &snapshot.spotlight {
        render.spotlight_section.set_data(spotlight);
        render.spotlight_section.widget().set_visible(true);
    } else {
        render.spotlight_section.widget().set_visible(false);
    }
    render.genres_section_data.set_data(&snapshot.genres);
    render.clock_section_data.set_data(&snapshot.clock);
    render
        .highlights_section_data
        .set_data(&snapshot.highlights);
    render_tracks(
        &render.top_tracks_box,
        snapshot,
        render.sort_by.get(),
        &render.cover_loader,
        &render.top_track_generation,
    );
}

fn build_sort_controls(
    tracks_box: &gtk4::Box,
    snapshot: &Rc<RefCell<Option<StatsSnapshot>>>,
    sort_by: &Rc<Cell<SortBy>>,
    cover_loader: &Rc<CoverLoader>,
    generation: &Rc<Cell<u64>>,
) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    row.set_halign(gtk4::Align::End);
    let plays = gtk4::ToggleButton::with_label("by plays");
    let time = gtk4::ToggleButton::with_label("by time");
    time.set_group(Some(&plays));
    plays.set_active(true);
    for (button, value) in [(&plays, SortBy::Plays), (&time, SortBy::Time)] {
        button.connect_toggled({
            let tracks_box = tracks_box.clone();
            let snapshot = snapshot.clone();
            let sort_by = sort_by.clone();
            let cover_loader = cover_loader.clone();
            let generation = generation.clone();
            move |button| {
                if !button.is_active() {
                    return;
                }
                sort_by.set(value);
                let snapshot = snapshot.borrow().clone();
                if let Some(snapshot) = snapshot {
                    render_tracks(&tracks_box, &snapshot, value, &cover_loader, &generation);
                }
            }
        });
    }
    row.append(&plays);
    row.append(&time);
    row
}

fn render_tracks(
    container: &gtk4::Box,
    snapshot: &StatsSnapshot,
    sort_by: SortBy,
    cover_loader: &Rc<CoverLoader>,
    generation: &Rc<Cell<u64>>,
) {
    clear(container);
    let token = generation.get().wrapping_add(1);
    generation.set(token);
    let tracks = snapshot.top_tracks_sorted(sort_by);
    let leader = tracks.first().map_or(0, |track| metric(track, sort_by));
    for (index, track) in tracks.iter().take(TOP_TRACK_LIMIT).enumerate() {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        row.add_css_class("stats-top-track-row");
        row.append(&label(&(index + 1).to_string(), "stats-rank"));
        let cover = gtk4::Image::builder()
            .pixel_size(40)
            .width_request(40)
            .height_request(40)
            .build();
        CoverLoader::set_placeholder(&cover);
        cover_loader.load_into(
            &cover,
            &track.track_path,
            ThumbnailSize::List,
            token,
            generation,
        );
        row.append(&cover);
        let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        text.set_hexpand(true);
        text.append(&label(&track.title, "stats-item-title"));
        text.append(&label(&track.artist, "stats-item-subtitle"));
        row.append(&text);
        let bar = gtk4::LevelBar::new();
        bar.set_min_value(0.0);
        bar.set_max_value(1.0);
        bar.set_value(if leader > 0 {
            metric(track, sort_by) as f64 / leader as f64
        } else {
            0.0
        });
        bar.set_width_request(120);
        row.append(&bar);
        row.append(&label(
            &format!(
                "{} plays \u{00b7} {}",
                track.play_count,
                format_duration(track.total_ms)
            ),
            "stats-play-count",
        ));
        container.append(&row);
    }
}

fn wire_unify(
    spotlight: &StatsSpotlight,
    genres: &StatsGenreBar,
    connection: &Rc<RefCell<Option<Rc<RefCell<Connection>>>>>,
    callback: &IdsCallback,
) {
    spotlight.set_on_unify({
        let connection = connection.clone();
        let callback = callback.clone();
        move |key| resolve_unify(&connection, &callback, GroupKind::Artist, &key)
    });
    genres.set_on_unify({
        let connection = connection.clone();
        let callback = callback.clone();
        move |key| resolve_unify(&connection, &callback, GroupKind::Genre, &key)
    });
}

fn resolve_unify(
    connection: &Rc<RefCell<Option<Rc<RefCell<Connection>>>>>,
    callback: &IdsCallback,
    kind: GroupKind,
    key: &str,
) {
    let connection = connection.borrow().clone();
    let Some(connection) = connection else { return };
    let ids = group_track_ids(&connection.borrow(), kind, key);
    match ids {
        Ok(ids) if !ids.is_empty() => {
            if let Some(callback) = callback.borrow().clone() {
                callback(ids);
            }
        }
        Ok(_) => {}
        Err(error) => tracing::error!(%error, "failed to resolve stats group tracks"),
    }
}

fn apply_layout_widgets(
    layout: StatsLayout,
    clock: &gtk4::Box,
    genres: &gtk4::Box,
    highlights: &gtk4::Box,
) {
    clock.set_visible(layout.clock);
    genres.set_visible(layout.genres);
    highlights.set_visible(layout.highlights);
}

fn metric(track: &TopTrack, sort_by: SortBy) -> i64 {
    match sort_by {
        SortBy::Plays => track.play_count,
        SortBy::Time => track.total_ms,
    }
}

fn section(title: &str, content: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    root.append(&label(title, "stats-section-title"));
    root.append(content);
    root
}

fn card(content: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    card.add_css_class("stats-card");
    card.append(content);
    card
}

fn label(text: &str, class: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class(class);
    label.set_xalign(0.0);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label
}

fn clear(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn format_duration(milliseconds: i64) -> String {
    let minutes = milliseconds.max(0) / 60_000;
    format!("{} h {} min", minutes / 60, minutes % 60)
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view_and_conn() -> (StatsView, Rc<RefCell<Connection>>) {
        let conn = Rc::new(RefCell::new(reprise_core::db::open(None).unwrap()));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        let loader = CoverLoader::new(crate::ui::cover_download_worker::setup());
        (StatsView::new(loader), conn)
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_7_customize_toggles_sections() {
        gtk4::init().unwrap();
        let (view, conn) = view_and_conn();
        view.wire_year_selector(&conn);
        assert!(view.clock_section.is_visible());
        assert!(view.genres_section.is_visible());
        assert!(view.highlights_section.is_visible());
        let before = SECTION_ORDER;

        let layout = StatsLayout {
            clock: false,
            genres: true,
            highlights: true,
        };
        view.apply_layout_and_persist(&conn.borrow(), layout);

        assert!(!view.clock_section.is_visible());
        assert!(view.genres_section.is_visible());
        assert!(view.highlights_section.is_visible());
        assert_eq!(SECTION_ORDER, before);
        assert_eq!(view.customize.check_count(), 3);
        assert_eq!(settings::get_stats_layout(&conn.borrow()), layout);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_view_empty_history_shows_the_status_page_and_no_ribbon() {
        gtk4::init().unwrap();
        let (view, conn) = view_and_conn();
        view.wire_year_selector(&conn);

        assert_eq!(
            view.page_stack.visible_child_name().as_deref(),
            Some("empty")
        );
        assert!(!view.ribbon.widget().is_visible());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_view_narrow_width_stacks_the_asymmetric_row() {
        gtk4::init().unwrap();
        let (view, _) = view_and_conn();
        let window = adw::Window::builder()
            .default_width(600)
            .default_height(700)
            .content(view.widget())
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert_eq!(
            view.asymmetric_row.orientation(),
            gtk4::Orientation::Vertical
        );
        assert!(view.asymmetric_bin.width() <= ASYMMETRIC_BREAKPOINT as i32);
    }
}
