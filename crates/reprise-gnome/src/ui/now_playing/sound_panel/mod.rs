//! The retained, worker-fed Sound tab in the Now Playing panel.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::sound_distance::DistanceWeights;
use reprise_core::sound_features::SoundFeatures;
use reprise_core::sound_file_info::SoundFileInfo;
use reprise_core::sound_neighbours::{
    rank_sound_neighbours, SoundNeighbourOptions, SoundNeighbourResult,
};
use reprise_core::sound_stats::{SoundStats, SoundStatsCache};

use super::super::cover_loader::CoverLoader;
use super::panel_state::{PanelTab, SOUND_PAGE, UP_NEXT_PAGE};

mod footer;
mod list;
mod profile;
#[cfg(test)]
mod tests;

pub(super) const MIN_READY_FEATURES: usize = 50;
const PROGRESS_RECHECK: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy)]
pub(super) struct SoundPanelOptions {
    pub(super) exclude_same_album: bool,
    pub(super) exclude_same_artist: bool,
    pub(super) include_tempo: bool,
    pub(super) weights: DistanceWeights,
    pub(super) limit: usize,
}

impl Default for SoundPanelOptions {
    fn default() -> Self {
        Self {
            exclude_same_album: true,
            exclude_same_artist: false,
            include_tempo: false,
            weights: DistanceWeights::DEFAULT,
            limit: 7,
        }
    }
}

#[derive(Debug, Clone)]
enum Snapshot {
    Progress {
        ready: usize,
        total: usize,
    },
    Ready {
        profile: profile::ProfilePositions,
        file_info: Option<SoundFileInfo>,
        neighbours: SoundNeighbourResult,
    },
    Error(String),
}

#[derive(Debug, Clone, Copy)]
struct Request {
    generation: u64,
    track_id: i64,
    options: SoundPanelOptions,
}

#[derive(Debug, Clone)]
struct Response {
    generation: u64,
    snapshot: Snapshot,
}

pub(super) struct SoundPanel {
    root: gtk4::Stack,
    progress_label: gtk4::Label,
    progress: gtk4::ProgressBar,
    error: gtk4::Label,
    profile: profile::Profile,
    file_info: gtk4::Label,
    matches_heading: gtk4::Label,
    matches: list::MatchList,
    footer: footer::Footer,
    request: Option<async_channel::Sender<Request>>,
    generation: Cell<u64>,
    track_id: Cell<Option<i64>>,
    options: Cell<SoundPanelOptions>,
    snapshot: RefCell<Option<Snapshot>>,
}

impl SoundPanel {
    pub(super) fn new(conn: &Rc<Db>, cover_loader: &Rc<CoverLoader>) -> Rc<Self> {
        let profile = profile::Profile::new();
        let matches = list::MatchList::new(cover_loader.clone());
        let footer = footer::Footer::new();
        let file_info = gtk4::Label::builder().xalign(0.0).wrap(true).build();
        file_info.add_css_class("reprise-sound-file-info");
        let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        let matches_heading = gtk4::Label::builder().xalign(0.0).build();
        matches_heading.add_css_class("heading");
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.set_margin_start(10);
        content.set_margin_end(10);
        content.append(profile.widget());
        content.append(&file_info);
        content.append(&separator);
        content.append(&matches_heading);
        content.append(matches.widget());
        content.append(footer.widget());
        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vexpand(true)
            .child(&content)
            .build();

        let progress_label = gtk4::Label::builder().wrap(true).build();
        let progress = gtk4::ProgressBar::new();
        let progress_box = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        progress_box.set_valign(gtk4::Align::Center);
        progress_box.set_margin_start(18);
        progress_box.set_margin_end(18);
        progress_box.append(&progress_label);
        progress_box.append(&progress);
        let error = gtk4::Label::new(Some(&crate::ui::strings::text(
            crate::ui::strings::SOUND_ANALYSIS_FAILED,
        )));
        error.set_wrap(true);
        error.set_valign(gtk4::Align::Center);

        let root = gtk4::Stack::new();
        root.add_named(&progress_box, Some("progress"));
        root.add_named(&scrolled, Some("ready"));
        root.add_named(&error, Some("error"));
        root.set_visible_child_name("progress");
        let (request, response) = conn.path().map_or((None, None), |path| {
            let (request, response) = spawn_worker(path);
            (Some(request), Some(response))
        });
        let panel = Rc::new(Self {
            root,
            progress_label,
            progress,
            error,
            profile,
            file_info,
            matches_heading,
            matches,
            footer,
            request,
            generation: Cell::new(0),
            track_id: Cell::new(None),
            options: Cell::new(SoundPanelOptions::default()),
            snapshot: RefCell::new(None),
        });
        if let Some(response) = response {
            panel.drain(response);
        }
        panel
    }

    pub(super) fn widget(&self) -> &gtk4::Stack {
        &self.root
    }

    pub(super) fn set_track(&self, track_id: Option<i64>) {
        self.track_id.set(track_id);
        let Some(track_id) = track_id else {
            self.render(Snapshot::Progress { ready: 0, total: 0 });
            return;
        };
        self.request(track_id);
    }

    #[allow(dead_code)] // wired by the module preferences in package P6
    pub(super) fn set_options(&self, options: SoundPanelOptions) {
        self.options.set(options);
        if let Some(track_id) = self.track_id.get() {
            self.request(track_id);
        }
    }

    pub(super) fn set_on_play(&self, callback: impl Fn(i64) + 'static) {
        self.matches.callbacks().set_play(callback);
    }

    pub(super) fn set_on_play_next(&self, callback: impl Fn(i64) + 'static) {
        self.matches.callbacks().set_play_next(callback);
    }

    pub(super) fn set_on_add_to_queue(&self, callback: impl Fn(&[i64]) + 'static) {
        let callback = Rc::new(callback);
        self.footer.set_on_add({
            let callback = callback.clone();
            move |ids| callback(ids)
        });
        self.matches
            .callbacks()
            .set_add_to_queue(move |id| callback(&[id]));
    }

    fn request(&self, track_id: i64) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let Some(sender) = &self.request else {
            return;
        };
        if let Err(error) = sender.try_send(Request {
            generation,
            track_id,
            options: self.options.get(),
        }) {
            tracing::warn!(%error, "sound-panel request dropped");
        }
    }

    fn drain(self: &Rc<Self>, receiver: async_channel::Receiver<Response>) {
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            while let Ok(response) = receiver.recv().await {
                let Some(panel) = weak.upgrade() else { break };
                if panel.generation.get() == response.generation {
                    panel.render(response.snapshot);
                }
            }
        });
    }

    fn render(&self, snapshot: Snapshot) {
        match &snapshot {
            Snapshot::Progress { ready, total } => {
                self.progress_label
                    .set_label(&crate::ui::strings::sound_analysing(*ready, *total));
                let fraction = if *total == 0 {
                    0.0
                } else {
                    *ready as f64 / *total as f64
                };
                self.progress.set_fraction(fraction.clamp(0.0, 1.0));
                self.root.set_visible_child_name("progress");
            }
            Snapshot::Ready {
                profile,
                file_info,
                neighbours,
            } => {
                self.profile.render(*profile);
                self.file_info.set_label(
                    &file_info
                        .as_ref()
                        .map_or_else(String::new, format_file_info),
                );
                self.matches_heading
                    .set_label(&crate::ui::strings::sound_sounds_like(
                        neighbours.library_count,
                    ));
                self.matches.render(&neighbours.matches);
                self.footer.set_ids(shown_track_ids(neighbours));
                self.root.set_visible_child_name("ready");
            }
            Snapshot::Error(message) => {
                tracing::warn!(%message, "sound-panel calculation failed");
                self.error.set_label(&crate::ui::strings::text(
                    crate::ui::strings::SOUND_ANALYSIS_FAILED,
                ));
                self.root.set_visible_child_name("error");
            }
        }
        *self.snapshot.borrow_mut() = Some(snapshot);
    }
}

pub(super) fn ready_for_matches(feature_count: usize, current_present: bool) -> bool {
    feature_count >= MIN_READY_FEATURES && current_present
}

fn profile_positions(
    features: &SoundFeatures,
    stats: &SoundStats,
    include_tempo: bool,
) -> profile::ProfilePositions {
    profile::positions(features, stats, include_tempo)
}

pub(super) fn shown_track_ids(result: &SoundNeighbourResult) -> Vec<i64> {
    result.matches.iter().map(|row| row.track_id).collect()
}

fn spawn_worker(
    path: PathBuf,
) -> (
    async_channel::Sender<Request>,
    async_channel::Receiver<Response>,
) {
    let (requests, request_receiver) = async_channel::unbounded::<Request>();
    let (responses, response_receiver) = async_channel::unbounded();
    std::thread::Builder::new()
        .name("reprise-sound-panel".into())
        .spawn(move || worker_loop(&path, &request_receiver, &responses))
        .expect("sound-panel worker thread should start");
    (requests, response_receiver)
}

fn worker_loop(
    path: &std::path::Path,
    requests: &async_channel::Receiver<Request>,
    responses: &async_channel::Sender<Response>,
) {
    let db = match Db::open_ready(path) {
        Ok(db) => db,
        Err(error) => {
            tracing::warn!(%error, "sound-panel worker could not open library");
            return;
        }
    };
    let mut stats_cache = SoundStatsCache::default();
    while let Ok(mut request) = requests.recv_blocking() {
        loop {
            while let Ok(newer) = requests.try_recv() {
                request = newer;
            }
            let snapshot = calculate(&db, &mut stats_cache, request);
            let pending = matches!(snapshot, Snapshot::Progress { .. });
            if responses
                .send_blocking(Response {
                    generation: request.generation,
                    snapshot,
                })
                .is_err()
            {
                return;
            }
            if !pending {
                break;
            }
            std::thread::sleep(PROGRESS_RECHECK);
        }
    }
}

fn calculate(db: &Db, stats_cache: &mut SoundStatsCache, request: Request) -> Snapshot {
    let mut calculation = || -> Result<Snapshot, reprise_core::db::DbError> {
        let (ready, total) = reprise_core::db::sound_feature_inventory(db)?;
        let candidates = reprise_core::sound_neighbours::load_sound_candidates(db)?;
        let current = candidates
            .iter()
            .find(|candidate| candidate.track_id == request.track_id);
        if !ready_for_matches(ready, current.is_some()) {
            return Ok(Snapshot::Progress { ready, total });
        }
        stats_cache.refresh(db)?;
        let stats = stats_cache
            .stats()
            .expect("refresh installs sound statistics");
        let current = current.expect("readiness requires current features");
        let profile = profile_positions(&current.features, stats, request.options.include_tempo);
        let weights = if request.options.include_tempo {
            request.options.weights.with_tempo(true)
        } else {
            request.options.weights
        };
        let neighbours = rank_sound_neighbours(
            current,
            &candidates,
            stats,
            weights,
            SoundNeighbourOptions {
                exclude_same_album: request.options.exclude_same_album,
                exclude_same_artist: request.options.exclude_same_artist,
                limit: request.options.limit,
            },
        );
        let file_info = reprise_core::sound_file_info::load_sound_file_info(db, request.track_id)?;
        Ok(Snapshot::Ready {
            profile,
            file_info,
            neighbours,
        })
    };
    calculation().unwrap_or_else(|error| Snapshot::Error(error.to_string()))
}

fn format_file_info(info: &SoundFileInfo) -> String {
    let mut parts = Vec::new();
    if !info.format.is_empty() {
        parts.push(info.format.clone());
    }
    match (info.bit_depth, info.sample_rate_hz) {
        (Some(bits), Some(rate)) => {
            parts.push(format!("{bits}-bit / {:.1} kHz", rate as f32 / 1000.0));
        }
        (Some(bits), None) => parts.push(format!("{bits}-bit")),
        (None, Some(rate)) => parts.push(format!("{:.1} kHz", rate as f32 / 1000.0)),
        (None, None) => {}
    }
    if let Some(bitrate) = info.bitrate_kbps {
        parts.push(format!("{bitrate} kbit/s"));
    }
    parts.push(crate::ui::strings::compact_file_size(info.file_size));
    if let Some(frequency) = info.occupied_upper_hz {
        let frequency = format!("{:.1} kHz", frequency as f32 / 1000.0);
        parts.push(crate::ui::strings::formatted(
            crate::ui::strings::SOUND_FILE_UP_TO,
            &[("frequency", &frequency)],
        ));
    }
    parts.join(" · ")
}

pub(super) fn css() -> String {
    r#"
.reprise-sound-profile progressbar trough,
.reprise-sound-match progressbar trough { min-height: 4px; }
.reprise-sound-match { padding: 4px 0; }
.reprise-sound-match image { border-radius: 5px; }
.reprise-sound-match .numeric { font-size: 11px; opacity: 0.72; }
.reprise-sound-file-info { font-size: 11px; opacity: 0.62; }
.reprise-sound-footer { margin-top: 4px; }
"#
    .to_owned()
}

impl super::surface::NowPlayingPanel {
    pub(in crate::ui) fn show_sound(&self) {
        if !self.widgets.sound_page.is_visible() {
            return;
        }
        self.widgets.tab_stack.set_visible_child_name(SOUND_PAGE);
        self.widgets.column.set_visible(true);
    }

    pub(in crate::ui) fn set_sound_similarity_enabled(&self, enabled: bool) {
        self.sound_similarity_enabled.set(enabled);
        let external_active = self.external_snapshot.borrow().is_some();
        self.widgets
            .sound_page
            .set_visible(enabled && !external_active);
        if !enabled && self.widgets.session.selected.get() == PanelTab::Sound {
            self.widgets.tab_stack.set_visible_child_name(UP_NEXT_PAGE);
        }
    }

    pub(in crate::ui) fn set_on_sound_play(&self, callback: impl Fn(i64) + 'static) {
        self.widgets.sound.set_on_play(callback);
    }

    pub(in crate::ui) fn set_on_sound_play_next(&self, callback: impl Fn(i64) + 'static) {
        self.widgets.sound.set_on_play_next(callback);
    }

    pub(in crate::ui) fn set_on_sound_add_to_queue(&self, callback: impl Fn(&[i64]) + 'static) {
        self.widgets.sound.set_on_add_to_queue(callback);
    }
}
