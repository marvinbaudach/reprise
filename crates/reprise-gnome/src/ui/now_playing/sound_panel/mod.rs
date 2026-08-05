//! The retained, worker-fed Sound tab in the Now Playing panel.
//!
//! The panel is the widget half only: it renders a snapshot, discards a late
//! answer for a track it no longer shows, and follows the module switch. The
//! snapshot itself is computed in `reprise_core::sound_snapshot`, on the thread
//! `reprise_platform_linux::sound_worker` owns.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::sound_file_info::SoundFileInfo;
use reprise_core::sound_neighbours::SoundNeighbourResult;
use reprise_core::sound_snapshot::{sound_work_allowed, SoundSnapshot, SoundSnapshotOptions};
use reprise_platform_linux::sound_worker::{SoundRequest, SoundResponse, SoundWorkerHandle};

use super::super::cover_loader::CoverLoader;
use super::panel_state::{tab_after_sound_visibility_change, SOUND_PAGE};

mod footer;
mod list;
mod profile;
#[cfg(test)]
mod tests;

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
    worker: RefCell<Option<SoundWorkerHandle>>,
    enabled: Cell<bool>,
    generation: Cell<u64>,
    track_id: Cell<Option<i64>>,
    options: Cell<SoundSnapshotOptions>,
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
            worker: RefCell::new(None),
            enabled: Cell::new(false),
            generation: Cell::new(0),
            track_id: Cell::new(None),
            options: Cell::new(SoundSnapshotOptions::default()),
        })
    }

    pub(super) fn widget(&self) -> &gtk4::Stack {
        &self.root
    }

    /// Follows the module switch. Enabling starts the worker and picks up the
    /// track the panel was told about while it was off; disabling drops the
    /// worker handle, which ends the worker thread.
    pub(super) fn set_enabled(self: &Rc<Self>, enabled: bool) {
        if self.enabled.get() == enabled {
            return;
        }
        self.enabled.set(enabled);
        if !enabled {
            self.worker.borrow_mut().take();
            // A response already in flight belongs to the enabled session.
            self.generation.set(self.generation.get().wrapping_add(1));
            self.render(SoundSnapshot::Progress { ready: 0, total: 0 });
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
            self.render(SoundSnapshot::Progress { ready: 0, total: 0 });
            return;
        };
        self.request(track_id);
    }

    pub(super) fn set_options(&self, options: SoundSnapshotOptions) {
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
        let running = self.worker.borrow().is_some();
        if running || !self.work_allowed() {
            return;
        }
        let Some(path) = self.path.clone() else {
            return;
        };
        let Some(worker) = SoundWorkerHandle::start(path) else {
            return;
        };
        let responses = worker.responses();
        *self.worker.borrow_mut() = Some(worker);
        self.drain(responses);
    }

    fn request(&self, track_id: i64) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let options = self.options.get();
        if let Some(worker) = self.worker.borrow().as_ref() {
            worker.request(SoundRequest {
                generation,
                track_id,
                options,
            });
        }
    }

    fn drain(self: &Rc<Self>, receiver: async_channel::Receiver<SoundResponse>) {
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

    fn render(&self, snapshot: SoundSnapshot) {
        match snapshot {
            SoundSnapshot::Progress { ready, total } => {
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
            SoundSnapshot::Ready {
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
            SoundSnapshot::Unavailable => self.render_unavailable(),
            SoundSnapshot::Error(message) => {
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

/// The ids the panel currently shows, in the order it shows them — what **Add
/// to queue** appends.
pub(super) fn shown_track_ids(result: &SoundNeighbourResult) -> Vec<i64> {
    result.matches.iter().map(|row| row.track_id).collect()
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
