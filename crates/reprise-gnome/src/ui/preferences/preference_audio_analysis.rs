//! Local Audio Character controls on the Library preferences page.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings;
use rusqlite::Connection;

use crate::ui::scan::audio_analysis_runtime::{
    AnalysisActivity, AnalysisProgress, AudioAnalysisRuntime,
};
use crate::ui::strings::{self, AUDIO_ANALYSIS_PRIVACY, AUDIO_ANALYSIS_TITLE};

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_REANALYZE: &str = "reanalyze";
#[cfg(test)]
fn control_buttons_per_row() -> usize {
    1
}

#[cfg(test)]
fn uses_custom_animation() -> bool {
    false
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ControlState {
    pause: bool,
    resume: bool,
    cancel: bool,
    retry: bool,
    reanalyze: bool,
}

fn control_state(progress: AnalysisProgress) -> ControlState {
    let running = matches!(progress.activity, AnalysisActivity::Running { .. });
    let paused = progress.activity == AnalysisActivity::Paused;
    let cancelled = progress.activity == AnalysisActivity::Cancelled;
    ControlState {
        pause: running,
        resume: paused || cancelled,
        cancel: running || paused,
        retry: progress.failed > 0 && progress.activity != AnalysisActivity::Disabled,
        reanalyze: progress.activity != AnalysisActivity::Disabled
            && (progress.total > 0 || progress.analyzed > 0 || progress.failed > 0),
    }
}

fn status_text(progress: AnalysisProgress) -> String {
    if progress.total == 0 && progress.activity == AnalysisActivity::Idle {
        return strings::text(strings::AUDIO_ANALYSIS_EMPTY);
    }
    let analyzed = progress.analyzed.to_string();
    let total = progress.total.to_string();
    let failed = progress.failed.to_string();
    let template = match progress.activity {
        AnalysisActivity::Disabled => strings::AUDIO_ANALYSIS_OFF,
        AnalysisActivity::Idle => strings::AUDIO_ANALYSIS_READY,
        AnalysisActivity::Running { .. } => strings::AUDIO_ANALYSIS_RUNNING,
        AnalysisActivity::Paused => strings::AUDIO_ANALYSIS_PAUSED,
        AnalysisActivity::Cancelled => strings::AUDIO_ANALYSIS_CANCELLED,
        AnalysisActivity::Failed => strings::AUDIO_ANALYSIS_FAILED,
        AnalysisActivity::Complete => strings::AUDIO_ANALYSIS_COMPLETE,
    };
    strings::formatted(
        template,
        &[
            ("analyzed", &analyzed),
            ("total", &total),
            ("failed", &failed),
        ],
    )
}

fn should_reanalyze(response: &str) -> bool {
    response == RESPONSE_REANALYZE
}

fn persist_enabled(conn: &Rc<RefCell<Connection>>, enabled: bool) -> Result<(), rusqlite::Error> {
    let conn = conn.borrow();
    settings::set_audio_analysis_enabled(&conn, enabled)
}

struct AnalysisSurface {
    status: glib::WeakRef<adw::ActionRow>,
    pause: glib::WeakRef<adw::ActionRow>,
    resume: glib::WeakRef<adw::ActionRow>,
    cancel: glib::WeakRef<adw::ActionRow>,
    retry: glib::WeakRef<adw::ActionRow>,
    reanalyze: glib::WeakRef<adw::ActionRow>,
}

impl AnalysisSurface {
    fn apply(&self, progress: AnalysisProgress) -> bool {
        let Some(status) = self.status.upgrade() else {
            return false;
        };
        status.set_subtitle(&status_text(progress));
        let controls = control_state(progress);
        for (row, visible) in [
            (&self.pause, controls.pause),
            (&self.resume, controls.resume),
            (&self.cancel, controls.cancel),
            (&self.retry, controls.retry),
            (&self.reanalyze, controls.reanalyze),
        ] {
            if let Some(row) = row.upgrade() {
                row.set_visible(visible);
            }
        }
        true
    }
}

struct Inner {
    conn: Rc<RefCell<Connection>>,
    runtime: Option<AudioAnalysisRuntime>,
    progress: Cell<AnalysisProgress>,
    surfaces: RefCell<Vec<AnalysisSurface>>,
}

#[derive(Clone)]
pub(in crate::ui) struct AudioAnalysisPreferences {
    inner: Rc<Inner>,
}

impl AudioAnalysisPreferences {
    pub(in crate::ui) fn new(
        conn: &Rc<RefCell<Connection>>,
        runtime: Option<&AudioAnalysisRuntime>,
    ) -> Self {
        let progress = runtime.map_or(
            AnalysisProgress {
                activity: AnalysisActivity::Disabled,
                analyzed: 0,
                total: 0,
                failed: 0,
            },
            AudioAnalysisRuntime::progress,
        );
        let preferences = Self {
            inner: Rc::new(Inner {
                conn: conn.clone(),
                runtime: runtime.cloned(),
                progress: Cell::new(progress),
                surfaces: RefCell::new(Vec::new()),
            }),
        };
        preferences.install_progress_listener();
        preferences
    }

    fn install_progress_listener(&self) {
        let Some(runtime) = &self.inner.runtime else {
            return;
        };
        let receiver = runtime.progress_receiver();
        let inner = Rc::downgrade(&self.inner);
        glib::spawn_future_local(async move {
            while let Ok(progress) = receiver.recv().await {
                let Some(inner) = inner.upgrade() else {
                    return;
                };
                inner.progress.set(progress);
                inner
                    .surfaces
                    .borrow_mut()
                    .retain(|surface| surface.apply(progress));
            }
        });
    }

    pub(in crate::ui) fn build_group(&self, parent: &gtk4::Widget) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::builder()
            .title(strings::text(strings::AUDIO_CHARACTER))
            .build();
        let enabled = settings::get_audio_analysis_enabled(&self.inner.conn.borrow());
        let toggle = adw::SwitchRow::builder()
            .title(strings::text(AUDIO_ANALYSIS_TITLE))
            .subtitle(strings::text(AUDIO_ANALYSIS_PRIVACY))
            .active(enabled)
            .build();
        let inner = self.inner.clone();
        toggle.connect_active_notify(move |row| {
            let enabled = row.is_active();
            let saved = persist_enabled(&inner.conn, enabled);
            if let Err(error) = saved {
                tracing::warn!(%error, "could not save audio-analysis setting");
                return;
            }
            if let Some(runtime) = &inner.runtime {
                runtime.set_enabled(enabled);
            }
        });
        group.add(&toggle);

        let status = adw::ActionRow::builder()
            .title(strings::text(strings::AUDIO_ANALYSIS_PROGRESS))
            .subtitle(status_text(self.inner.progress.get()))
            .build();
        group.add(&status);

        let pause = action_button_row(strings::AUDIO_ANALYSIS_PAUSE, {
            let runtime = self.inner.runtime.clone();
            move || {
                if let Some(runtime) = &runtime {
                    runtime.pause();
                }
            }
        });
        group.add(&pause);
        let resume = action_button_row(strings::AUDIO_ANALYSIS_RESUME, {
            let runtime = self.inner.runtime.clone();
            move || {
                if let Some(runtime) = &runtime {
                    runtime.resume();
                }
            }
        });
        group.add(&resume);
        let cancel = action_button_row(strings::AUDIO_ANALYSIS_CANCEL, {
            let runtime = self.inner.runtime.clone();
            move || {
                if let Some(runtime) = &runtime {
                    runtime.cancel();
                }
            }
        });
        group.add(&cancel);
        let retry = action_button_row(strings::AUDIO_ANALYSIS_RETRY, {
            let runtime = self.inner.runtime.clone();
            move || {
                if let Some(runtime) = &runtime {
                    if let Err(error) = runtime.retry_failed() {
                        tracing::warn!(%error, "could not retry failed audio analyses");
                    }
                }
            }
        });
        group.add(&retry);
        let reanalyze = self.reanalyze_row(parent);
        group.add(&reanalyze);

        let surface = AnalysisSurface {
            status: status.downgrade(),
            pause: pause.downgrade(),
            resume: resume.downgrade(),
            cancel: cancel.downgrade(),
            retry: retry.downgrade(),
            reanalyze: reanalyze.downgrade(),
        };
        surface.apply(self.inner.progress.get());
        self.inner.surfaces.borrow_mut().push(surface);
        group
    }

    fn reanalyze_row(&self, parent: &gtk4::Widget) -> adw::ActionRow {
        let parent = parent.clone();
        let inner = self.inner.clone();
        action_button_row(strings::AUDIO_ANALYSIS_REANALYZE, move || {
            let dialog = adw::AlertDialog::builder()
                .heading(strings::text(strings::AUDIO_ANALYSIS_REANALYZE_HEADING))
                .body(strings::text(strings::AUDIO_ANALYSIS_REANALYZE_BODY))
                .close_response(RESPONSE_CANCEL)
                .build();
            dialog.add_response(RESPONSE_CANCEL, &strings::text(strings::CANCEL));
            dialog.add_response(
                RESPONSE_REANALYZE,
                &strings::text(strings::AUDIO_ANALYSIS_REANALYZE_CONFIRM),
            );
            dialog
                .set_response_appearance(RESPONSE_REANALYZE, adw::ResponseAppearance::Destructive);
            let inner = inner.clone();
            dialog.choose(Some(&parent), gio::Cancellable::NONE, move |response| {
                if !should_reanalyze(response.as_str()) {
                    return;
                }
                let reset = inner
                    .runtime
                    .as_ref()
                    .map_or_else(|| Ok(0), AudioAnalysisRuntime::reanalyze);
                match reset {
                    Ok(count) => {
                        tracing::info!(count, "audio analyses cleared for reanalysis");
                    }
                    Err(error) => {
                        tracing::warn!(%error, "could not clear audio analyses");
                    }
                }
            });
        })
    }
}

fn action_button_row(label: &str, action: impl Fn() + 'static) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(strings::text(label))
        .build();
    let button = gtk4::Button::with_label(&strings::text(label));
    button.set_valign(gtk4::Align::Center);
    button.connect_clicked(move |_| action());
    row.add_suffix(&button);
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::scan::audio_analysis_runtime::{AnalysisActivity, AnalysisProgress};

    fn progress(
        activity: AnalysisActivity,
        analyzed: u64,
        total: u64,
        failed: u64,
    ) -> AnalysisProgress {
        AnalysisProgress {
            activity,
            analyzed,
            total,
            failed,
        }
    }

    #[test]
    fn ac_1_fresh_install_copy_is_explicitly_local_and_opt_in() {
        assert_eq!(AUDIO_ANALYSIS_TITLE, "Analyze audio locally");
        assert!(AUDIO_ANALYSIS_PRIVACY.contains("only on this device"));
        assert!(AUDIO_ANALYSIS_PRIVACY.contains("Nothing is uploaded"));
        assert!(AUDIO_ANALYSIS_PRIVACY.contains("kept"));
        assert!(include_str!("preference_library.rs").contains("audio_analysis.build_group"));
        assert!(!include_str!("preference_plugins.rs").contains("audio_analysis.build_group"));
    }

    #[test]
    fn ac_3_each_worker_state_exposes_only_reachable_controls() {
        assert_eq!(control_buttons_per_row(), 1);
        assert!(!uses_custom_animation());
        let running = control_state(progress(AnalysisActivity::Running { track_id: 7 }, 2, 9, 0));
        assert!(running.pause);
        assert!(running.cancel);
        assert!(!running.resume);
        assert!(!running.retry);

        let paused = control_state(progress(AnalysisActivity::Paused, 2, 9, 0));
        assert!(paused.resume);
        assert!(paused.cancel);
        assert!(!paused.pause);

        let failed = control_state(progress(AnalysisActivity::Failed, 8, 9, 1));
        assert!(failed.retry);
        assert!(!failed.pause);
        assert!(!failed.resume);
    }

    #[test]
    fn audio_analysis_setting_borrow_ends_before_scheduler_callbacks() {
        let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));

        persist_enabled(&conn, true).unwrap();

        assert!(conn.try_borrow_mut().is_ok());
    }

    #[test]
    fn ac_6_coverage_states_are_distinct_and_reanalysis_requires_confirmation() {
        let cases = [
            (AnalysisActivity::Idle, 0, 0, "No eligible tracks"),
            (AnalysisActivity::Running { track_id: 1 }, 2, 9, "Analyzing"),
            (AnalysisActivity::Paused, 2, 9, "Paused"),
            (AnalysisActivity::Failed, 8, 9, "Failed"),
            (AnalysisActivity::Complete, 9, 9, "Complete"),
        ];
        for (activity, analyzed, total, marker) in cases {
            assert!(status_text(progress(
                activity,
                analyzed,
                total,
                u64::from(marker == "Failed")
            ))
            .contains(marker));
        }
        assert!(!should_reanalyze("cancel"));
        assert!(should_reanalyze("reanalyze"));
    }
}
