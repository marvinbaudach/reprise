//! Full-width songs card: a two-column top ten and its expandable ranking.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::cover::ThumbnailSize;
use reprise_core::format::format_thousands;
use reprise_core::library::stats_screen::TopTrack;
use reprise_core::library::stats_snapshot::{SortBy, StatsSnapshot};
use reprise_core::playback::PlaybackState;

use super::stats_metadata_links::{self, MetadataCallback, StatsMetadataTarget};
use super::stats_songs_playback::{SongRowPlayback, TrackMark};
use super::stats_songs_row_actions::RowActions;
use super::stats_view_widgets::{clear, label};
use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;

/// The card shows a full top ten split over two columns, so the ranking a
/// user actually reads no longer needs the expander.
pub(super) const SONG_ROW_LIMIT: usize = 10;
const SUMMARY_COLUMN_ROWS: usize = 5;
/// How many further tracks the expander adds *below* the visible top ten. It
/// continues the ranking rather than restating it: someone who opens it has
/// already read rows 1-10 and wants what comes next.
const FULL_TRACK_EXTRA: usize = 15;

/// The loaded track as both song lists see it. Shared by `Rc` so a mark
/// arriving between renders reaches the rows that already exist and the rows
/// built afterwards alike — a re-render reads this, it never re-derives it.
type SharedMark = Rc<Cell<Option<TrackMark>>>;
type RowPlaybacks = Rc<RefCell<Vec<Rc<SongRowPlayback>>>>;

/// What one render of the continuation fills: its rows' playback markers and
/// the controllers those rows must outlive.
#[derive(Clone, Default)]
struct ContinuationParts {
    playbacks: RowPlaybacks,
    row_clicks: Rc<RefCell<Vec<gtk4::GestureClick>>>,
    context_actions: Rc<RefCell<Vec<gio::SimpleActionGroup>>>,
}

#[derive(Clone, Default)]
struct CoverGenerations {
    summary: Rc<Cell<u64>>,
    full: Rc<Cell<u64>>,
}

impl CoverGenerations {
    fn next_summary(&self) -> u64 {
        next_generation(&self.summary)
    }

    fn next_full(&self) -> u64 {
        next_generation(&self.full)
    }
}

#[derive(Clone)]
struct SummaryRenderer {
    #[cfg_attr(not(test), allow(dead_code))]
    rows: gtk4::Box,
    columns: [gtk4::Box; 2],
    bars: Rc<RefCell<Vec<gtk4::LevelBar>>>,
    row_clicks: Rc<RefCell<Vec<gtk4::GestureClick>>>,
    cover_loader: Rc<CoverLoader>,
    generations: CoverGenerations,
    context_actions: Rc<RefCell<Vec<gio::SimpleActionGroup>>>,
    mark: SharedMark,
    playbacks: RowPlaybacks,
    actions: RowActions,
}

#[derive(Clone)]
pub(in crate::ui) struct StatsSongsCard {
    root: gtk4::Box,
    summary: SummaryRenderer,
    full: ContinuationParts,
    #[cfg_attr(not(test), allow(dead_code))]
    revealer: gtk4::Revealer,
    #[cfg_attr(not(test), allow(dead_code))]
    reveal_button: gtk4::Button,
    full_rows: gtk4::Box,
    #[cfg_attr(not(test), allow(dead_code))]
    sort_toggle: adw::ToggleGroup,
    snapshot: Rc<RefCell<Option<StatsSnapshot>>>,
    sort_by: Rc<Cell<SortBy>>,
}

impl StatsSongsCard {
    pub(in crate::ui) fn new(cover_loader: Rc<CoverLoader>, metadata: MetadataCallback) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        root.add_css_class("stats-songs-card");
        let kicker = label("MOST PLAYED SONGS", "stats-eyebrow");
        let plays_sort = adw::Toggle::builder()
            .name("plays")
            .label("by plays")
            .build();
        let time_sort = adw::Toggle::builder().name("time").label("by time").build();
        let sort_toggle = adw::ToggleGroup::new();
        sort_toggle.add(plays_sort);
        sort_toggle.add(time_sort);
        sort_toggle.set_active_name(Some("plays"));
        sort_toggle.set_halign(gtk4::Align::End);
        // a11y-semantics: role=group name=sort-top-tracks state=one-selected action=arrow-keys
        sort_toggle.update_property(&[gtk4::accessible::Property::Label("Sort top tracks")]);
        // input-parity: ACC-8 keyboard=sort-toggle-arrows
        // AdwToggleGroup owns the focusable pill buttons and native arrow-key
        // selection, with one selected item exposed through its group label.
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        kicker.set_hexpand(true);
        header.append(&kicker);
        header.append(&sort_toggle);
        root.append(&header);

        // Two columns rather than one long list: at full width a single
        // ten-row column would leave most of the card empty and push the
        // genres below the fold.
        let rows = gtk4::Box::new(gtk4::Orientation::Horizontal, 24);
        rows.set_homogeneous(true);
        let columns = [
            gtk4::Box::new(gtk4::Orientation::Vertical, 2),
            gtk4::Box::new(gtk4::Orientation::Vertical, 2),
        ];
        for column in &columns {
            column.set_hexpand(true);
            rows.append(column);
        }
        root.append(&rows);

        let reveal_button = gtk4::Button::with_label("Show more top tracks");
        reveal_button.add_css_class("flat");
        reveal_button.add_css_class("stats-songs-reveal");
        reveal_button.set_halign(gtk4::Align::Start);
        root.append(&reveal_button);

        // STATS-22: the continuation belongs to the ranking it continues, so
        // it grows this card instead of opening a second one below it. The
        // rows keep the column rhythm above them, which is what lets ranks 10
        // and 11 read as neighbours rather than as two lists.
        let full_rows = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        full_rows.set_hexpand(true);
        let revealer = gtk4::Revealer::new();
        revealer.set_reveal_child(false);
        revealer.set_child(Some(&full_rows));
        // A collapsed revealer is still a visible box child, and the card puts
        // spacing between visible children — so it leaves the card entirely
        // until it has something to show.
        revealer.set_visible(false);
        revealer.connect_child_revealed_notify(|revealer| {
            if !revealer.is_child_revealed() && !revealer.reveals_child() {
                revealer.set_visible(false);
            }
        });
        root.append(&revealer);

        let snapshot = Rc::new(RefCell::new(None::<StatsSnapshot>));
        let sort_by = Rc::new(Cell::new(SortBy::Plays));
        let cover_generations = CoverGenerations::default();
        let context_actions = Rc::new(RefCell::new(Vec::new()));
        let summary_bars = Rc::new(RefCell::new(Vec::new()));
        let row_clicks = Rc::new(RefCell::new(Vec::new()));
        let full = ContinuationParts::default();
        let summary = SummaryRenderer {
            rows,
            columns,
            bars: summary_bars,
            row_clicks,
            cover_loader,
            generations: cover_generations,
            context_actions,
            mark: Rc::new(Cell::new(None)),
            playbacks: Rc::new(RefCell::new(Vec::new())),
            actions: RowActions::new(metadata),
        };

        let expanded = summary.actions.play_context.expanded.clone();
        reveal_button.connect_clicked(glib::clone!(
            #[weak]
            revealer,
            #[strong]
            expanded,
            move |button| {
                let reveal = !revealer.reveals_child();
                // The revealed ranks join the ranking a play hands over the
                // moment they are on screen, and leave it again when they go.
                expanded.set(reveal);
                if reveal {
                    // A hidden revealer cannot animate. Join the section flow
                    // before starting the transition; collapse removes it only
                    // from `child-revealed` after the animation finishes.
                    revealer.set_visible(true);
                    revealer.set_reveal_child(true);
                } else {
                    revealer.set_reveal_child(false);
                }
                button.set_label(if reveal {
                    "Hide more top tracks"
                } else {
                    "Show more top tracks"
                });
            }
        ));

        sort_toggle.connect_active_name_notify({
            let full_rows = full_rows.clone();
            let snapshot = snapshot.clone();
            let sort_by = sort_by.clone();
            let summary = summary.clone();
            let full = full.clone();
            move |toggle| {
                let active_name = toggle.active_name();
                let value = sort_for_toggle_name(active_name.as_deref());
                sort_by.set(value);
                let snapshot = snapshot.borrow().clone();
                if let Some(snapshot) = snapshot {
                    summary.render(&snapshot, value);
                    render_full_rows(&full_rows, &snapshot, value, &summary, &full);
                }
            }
        });

        Self {
            root,
            summary,
            full,
            revealer,
            reveal_button,
            full_rows,
            sort_toggle,
            snapshot,
            sort_by,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// The continuation lives inside the card (STATS-22); only the tests still
    /// need a handle on it, to prove exactly that.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(in crate::ui) fn expanded_widget(&self) -> &gtk4::Revealer {
        &self.revealer
    }

    pub(in crate::ui) fn set_data(&self, snapshot: &StatsSnapshot) {
        // The expander only continues the ranking, so it is offered exactly
        // when there is something past the visible ten — otherwise it would
        // open onto nothing.
        let has_more = snapshot.top_tracks.len() > SONG_ROW_LIMIT;
        self.reveal_button.set_visible(has_more);
        if !has_more {
            self.revealer.set_reveal_child(false);
            self.summary.actions.play_context.expanded.set(false);
            self.reveal_button.set_label("Show more top tracks");
        }
        *self.snapshot.borrow_mut() = Some(snapshot.clone());
        self.summary.render(snapshot, self.sort_by.get());
        render_full_rows(
            &self.full_rows,
            snapshot,
            self.sort_by.get(),
            &self.summary,
            &self.full,
        );
    }

    /// Points the shared playback marker at `mark` (or clears it). Deliberately
    /// does **not** re-render: a track change would otherwise rebuild both
    /// lists, throwing away the expanded state and scroll position. The
    /// already-built rows are mutated in place, which is the same
    /// viewport-neutral discipline NAV-10a imposes on the track table.
    fn set_mark(&self, mark: Option<TrackMark>) {
        self.summary.mark.set(mark);
        for playback in self.summary.playbacks.borrow().iter() {
            playback.set_mark(mark);
        }
        for playback in self.full.playbacks.borrow().iter() {
            playback.set_mark(mark);
        }
    }

    /// A track was loaded — it becomes the marked row and starts out running.
    pub(super) fn set_loaded_track(&self, track_id: i64) {
        self.set_mark(Some(TrackMark {
            track_id,
            playing: true,
        }));
    }

    /// Playback ran, paused, or stopped. Which of those keeps the mark is
    /// decided in one place, see `stats_songs_playback::mark_for_state`.
    pub(super) fn set_playback_state(&self, state: PlaybackState) {
        self.set_mark(super::stats_songs_playback::mark_for_state(
            self.summary.mark.get(),
            state,
        ));
    }
}

fn sort_for_toggle_name(name: Option<&str>) -> SortBy {
    if name == Some("time") {
        SortBy::Time
    } else {
        SortBy::Plays
    }
}

impl SummaryRenderer {
    fn render(&self, snapshot: &StatsSnapshot, sort_by: SortBy) {
        for column in &self.columns {
            clear(column);
        }
        self.context_actions.borrow_mut().clear();
        self.bars.borrow_mut().clear();
        self.row_clicks.borrow_mut().clear();
        self.playbacks.borrow_mut().clear();
        let tracks = snapshot.top_tracks_sorted(sort_by);
        let leader = tracks.first().map_or(0, |track| metric(track, sort_by));
        let token = self.generations.next_summary();
        // The ranking the rows hand over when they play, rebuilt every render
        // so a play never seeds a queue in yesterday's order. The continuation
        // appends its own ranks right after (STATS-22).
        {
            let mut ranking = self.actions.play_context.ranking.borrow_mut();
            ranking.clear();
            ranking.extend(
                tracks
                    .iter()
                    .take(SONG_ROW_LIMIT)
                    .map(|track| track.track_id),
            );
        }
        for (index, track) in tracks.iter().take(SONG_ROW_LIMIT).enumerate() {
            let row = self.song_row(track, index, leader, sort_by, token);
            self.columns[index / SUMMARY_COLUMN_ROWS].append(&row);
        }
    }

    fn song_row(
        &self,
        track: &TopTrack,
        index: usize,
        leader: i64,
        sort_by: SortBy,
        token: u64,
    ) -> gtk4::Box {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        row.add_css_class("stats-song-row");
        row.set_hexpand(true);

        // `STATS-18`: the rank slot is the marker slot — the loaded row shows
        // the shared equaliser where every other row shows its number.
        let rank_slot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        rank_slot.add_css_class("stats-song-rank-slot");
        rank_slot.set_size_request(22, 16);
        rank_slot.set_halign(gtk4::Align::End);
        rank_slot.set_valign(gtk4::Align::Center);
        let rank = label(&(index + 1).to_string(), "stats-rank");
        rank.set_xalign(1.0);
        rank_slot.append(&rank);
        row.append(&rank_slot);

        let cover = gtk4::Image::builder()
            .pixel_size(42)
            .width_request(42)
            .height_request(42)
            .build();
        cover.add_css_class("stats-song-cover");
        CoverLoader::set_placeholder(&cover);
        self.cover_loader.load_into(
            &cover,
            &track.track_path,
            ThumbnailSize::List,
            token,
            &self.generations.summary,
        );
        row.append(&cover);

        let text = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        text.set_hexpand(true);
        text.set_valign(gtk4::Align::Center);
        // The row plays; the two labels stay the way into the library, so a
        // list that starts playback does not also lose its navigation.
        let title = stats_metadata_links::link(
            &track.title,
            "stats-item-title",
            StatsMetadataTarget::Track(track.track_id),
            &self.actions.metadata,
        );
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        text.append(&title);
        let artist = stats_metadata_links::link(
            &track.artist,
            "stats-item-subtitle",
            StatsMetadataTarget::Artist {
                track_id: track.track_id,
                artist: track.effective_artist.clone(),
            },
            &self.actions.metadata,
        );
        artist.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        text.append(&artist);
        row.append(&text);

        let bar = gtk4::LevelBar::new();
        bar.add_css_class("stats-song-bar");
        bar.set_min_value(0.0);
        bar.set_max_value(1.0);
        bar.set_value(relative_value(metric(track, sort_by), leader));
        bar.set_width_request(110);
        bar.set_height_request(8);
        bar.set_valign(gtk4::Align::Center);
        self.bars.borrow_mut().push(bar.clone());
        row.append(&bar);

        let value = label(&metric_text(track, sort_by), "stats-play-count");
        value.set_xalign(1.0);
        value.set_width_request(72);
        row.append(&value);

        let playback = SongRowPlayback::new(&row, &rank_slot, &rank, &title, &bar, track.track_id);
        playback.set_mark(self.mark.get());
        self.playbacks.borrow_mut().push(playback);

        let (activate, actions) = self.actions.attach(&row, track, index);
        self.row_clicks.borrow_mut().push(activate);
        self.context_actions.borrow_mut().push(actions);
        row
    }
}

impl StatsSongsCard {
    pub(in crate::ui) fn set_on_play_track(&self, callback: impl Fn(&[i64], usize) + 'static) {
        *self.summary.actions.on_play_track.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_play_next(&self, callback: impl Fn(i64) + 'static) {
        *self.summary.actions.on_play_next.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_add_to_queue(&self, callback: impl Fn(i64) + 'static) {
        *self.summary.actions.on_add_to_queue.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn summary_bars(&self) -> Vec<gtk4::LevelBar> {
        self.summary.bars.borrow().clone()
    }
}

fn render_full_rows(
    container: &gtk4::Box,
    snapshot: &StatsSnapshot,
    sort_by: SortBy,
    summary: &SummaryRenderer,
    parts: &ContinuationParts,
) {
    clear(container);
    parts.playbacks.borrow_mut().clear();
    parts.row_clicks.borrow_mut().clear();
    parts.context_actions.borrow_mut().clear();
    let generations = &summary.generations;
    let metadata = &summary.actions.metadata;
    let token = generations.next_full();
    let tracks = snapshot.top_tracks_sorted(sort_by);
    let leader = tracks.first().map_or(0, |track| metric(track, sort_by));
    // The continuation appends its ranks to the ranking the visible ten just
    // wrote — one list, so rank 11 can play at the index it actually holds.
    {
        let mut ranking = summary.actions.play_context.ranking.borrow_mut();
        ranking.truncate(SONG_ROW_LIMIT);
        ranking.extend(
            tracks
                .iter()
                .skip(SONG_ROW_LIMIT)
                .take(FULL_TRACK_EXTRA)
                .map(|track| track.track_id),
        );
    }
    for (offset, track) in tracks
        .iter()
        .skip(SONG_ROW_LIMIT)
        .take(FULL_TRACK_EXTRA)
        .enumerate()
    {
        // Ranks continue from the card above, so the two lists read as one.
        let index = offset + SONG_ROW_LIMIT;
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        row.add_css_class("stats-top-track-row");
        row.set_height_request(56);
        // `STATS-18`: NAV-10a wants *every* visible instance of the loaded
        // track marked, so the expanded ranking marks its rank slot too.
        let rank_slot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        rank_slot.add_css_class("stats-song-rank-slot");
        rank_slot.set_size_request(24, 16);
        rank_slot.set_halign(gtk4::Align::End);
        rank_slot.set_valign(gtk4::Align::Center);
        let rank = label(&(index + 1).to_string(), "stats-rank");
        rank.set_xalign(1.0);
        rank_slot.append(&rank);
        row.append(&rank_slot);
        let cover = gtk4::Image::builder()
            .pixel_size(42)
            .width_request(42)
            .height_request(42)
            .valign(gtk4::Align::Center)
            .build();
        cover.add_css_class("stats-song-cover");
        CoverLoader::set_placeholder(&cover);
        summary.cover_loader.load_into(
            &cover,
            &track.track_path,
            ThumbnailSize::List,
            token,
            &generations.full,
        );
        row.append(&cover);
        let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let title = stats_metadata_links::link(
            &track.title,
            "stats-item-title",
            StatsMetadataTarget::Track(track.track_id),
            metadata,
        );
        text.append(&title);
        text.append(&stats_metadata_links::link(
            &track.artist,
            "stats-item-subtitle",
            StatsMetadataTarget::Artist {
                track_id: track.track_id,
                artist: track.effective_artist.clone(),
            },
            metadata,
        ));
        row.append(&text);
        let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        bar.add_css_class("stats-compact-song-bar");
        bar.set_size_request(120, 8);
        bar.set_valign(gtk4::Align::Center);
        bar.set_accessible_role(gtk4::AccessibleRole::Presentation);
        let fill = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        fill.add_css_class("stats-compact-song-bar-fill");
        fill.set_size_request(
            (120.0 * relative_value(metric(track, sort_by), leader)).round() as i32,
            8,
        );
        bar.append(&fill);
        row.append(&bar);
        row.append(&label(
            &format!(
                "{} plays · {}",
                format_thousands(track.play_count),
                strings::stats_duration(track.total_ms)
            ),
            "stats-play-count",
        ));
        let playback = SongRowPlayback::new(&row, &rank_slot, &rank, &title, &bar, track.track_id);
        playback.set_mark(summary.mark.get());
        parts.playbacks.borrow_mut().push(playback);
        // STATS-22: the continuation row answers like the ten above it —
        // click and Enter play it inside the ranking, Shift+F10 and the right
        // button open the same three actions.
        let (activate, actions) = summary.actions.attach(&row, track, index);
        parts.row_clicks.borrow_mut().push(activate);
        parts.context_actions.borrow_mut().push(actions);
        container.append(&row);
    }
}

fn next_generation(generation: &Cell<u64>) -> u64 {
    let token = generation.get().wrapping_add(1);
    generation.set(token);
    token
}

fn metric(track: &TopTrack, sort_by: SortBy) -> i64 {
    match sort_by {
        SortBy::Plays => track.play_count,
        SortBy::Time => track.total_ms,
    }
}

/// The value column follows the sort: counting by plays prints plays, ranking
/// by time prints time. Printing plays under a time sort would leave the
/// column unrelated to the bar beside it.
fn metric_text(track: &TopTrack, sort_by: SortBy) -> String {
    match sort_by {
        SortBy::Plays => format!("{} plays", format_thousands(track.play_count)),
        SortBy::Time => strings::stats_duration(track.total_ms),
    }
}

fn relative_value(value: i64, leader: i64) -> f64 {
    if leader <= 0 {
        0.0
    } else {
        value.max(0) as f64 / leader as f64
    }
}

#[cfg(test)]
#[path = "stats_songs_card_tests.rs"]
mod tests;
