//! Editorial My Stats composer and refresh orchestration.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Datelike;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::db::Db;
use reprise_core::library::group_key::GroupKind;
use reprise_core::library::stats_period::StatsPeriod;
use reprise_core::library::stats_screen::group_track_ids;
use reprise_core::library::stats_snapshot::{self, StatsSnapshot};

use super::stats_band_card::StatsBandCard;
use super::stats_entrance::{HorizontalBarGroup, StatsEntrance};
use super::stats_genre_card::StatsGenreCard;
use super::stats_header::StatsHeader;
use super::stats_hero::StatsHero;
use super::stats_metadata_links::{MetadataCallback, StatsMetadataTarget};
use super::stats_ribbon::StatsRibbon;
use super::stats_songs_card::StatsSongsCard;
use super::stats_view_widgets::card;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;

const CONTENT_MAX_WIDTH: i32 = 1120;
const SECTION_SPACING: i32 = 20;
const STORY_NATURAL_LINE_LENGTH: i32 = 960;
const STORY_SPACING: i32 = 20;
const SONGS_WIDTH: i32 = 490;
const MIN_PLAYS_FOR_TREND: i64 = 10;
/// The fixed editorial order of the page's sections (STATS-10). The test reads
/// the real widget tree and compares against this.
#[cfg(test)]
const SECTION_ORDER: [&str; 6] = [
    "header",
    "hero",
    "chart",
    "band-songs",
    "top-tracks",
    "genres",
];

type StringCallback = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;
type IdsCallback = Rc<RefCell<Option<Rc<dyn Fn(Vec<i64>)>>>>;
#[derive(Clone)]
pub(in crate::ui) struct StatsView {
    root: gtk4::ScrolledWindow,
    page_stack: gtk4::Stack,
    period_dropdown: gtk4::DropDown,
    period_model: gtk4::StringList,
    periods: Rc<RefCell<Vec<StatsPeriod>>>,
    wired: Cell<bool>,
    entrance_pending: Rc<Cell<bool>>,
    connection: Rc<RefCell<Option<Rc<Db>>>>,
    #[cfg_attr(not(test), allow(dead_code))]
    page: glib::WeakRef<gtk4::Box>,
    #[cfg_attr(not(test), allow(dead_code))]
    story_row: glib::WeakRef<adw::WrapBox>,
    #[cfg_attr(not(test), allow(dead_code))]
    hero_row: glib::WeakRef<adw::WrapBox>,
    #[cfg_attr(not(test), allow(dead_code))]
    hero_time_row: glib::WeakRef<gtk4::Box>,
    current_snapshot: Rc<RefCell<Option<StatsSnapshot>>>,
    /// Built once and shared: the period dropdown's handler holds it weakly,
    /// which is what keeps the handler from owning the page it lives in.
    render: Rc<RenderParts>,
    on_go_to_artist: StringCallback,
    on_unify_spellings: IdsCallback,
    on_metadata_activate: MetadataCallback,
}

impl StatsView {
    pub(in crate::ui) fn new(cover_loader: Rc<CoverLoader>) -> Self {
        let header = StatsHeader::new();
        let hero = StatsHero::new();
        let period_dropdown = header.period_dropdown.clone();
        let period_model = header.period_model.clone();

        let ribbon = StatsRibbon::new();
        let band_card = StatsBandCard::new();
        band_card.set_cover_loader(cover_loader.clone());
        let genres = StatsGenreCard::new(cover_loader.clone());
        let genres_section = card(genres.widget());

        let current_snapshot = Rc::new(RefCell::new(None::<StatsSnapshot>));
        let on_metadata_activate: MetadataCallback = Rc::new(RefCell::new(None));
        let songs_card = StatsSongsCard::new(cover_loader, on_metadata_activate.clone());
        let band_section = card(band_card.widget());
        band_section.set_hexpand(true);
        let songs_section = card(songs_card.widget());
        songs_section.set_width_request(SONGS_WIDTH);
        songs_section.set_hexpand(true);
        let story_row = adw::WrapBox::new();
        story_row.set_child_spacing(STORY_SPACING);
        story_row.set_line_spacing(STORY_SPACING);
        story_row.set_natural_line_length(STORY_NATURAL_LINE_LENGTH);
        story_row.set_wrap_policy(adw::WrapPolicy::Natural);
        story_row.set_justify(adw::JustifyMode::Fill);
        story_row.set_justify_last_line(true);
        story_row.set_hexpand(true);
        story_row.append(&band_section);
        story_row.append(&songs_section);

        let sections = gtk4::Box::new(gtk4::Orientation::Vertical, SECTION_SPACING);
        let chart_card = card(ribbon.widget());
        let thin_hint = gtk4::Label::new(Some(strings::stats_thin_history_hint().as_str()));
        thin_hint.add_css_class("stats-thin-history");
        thin_hint.set_wrap(true);
        thin_hint.set_xalign(0.0);
        let hint_card = card(&thin_hint);
        let trend_stack = gtk4::Stack::new();
        trend_stack.set_vhomogeneous(false);
        trend_stack.add_named(&chart_card, Some("chart"));
        trend_stack.add_named(&hint_card, Some("hint"));
        trend_stack.set_visible_child_name("chart");
        sections.append(&trend_stack);
        sections.append(&story_row);
        sections.append(songs_card.expanded_widget());
        sections.append(&genres_section);

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
        page_stack.set_vhomogeneous(false);
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
        page.append(&header.root);
        page.append(&hero.root);
        page.append(&page_stack);
        // Tighten only at the maximum width. Adw::Clamp otherwise starts
        // squeezing at its 400px default threshold, which costs the story row
        // enough width to wrap band and songs onto separate lines in a window
        // that is plainly wide enough for both.
        let clamp = adw::Clamp::builder()
            .maximum_size(CONTENT_MAX_WIDTH)
            .tightening_threshold(CONTENT_MAX_WIDTH)
            .child(&page)
            .build();
        let root = gtk4::ScrolledWindow::builder()
            .child(&clamp)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .hexpand(true)
            .vexpand(true)
            .build();

        let connection = Rc::new(RefCell::new(None::<Rc<Db>>));
        let on_go_to_artist: StringCallback = Rc::new(RefCell::new(None));
        let on_unify_spellings: IdsCallback = Rc::new(RefCell::new(None));
        let entrance_pending = Rc::new(Cell::new(false));
        let entrance = StatsEntrance::default();

        band_card.set_on_open_artist({
            let callback = on_go_to_artist.clone();
            move |artist| {
                let callback = callback.borrow().clone();
                if let Some(callback) = callback {
                    callback(artist);
                }
            }
        });
        wire_unify(&band_card, &genres, &connection, &on_unify_spellings);

        let render = Rc::new(RenderParts {
            header: header.clone(),
            hero: hero.clone(),
            ribbon: ribbon.clone(),
            band_card: band_card.clone(),
            genres_section_data: genres.clone(),
            songs_card: songs_card.clone(),
            trend_stack: trend_stack.clone(),
            band_section: band_section.clone(),
            songs_section: songs_section.clone(),
            top_tracks_section: songs_card.expanded_widget().clone(),
            genres_section: genres_section.clone(),
            entrance,
        });

        root.connect_map({
            let current_snapshot = current_snapshot.clone();
            let entrance_pending = entrance_pending.clone();
            let render = Rc::downgrade(&render);
            move |root| {
                if !entrance_pending.replace(false) {
                    return;
                }
                let current_snapshot = current_snapshot.clone();
                let render = render.clone();
                root.add_tick_callback(move |_, _| {
                    let snapshot = current_snapshot.borrow().clone();
                    let Some(snapshot) = snapshot else {
                        return glib::ControlFlow::Break;
                    };
                    let Some(render) = render.upgrade() else {
                        return glib::ControlFlow::Break;
                    };
                    if !snapshot.is_empty() {
                        render_snapshot(&render, &snapshot, true);
                    }
                    glib::ControlFlow::Break
                });
            }
        });

        Self {
            root,
            page_stack,
            period_dropdown,
            period_model,
            periods: Rc::new(RefCell::new(Vec::new())),
            wired: Cell::new(false),
            entrance_pending,
            connection,
            page: page.downgrade(),
            story_row: story_row.downgrade(),
            hero_row: hero.root.downgrade(),
            hero_time_row: hero.time_block.downgrade(),
            current_snapshot,
            render,
            on_go_to_artist,
            on_unify_spellings,
            on_metadata_activate,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::ScrolledWindow {
        &self.root
    }

    pub(in crate::ui) fn wire_year_selector(&self, conn: &Rc<Db>) {
        *self.connection.borrow_mut() = Some(conn.clone());
        let now_year = chrono::Local::now().year();
        let periods = {
            let conn = &conn;
            StatsPeriod::available(conn, now_year, &chrono::Local).unwrap_or_else(|error| {
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
        *self.periods.borrow_mut() = periods;
        self.period_model
            .splice(0, self.period_model.n_items(), &label_refs);
        self.period_dropdown.set_selected(0);

        if !self.wired.replace(true) {
            let connection = self.connection.clone();
            let periods = self.periods.clone();
            let current_snapshot = self.current_snapshot.clone();
            let entrance_pending = self.entrance_pending.clone();
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
                        refresh_parts(
                            &conn,
                            period,
                            &page_stack,
                            &current_snapshot,
                            &render,
                            &entrance_pending,
                        );
                    }
                }
            ));
        }
        self.refresh(conn);
    }

    pub(in crate::ui) fn refresh(&self, conn: &Rc<Db>) {
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
            &self.entrance_pending,
        );
    }

    pub(in crate::ui) fn prepare_entrance(&self) {
        self.entrance_pending.set(true);
    }

    pub(in crate::ui) fn set_on_go_to_artist(&self, callback: impl Fn(String) + 'static) {
        *self.on_go_to_artist.borrow_mut() = Some(Rc::new(callback));
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

    pub(in crate::ui) fn set_on_play_track(&self, callback: impl Fn(i64) + 'static) {
        self.render.songs_card.set_on_play_track(callback);
    }

    pub(in crate::ui) fn set_on_play_next(&self, callback: impl Fn(i64) + 'static) {
        self.render.songs_card.set_on_play_next(callback);
    }

    pub(in crate::ui) fn set_on_add_to_queue(&self, callback: impl Fn(i64) + 'static) {
        self.render.songs_card.set_on_add_to_queue(callback);
    }

    pub(in crate::ui) fn set_on_genre_album(&self, callback: impl Fn(String) + 'static) {
        self.render
            .genres_section_data
            .set_on_open_album_path(callback);
    }

    pub(in crate::ui) fn set_on_go_to_genre(&self, callback: impl Fn(String) + 'static) {
        self.render.genres_section_data.set_on_open_genre(callback);
    }

    /// The sections in the order the page actually stacks them, read off the
    /// live widget tree — not off a constant that nothing binds to it.
    #[cfg(test)]
    fn section_order(&self) -> Vec<&'static str> {
        let mut order = Vec::new();
        let page = self.page.upgrade().expect("stats page must be alive");
        let mut child = page.first_child();
        let stack = loop {
            let widget = child.expect("stats page must own its content stack");
            if self.period_dropdown.is_ancestor(&widget) {
                order.push("header");
            } else if self.render.hero.time.is_ancestor(&widget) {
                order.push("hero");
            } else if let Ok(stack) = widget.clone().downcast::<gtk4::Stack>() {
                break stack;
            } else {
                order.push("unknown");
            }
            child = widget.next_sibling();
        };
        let sections = stack
            .child_by_name("sections")
            .expect("stats content stack must own its sections page");
        let mut child = sections.first_child();
        while let Some(widget) = child {
            if self.render.ribbon.widget().is_ancestor(&widget) {
                order.push("chart");
            } else if self.render.band_section.is_ancestor(&widget)
                && self.render.songs_section.is_ancestor(&widget)
            {
                order.push("band-songs");
            } else if widget
                == self
                    .render
                    .top_tracks_section
                    .clone()
                    .upcast::<gtk4::Widget>()
            {
                order.push("top-tracks");
            } else if self
                .render
                .genres_section_data
                .widget()
                .is_ancestor(&widget)
            {
                order.push("genres");
            } else {
                order.push("unknown");
            }
            child = widget.next_sibling();
        }
        order
    }
}

#[derive(Clone)]
struct RenderParts {
    header: StatsHeader,
    hero: StatsHero,
    ribbon: StatsRibbon,
    band_card: StatsBandCard,
    genres_section_data: StatsGenreCard,
    songs_card: StatsSongsCard,
    trend_stack: gtk4::Stack,
    band_section: gtk4::Box,
    songs_section: gtk4::Box,
    #[cfg_attr(not(test), allow(dead_code))]
    top_tracks_section: gtk4::Revealer,
    genres_section: gtk4::Box,
    entrance: StatsEntrance,
}

fn refresh_parts(
    conn: &Rc<Db>,
    period: StatsPeriod,
    page_stack: &gtk4::Stack,
    current_snapshot: &Rc<RefCell<Option<StatsSnapshot>>>,
    render: &RenderParts,
    entrance_pending: &Cell<bool>,
) {
    let now_unix = now_unix();
    let result = {
        let conn = &conn;
        stats_snapshot::compute(conn, period, now_unix, &chrono::Local)
    };
    match result {
        Ok(snapshot) => {
            render.hero.set_data(&snapshot, period, &render.header);
            let entrance = entrance_pending.replace(false);
            // The stack decides what is on screen; hiding sections inside the
            // page it just switched away from changes nothing.
            if snapshot.is_empty() {
                page_stack.set_visible_child_name("empty");
            } else {
                let mapped_entrance = entrance && page_stack.is_mapped();
                render_snapshot(render, &snapshot, mapped_entrance);
                if entrance && !mapped_entrance {
                    entrance_pending.set(true);
                }
                page_stack.set_visible_child_name("sections");
            }
            *current_snapshot.borrow_mut() = Some(snapshot);
        }
        Err(error) => {
            tracing::error!(%error, "failed to compute My Stats snapshot");
            render.hero.clear(&render.header);
            page_stack.set_visible_child_name("failed");
            *current_snapshot.borrow_mut() = None;
        }
    }
}

fn render_snapshot(render: &RenderParts, snapshot: &StatsSnapshot, entrance: bool) {
    let ribbon_values = snapshot
        .ribbon
        .iter()
        .map(|point| point.total_ms)
        .collect::<Vec<_>>();
    render.ribbon.set_data(
        &snapshot.period,
        &ribbon_values,
        snapshot.best_week.as_ref(),
    );
    if let Some(spotlight) = &snapshot.spotlight {
        render.band_card.set_data(spotlight);
        render.band_section.set_visible(true);
    } else {
        render.band_card.clear_data();
        render.band_section.set_visible(false);
    }
    render.genres_section_data.set_data(&snapshot.genres);
    render
        .genres_section
        .set_visible(!snapshot.genres.segments.is_empty());
    render
        .songs_section
        .set_visible(!snapshot.top_tracks.is_empty());
    render.songs_card.set_data(snapshot);
    let thin = snapshot.hero.plays < MIN_PLAYS_FOR_TREND;
    render
        .trend_stack
        .set_visible_child_name(if thin { "hint" } else { "chart" });
    let groups = [
        HorizontalBarGroup::new(render.band_card.bars(), Vec::new()),
        HorizontalBarGroup::new(render.songs_card.summary_bars(), Vec::new()),
        HorizontalBarGroup::new(Vec::new(), render.genres_section_data.segment_reveals()),
    ];
    render.entrance.update(
        &render.ribbon,
        &groups,
        &render.genres_section_data,
        entrance,
    );
}

fn wire_unify(
    band_card: &StatsBandCard,
    genres: &StatsGenreCard,
    connection: &Rc<RefCell<Option<Rc<Db>>>>,
    callback: &IdsCallback,
) {
    band_card.set_on_unify({
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
    connection: &Rc<RefCell<Option<Rc<Db>>>>,
    callback: &IdsCallback,
    kind: GroupKind,
    key: &str,
) {
    let connection = connection.borrow().clone();
    let Some(connection) = connection else { return };
    let ids = {
        let connection = &connection;
        group_track_ids(connection, kind, key)
    };
    match ids {
        Ok(ids) if !ids.is_empty() => {
            let callback = callback.borrow().clone();
            if let Some(callback) = callback {
                callback(ids);
            }
        }
        Ok(_) => {}
        Err(error) => tracing::error!(%error, "failed to resolve stats group tracks"),
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
#[cfg(test)]
#[path = "stats_view_entrance_tests.rs"]
mod entrance_tests;
#[cfg(test)]
#[path = "stats_view_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "stats_view_unify_tests.rs"]
mod unify_tests;
