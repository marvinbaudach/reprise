//! Editorial My Stats composer and refresh orchestration.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Datelike;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::cover::ThumbnailSize;
use reprise_core::format::format_thousands;
use reprise_core::library::group_key::GroupKind;
use reprise_core::library::settings::{self, StatsLayout};
use reprise_core::library::stats_period::StatsPeriod;
use reprise_core::library::stats_screen::{group_track_ids, TopTrack};
use reprise_core::library::stats_snapshot::{self, ComparisonPresentation, SortBy, StatsSnapshot};
use rusqlite::Connection;

use super::hourly_chart::HourlyChart;
use super::stats_customize::StatsCustomize;
use super::stats_genre_bar::StatsGenreBar;
use super::stats_hero::StatsHero;
use super::stats_highlights::{StatsHighlights, TopGenre};
use super::stats_metadata_links::{self, MetadataCallback, StatsMetadataTarget};
use super::stats_ribbon::StatsRibbon;
use super::stats_spotlight::StatsSpotlight;
use super::stats_view_widgets::{card, clear, label, section};
use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;

const CONTENT_MAX_WIDTH: i32 = 1120;
const TOP_TRACK_LIMIT: usize = 10;
const SECTION_SPACING: i32 = 28;
const ASYMMETRIC_NATURAL_LINE_LENGTH: i32 = 720;
/// The asymmetric row keeps its 1.35 / 1 ratio, but both minimum widths have
/// to fit inside [`ASYMMETRIC_NATURAL_LINE_LENGTH`] together with the spacing.
/// The enclosing `ScrolledWindow` never scrolls horizontally, so this is also
/// the narrowest the whole window can get while the row is side by side.
const CLOCK_MIN_WIDTH: i32 = 324;
const HIGHLIGHTS_MIN_WIDTH: i32 = 240;
const ASYMMETRIC_SPACING: i32 = 20;
/// The fixed editorial order of the page's sections (STATS-7). The test reads
/// the real widget tree and compares against this.
#[cfg(test)]
const SECTION_ORDER: [&str; 6] = [
    "hero",
    "ribbon",
    "spotlight",
    "genres",
    "clock-highlights",
    "top-tracks",
];

type StringCallback = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;
type PairCallback = Rc<RefCell<Option<Rc<dyn Fn(String, String)>>>>;
type GenreCallback = Rc<RefCell<Option<Rc<dyn Fn(TopGenre)>>>>;
type IdsCallback = Rc<RefCell<Option<Rc<dyn Fn(Vec<i64>)>>>>;
#[derive(Clone)]
pub(in crate::ui) struct StatsView {
    root: gtk4::ScrolledWindow,
    page_stack: gtk4::Stack,
    period_dropdown: gtk4::DropDown,
    period_model: gtk4::StringList,
    periods: Rc<RefCell<Vec<StatsPeriod>>>,
    wired: Cell<bool>,
    connection: Rc<RefCell<Option<Rc<RefCell<Connection>>>>>,
    #[cfg_attr(not(test), allow(dead_code))]
    page: glib::WeakRef<gtk4::Box>,
    #[cfg_attr(not(test), allow(dead_code))]
    asymmetric_row: glib::WeakRef<adw::WrapBox>,
    #[cfg_attr(not(test), allow(dead_code))]
    hero_row: glib::WeakRef<adw::WrapBox>,
    #[cfg_attr(not(test), allow(dead_code))]
    hero_time_row: glib::WeakRef<adw::WrapBox>,
    current_snapshot: Rc<RefCell<Option<StatsSnapshot>>>,
    /// Built once and shared: the period dropdown's handler holds it weakly,
    /// which is what keeps the handler from owning the page it lives in.
    render: Rc<RenderParts>,
    on_spotlight_play: PairCallback,
    on_go_to_artist: StringCallback,
    on_create_smart_mix: GenreCallback,
    on_unify_spellings: IdsCallback,
    on_metadata_activate: MetadataCallback,
}

impl StatsView {
    pub(in crate::ui) fn new(cover_loader: Rc<CoverLoader>) -> Self {
        let customize = StatsCustomize::new();
        let hero = StatsHero::new(&customize);
        let hero_time = hero.time.clone();
        let comparison_pill = hero.comparison.clone();
        let hero_subline = hero.subline.clone();
        let period_dropdown = hero.period_dropdown.clone();
        let period_model = hero.period_model.clone();

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
        clock_section.set_width_request(CLOCK_MIN_WIDTH);
        highlights_section.set_hexpand(true);
        highlights_section.set_width_request(HIGHLIGHTS_MIN_WIDTH);
        // This row needs height-for-width layout just like the hero. Let its
        // owner measure wrapped lines instead of keeping a separately-owned
        // breakpoint target behind a one-pixel size request.
        let asymmetric_row = adw::WrapBox::new();
        asymmetric_row.set_child_spacing(ASYMMETRIC_SPACING);
        asymmetric_row.set_line_spacing(ASYMMETRIC_SPACING);
        asymmetric_row.set_natural_line_length(ASYMMETRIC_NATURAL_LINE_LENGTH);
        asymmetric_row.set_wrap_policy(adw::WrapPolicy::Natural);
        asymmetric_row.set_justify(adw::JustifyMode::Fill);
        asymmetric_row.set_justify_last_line(true);
        asymmetric_row.set_hexpand(true);
        asymmetric_row.append(&clock_section);
        asymmetric_row.append(&highlights_section);

        let top_tracks_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let current_snapshot = Rc::new(RefCell::new(None::<StatsSnapshot>));
        let sort_by = Rc::new(Cell::new(SortBy::Plays));
        let top_track_generation = Rc::new(Cell::new(0_u64));
        let on_metadata_activate: MetadataCallback = Rc::new(RefCell::new(None));
        let sort_controls = build_sort_controls(
            &top_tracks_box,
            &current_snapshot,
            &sort_by,
            &cover_loader,
            &top_track_generation,
            &on_metadata_activate,
        );
        let top_tracks_content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        top_tracks_content.append(&sort_controls);
        top_tracks_content.append(&top_tracks_box);

        let sections = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_SPACING);
        sections.append(&card(ribbon.widget()));
        sections.append(&card(spotlight.widget()));
        sections.append(&genres_section);
        sections.append(&asymmetric_row);
        sections.append(&section("TOP TRACKS", &top_tracks_content));

        let empty = adw::StatusPage::builder()
            .title(strings::stats_empty_title())
            .icon_name("audio-x-generic-symbolic")
            .build();
        // A failed query is not an empty history: telling the user to start
        // listening when the numbers exist but could not be read is a lie.
        let failed = adw::StatusPage::builder()
            .title(strings::stats_unavailable_title())
            .description(strings::stats_unavailable_description())
            .icon_name("dialog-warning-symbolic")
            .build();
        let page_stack = gtk4::Stack::new();
        page_stack.set_vexpand(true);
        page_stack.add_named(&sections, Some("sections"));
        page_stack.add_named(&empty, Some("empty"));
        page_stack.add_named(&failed, Some("failed"));
        page_stack.set_visible_child_name("empty");

        // The selected period is navigation, not data. Keep its hero outside
        // the conditional content stack so an empty rolling window or calendar
        // year never hides the only control that can leave that state.
        let page = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_SPACING);
        page.set_margin_top(32);
        page.set_margin_bottom(32);
        page.set_margin_start(24);
        page.set_margin_end(24);
        page.append(&hero.root);
        page.append(&page_stack);
        let clamp = adw::Clamp::builder()
            .maximum_size(CONTENT_MAX_WIDTH)
            .child(&page)
            .build();
        let root = gtk4::ScrolledWindow::builder()
            .child(&clamp)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .hexpand(true)
            .vexpand(true)
            .build();

        let connection = Rc::new(RefCell::new(None::<Rc<RefCell<Connection>>>));
        let on_spotlight_play: PairCallback = Rc::new(RefCell::new(None));
        let on_go_to_artist: StringCallback = Rc::new(RefCell::new(None));
        let on_create_smart_mix: GenreCallback = Rc::new(RefCell::new(None));
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
            let callback = on_create_smart_mix.clone();
            move |genre| {
                if let Some(callback) = callback.borrow().clone() {
                    callback(genre);
                }
            }
        });
        // The menu lives inside the very page whose sections it shows and
        // hides, so the sections are held weakly — a strong clone would make
        // the page tree own itself and nothing would ever be finalized.
        customize.set_on_changed(glib::clone!(
            #[strong]
            connection,
            #[weak]
            clock_section,
            #[weak]
            genres_section,
            #[weak]
            highlights_section,
            move |layout| {
                apply_layout_widgets(layout, &clock_section, &genres_section, &highlights_section);
                let conn = connection.borrow().clone();
                if let Some(conn) = conn {
                    if let Err(error) = settings::set_stats_layout(&conn.borrow(), layout) {
                        tracing::error!(%error, "failed to persist stats layout");
                    }
                }
            }
        ));

        let render = Rc::new(RenderParts {
            hero_time: hero_time.clone(),
            comparison_pill: comparison_pill.clone(),
            hero_subline: hero_subline.clone(),
            ribbon: ribbon.clone(),
            spotlight_section: spotlight.clone(),
            genres_section_data: genres.clone(),
            clock_section_data: clock.clone(),
            highlights_section_data: highlights.clone(),
            top_tracks_box: top_tracks_box.clone(),
            sort_by: sort_by.clone(),
            cover_loader,
            top_track_generation: top_track_generation.clone(),
            on_metadata_activate: on_metadata_activate.clone(),
            customize: customize.clone(),
            clock_section: clock_section.clone(),
            genres_section: genres_section.clone(),
            highlights_section: highlights_section.clone(),
        });

        Self {
            root,
            page_stack,
            period_dropdown,
            period_model,
            periods: Rc::new(RefCell::new(Vec::new())),
            wired: Cell::new(false),
            connection,
            page: page.downgrade(),
            asymmetric_row: asymmetric_row.downgrade(),
            hero_row: hero.row.downgrade(),
            hero_time_row: hero.time_row.downgrade(),
            current_snapshot,
            render,
            on_spotlight_play,
            on_go_to_artist,
            on_create_smart_mix,
            on_unify_spellings,
            on_metadata_activate,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    pub(in crate::ui) fn wire_year_selector(&self, conn: &Rc<RefCell<Connection>>) {
        *self.connection.borrow_mut() = Some(conn.clone());
        let now_year = chrono::Local::now().year();
        let periods = {
            let conn = conn.borrow();
            StatsPeriod::available(&conn, now_year, &chrono::Local).unwrap_or_else(|error| {
                tracing::error!(%error, "failed to read available My Stats periods");
                vec![
                    StatsPeriod::YearToDate(now_year),
                    StatsPeriod::AllTime,
                    StatsPeriod::Last30Days,
                ]
            })
        };
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
            let current_snapshot = self.current_snapshot.clone();
            // The dropdown sits inside the page it re-renders. Both the stack
            // and the render targets are therefore held weakly; otherwise the
            // handler keeps the entire page widget tree alive forever.
            let render = Rc::downgrade(&self.render);
            self.period_dropdown.connect_selected_notify(glib::clone!(
                #[weak(rename_to = page_stack)]
                self.page_stack,
                move |dropdown| {
                    let Some(render) = render.upgrade() else {
                        return;
                    };
                    let period = periods.borrow().get(dropdown.selected() as usize).copied();
                    let conn = connection.borrow().clone();
                    if let (Some(period), Some(conn)) = (period, conn) {
                        refresh_parts(&conn, period, &page_stack, &current_snapshot, &render);
                    }
                }
            ));
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
        refresh_parts(
            conn,
            period,
            &self.page_stack,
            &self.current_snapshot,
            &self.render,
        );
    }

    pub(in crate::ui) fn set_on_spotlight_play(&self, callback: impl Fn(String, String) + 'static) {
        *self.on_spotlight_play.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_go_to_artist(&self, callback: impl Fn(String) + 'static) {
        *self.on_go_to_artist.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_create_smart_mix(&self, callback: impl Fn(TopGenre) + 'static) {
        *self.on_create_smart_mix.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_unify_spellings(&self, callback: impl Fn(Vec<i64>) + 'static) {
        *self.on_unify_spellings.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_metadata_activate(
        &self,
        callback: impl Fn(StatsMetadataTarget) + 'static,
    ) {
        *self.on_metadata_activate.borrow_mut() = Some(Rc::new(callback));
    }

    /// The sections in the order the page actually stacks them, read off the
    /// live widget tree — not off a constant that nothing binds to it.
    #[cfg(test)]
    fn section_order(&self) -> Vec<&'static str> {
        let mut order = vec!["hero"];
        let page = self.page.upgrade().expect("stats page must be alive");
        let stack = page
            .last_child()
            .expect("stats page must own its content stack")
            .downcast::<gtk4::Stack>()
            .expect("last stats page child must be its content stack");
        let sections = stack
            .child_by_name("sections")
            .expect("stats content stack must own its sections page");
        let mut child = sections.first_child();
        while let Some(widget) = child {
            order.push(self.section_name(&widget));
            child = widget.next_sibling();
        }
        order
    }

    #[cfg(test)]
    fn section_name(&self, widget: &gtk4::Widget) -> &'static str {
        let render = &self.render;
        if render.ribbon.widget().is_ancestor(widget) {
            "ribbon"
        } else if render.spotlight_section.widget().is_ancestor(widget) {
            "spotlight"
        } else if render.genres_section_data.widget().is_ancestor(widget) {
            "genres"
        } else if render.clock_section_data.widget().is_ancestor(widget)
            && render.highlights_section_data.widget().is_ancestor(widget)
        {
            "clock-highlights"
        } else if render.top_tracks_box.is_ancestor(widget) {
            "top-tracks"
        } else {
            "unknown"
        }
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
    on_metadata_activate: MetadataCallback,
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
            render_hero(render, &snapshot, period);
            // The stack decides what is on screen; hiding sections inside the
            // page it just switched away from changes nothing.
            if snapshot.is_empty() {
                page_stack.set_visible_child_name("empty");
            } else {
                render_snapshot(render, &snapshot);
                page_stack.set_visible_child_name("sections");
            }
            *current_snapshot.borrow_mut() = Some(snapshot);
        }
        Err(error) => {
            tracing::error!(%error, "failed to compute My Stats snapshot");
            page_stack.set_visible_child_name("failed");
            *current_snapshot.borrow_mut() = None;
        }
    }
}

fn render_snapshot(render: &RenderParts, snapshot: &StatsSnapshot) {
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
    render
        .highlights_section_data
        .set_top_genre(top_genre(&snapshot.genres));
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
        &render.on_metadata_activate,
    );
}

fn render_hero(render: &RenderParts, snapshot: &StatsSnapshot, period: StatsPeriod) {
    render
        .hero_time
        .set_label(&strings::hero_listening_time(snapshot.hero.total_ms));
    render_comparison(render, snapshot.hero.comparison_presentation, period);
    render.hero_subline.set_label(&format!(
        "{} plays \u{00b7} \u{00d8} {} min/day \u{00b7} {} artists",
        format_thousands(snapshot.hero.plays),
        snapshot.hero.average_ms_per_day / 60_000,
        format_thousands(snapshot.hero.artists)
    ));
}

fn render_comparison(
    render: &RenderParts,
    presentation: Option<ComparisonPresentation>,
    period: StatsPeriod,
) {
    let copy = presentation.and_then(|value| strings::comparison_copy(value, period));
    if let Some(copy) = copy {
        render.comparison_pill.set_label(&copy.pill);
        render.comparison_pill.set_tooltip_text(Some(&copy.tooltip));
        render.comparison_pill.set_visible(true);
    } else {
        render.comparison_pill.set_visible(false);
        render.comparison_pill.set_tooltip_text(None);
    }
}

fn build_sort_controls(
    tracks_box: &gtk4::Box,
    snapshot: &Rc<RefCell<Option<StatsSnapshot>>>,
    sort_by: &Rc<Cell<SortBy>>,
    cover_loader: &Rc<CoverLoader>,
    generation: &Rc<Cell<u64>>,
    on_metadata_activate: &MetadataCallback,
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
            let on_metadata_activate = on_metadata_activate.clone();
            move |button| {
                if !button.is_active() {
                    return;
                }
                sort_by.set(value);
                let snapshot = snapshot.borrow().clone();
                if let Some(snapshot) = snapshot {
                    render_tracks(
                        &tracks_box,
                        &snapshot,
                        value,
                        &cover_loader,
                        &generation,
                        &on_metadata_activate,
                    );
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
    on_metadata_activate: &MetadataCallback,
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
        text.append(&stats_metadata_links::button(
            &track.title,
            "stats-item-title",
            StatsMetadataTarget::Track(track.track_id),
            on_metadata_activate,
        ));
        text.append(&stats_metadata_links::button(
            &track.artist,
            "stats-item-subtitle",
            StatsMetadataTarget::Artist {
                track_id: track.track_id,
                artist: track.effective_artist.clone(),
            },
            on_metadata_activate,
        ));
        text.append(&stats_metadata_links::button(
            &track.album,
            "stats-item-subtitle",
            StatsMetadataTarget::Album {
                track_id: track.track_id,
                album: track.album.clone(),
                album_artist: track.effective_artist.clone(),
            },
            on_metadata_activate,
        ));
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
                format_thousands(track.play_count),
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

/// The strongest real genre of the period. The bundled "Other" segment is not
/// a genre group and has no tracks of its own to mix from.
fn top_genre(section: &reprise_core::library::stats_snapshot::GenreSection) -> Option<TopGenre> {
    section
        .segments
        .iter()
        .find(|segment| segment.label != "Other")
        .map(TopGenre::from_segment)
}

fn metric(track: &TopTrack, sort_by: SortBy) -> i64 {
    match sort_by {
        SortBy::Plays => track.play_count,
        SortBy::Time => track.total_ms,
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
#[path = "stats_view_tests.rs"]
mod tests;
