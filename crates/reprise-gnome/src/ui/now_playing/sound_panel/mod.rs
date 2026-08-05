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
use super::panel_state::{tab_after_sound_visibility_change, SOUND_PAGE};

mod footer;
mod list;
mod profile;
#[cfg(test)]
mod tests;

pub(super) const MIN_READY_FEATURES: usize = 50;
const PROGRESS_RECHECK: Duration = Duration::from_millis(500);
/// How many identical inventory readings in a row end the re-checks. The
/// backfill stores one track at a time, so a library that is still catching up
/// moves the counts well inside this budget; ten seconds of complete standstill
/// mean nothing is deriving profiles any more and re-checking cannot help.
const PROGRESS_STALL_LIMIT: usize = 20;

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

impl From<reprise_core::sound_preferences::SoundSimilarityPreferences> for SoundPanelOptions {
    fn from(preferences: reprise_core::sound_preferences::SoundSimilarityPreferences) -> Self {
        Self {
            exclude_same_album: preferences.exclude_same_album,
            exclude_same_artist: preferences.exclude_same_artist,
            include_tempo: preferences.include_tempo,
            weights: preferences.weighting.weights(),
            limit: preferences.match_count,
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
    /// The inventory stopped advancing before it could carry this track, so
    /// waiting longer changes nothing until something asks again.
    Unavailable,
    Error(String),
}

/// Watches whether the profile inventory still advances between re-checks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ProgressWatch {
    inventory: Option<(usize, usize)>,
    stalled: usize,
}

impl ProgressWatch {
    /// Folds one `(ready, total)` reading in. `None` means the counts have stood
    /// still for `PROGRESS_STALL_LIMIT` readings: the library is not catching up
    /// any more, so the panel settles instead of polling for the rest of the
    /// session. A later request starts a fresh watch.
    #[must_use]
    pub(super) fn observe(self, inventory: (usize, usize)) -> Option<Self> {
        if self.inventory != Some(inventory) {
            return Some(Self {
                inventory: Some(inventory),
                stalled: 0,
            });
        }
        (self.stalled < PROGRESS_STALL_LIMIT).then_some(Self {
            inventory: self.inventory,
            stalled: self.stalled + 1,
        })
    }
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
    path: Option<PathBuf>,
    request: RefCell<Option<async_channel::Sender<Request>>>,
    enabled: Cell<bool>,
    generation: Cell<u64>,
    track_id: Cell<Option<i64>>,
    options: Cell<SoundPanelOptions>,
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
        // Nothing is spawned here: a module the user never switched on gets no
        // worker thread and no library query. `set_enabled` starts the work.
        Rc::new(Self {
            root,
            progress_label,
            progress,
            error,
            profile,
            file_info,
            matches_heading,
            matches,
            footer,
            path: conn.path(),
            request: RefCell::new(None),
            enabled: Cell::new(false),
            generation: Cell::new(0),
            track_id: Cell::new(None),
            options: Cell::new(SoundPanelOptions::default()),
        })
    }

    pub(super) fn widget(&self) -> &gtk4::Stack {
        &self.root
    }

    /// Follows the module switch. Enabling starts the worker and picks up the
    /// track the panel was told about while it was off; disabling drops the
    /// request channel, which ends the worker thread.
    pub(super) fn set_enabled(self: &Rc<Self>, enabled: bool) {
        if self.enabled.get() == enabled {
            return;
        }
        self.enabled.set(enabled);
        if !enabled {
            self.request.borrow_mut().take();
            // A response already in flight belongs to the enabled session.
            self.generation.set(self.generation.get().wrapping_add(1));
            self.render(Snapshot::Progress { ready: 0, total: 0 });
            return;
        }
        self.start_worker();
        if let Some(track_id) = self.track_id.get() {
            self.request(track_id);
        }
    }

    pub(super) fn set_track(&self, track_id: Option<i64>) {
        self.track_id.set(track_id);
        if !self.work_allowed() {
            return;
        }
        let Some(track_id) = track_id else {
            self.render(Snapshot::Progress { ready: 0, total: 0 });
            return;
        };
        self.request(track_id);
    }

    pub(super) fn set_options(&self, options: SoundPanelOptions) {
        self.options.set(options);
        if !self.work_allowed() {
            return;
        }
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

    pub(super) fn set_on_open_album(&self, callback: impl Fn(i64, &str, &str) + 'static) {
        self.matches.callbacks().set_open_album(callback);
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

    fn work_allowed(&self) -> bool {
        sound_work_allowed(self.enabled.get(), self.path.is_some())
    }

    fn start_worker(self: &Rc<Self>) {
        let running = self.request.borrow().is_some();
        if running || !self.work_allowed() {
            return;
        }
        let Some(path) = self.path.clone() else {
            return;
        };
        let Some((sender, responses)) = spawn_worker(path) else {
            return;
        };
        *self.request.borrow_mut() = Some(sender);
        self.drain(responses);
    }

    fn request(&self, track_id: i64) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let sender = self.request.borrow().clone();
        let Some(sender) = sender else {
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
        match snapshot {
            Snapshot::Progress { ready, total } => {
                self.progress_label
                    .set_label(&crate::ui::strings::sound_analysing(ready, total));
                let fraction = if total == 0 {
                    0.0
                } else {
                    ready as f64 / total as f64
                };
                self.progress.set_fraction(fraction.clamp(0.0, 1.0));
                self.root.set_visible_child_name("progress");
            }
            Snapshot::Ready {
                profile,
                file_info,
                neighbours,
            } => {
                self.profile.render(profile);
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
                self.footer.set_ids(shown_track_ids(&neighbours));
                self.root.set_visible_child_name("ready");
            }
            Snapshot::Unavailable => self.render_unavailable(),
            Snapshot::Error(message) => {
                tracing::warn!(%message, "sound-panel calculation failed");
                self.render_unavailable();
            }
        }
    }

    fn render_unavailable(&self) {
        self.error.set_label(&crate::ui::strings::text(
            crate::ui::strings::SOUND_ANALYSIS_FAILED,
        ));
        self.root.set_visible_child_name("error");
    }
}

/// A disabled module does no sound work at all: no worker thread, no library
/// query, no ranking. The panel still remembers the track it was told about, so
/// switching the module on picks it up without waiting for the next track.
pub(super) fn sound_work_allowed(enabled: bool, has_database: bool) -> bool {
    enabled && has_database
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

/// Starts the panel's worker thread. A thread the system refuses to start
/// degrades to the same "no sound panel" state as a library without a path —
/// this runs while the window is being built and must not take it down.
fn spawn_worker(
    path: PathBuf,
) -> Option<(
    async_channel::Sender<Request>,
    async_channel::Receiver<Response>,
)> {
    let (requests, request_receiver) = async_channel::unbounded::<Request>();
    let (responses, response_receiver) = async_channel::unbounded();
    if let Err(error) = std::thread::Builder::new()
        .name("reprise-sound-panel".into())
        .spawn(move || worker_loop(&path, &request_receiver, &responses))
    {
        tracing::warn!(%error, "could not start sound-panel worker");
        return None;
    }
    Some((requests, response_receiver))
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
            report_open_failure(&error.to_string(), requests, responses);
            return;
        }
    };
    let mut stats_cache = SoundStatsCache::default();
    while let Ok(mut request) = requests.recv_blocking() {
        let mut watch = ProgressWatch::default();
        loop {
            let mut newer = false;
            loop {
                match requests.try_recv() {
                    Ok(request_from_panel) => {
                        request = request_from_panel;
                        newer = true;
                    }
                    Err(async_channel::TryRecvError::Empty) => break,
                    // The panel dropped its request channel: the module is off.
                    Err(async_channel::TryRecvError::Closed) => return,
                }
            }
            if newer {
                watch = ProgressWatch::default();
            }
            let snapshot = calculate(&db, &mut stats_cache, request);
            let inventory = match &snapshot {
                Snapshot::Progress { ready, total } => Some((*ready, *total)),
                _ => None,
            };
            let next_watch = inventory.and_then(|inventory| watch.observe(inventory));
            let settled = inventory.is_some() && next_watch.is_none();
            if responses
                .send_blocking(Response {
                    generation: request.generation,
                    snapshot: if settled {
                        Snapshot::Unavailable
                    } else {
                        snapshot
                    },
                })
                .is_err()
            {
                return;
            }
            let Some(next_watch) = next_watch else {
                break;
            };
            watch = next_watch;
            std::thread::sleep(PROGRESS_RECHECK);
        }
    }
}

/// Answers every request with the failure the panel could not see otherwise:
/// without this the response channel just closes and the tab keeps showing an
/// empty progress bar for the whole session.
fn report_open_failure(
    message: &str,
    requests: &async_channel::Receiver<Request>,
    responses: &async_channel::Sender<Response>,
) {
    while let Ok(request) = requests.recv_blocking() {
        if responses
            .send_blocking(Response {
                generation: request.generation,
                snapshot: Snapshot::Error(message.to_owned()),
            })
            .is_err()
        {
            return;
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
        self.widgets.sound.set_enabled(enabled);
        let external_active = self.external_snapshot.borrow().is_some();
        self.widgets
            .sound_page
            .set_visible(enabled && !external_active);
        let selected = self.widgets.session.selected.get();
        let next = tab_after_sound_visibility_change(selected, enabled);
        if next != selected {
            self.widgets
                .tab_stack
                .set_visible_child_name(next.page_name());
        }
    }

    pub(in crate::ui) fn refresh_sound_options(&self) {
        match reprise_core::sound_preferences::SoundSimilarityPreferences::load(&self.conn) {
            Ok(preferences) => self.widgets.sound.set_options(preferences.into()),
            Err(error) => tracing::warn!(%error, "could not load Sound Similarity preferences"),
        }
    }

    pub(in crate::ui) fn set_on_sound_play(&self, callback: impl Fn(i64) + 'static) {
        self.widgets.sound.set_on_play(callback);
    }

    pub(in crate::ui) fn set_on_sound_play_next(&self, callback: impl Fn(i64) + 'static) {
        self.widgets.sound.set_on_play_next(callback);
    }

    pub(in crate::ui) fn set_on_sound_open_album(
        &self,
        callback: impl Fn(i64, &str, &str) + 'static,
    ) {
        self.widgets.sound.set_on_open_album(callback);
    }

    pub(in crate::ui) fn set_on_sound_add_to_queue(&self, callback: impl Fn(&[i64]) + 'static) {
        self.widgets.sound.set_on_add_to_queue(callback);
    }
}
