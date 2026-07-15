use std::cell::Cell;
use std::path::Path;
use std::rc::{Rc, Weak};
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::scanner::ScanProgress;

use super::strings;

const PULSE_INTERVAL: Duration = Duration::from_millis(100);
const PULSE_STEP: f64 = 0.08;

#[derive(Clone, Copy, Debug, PartialEq)]
enum ProgressMode {
    Indeterminate,
    Determinate(f64),
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum DisplayPhase {
    #[default]
    Hidden,
    Discovering,
    Scanning,
    Fetching,
}

#[derive(Debug, PartialEq)]
struct ScanProgressState {
    title: String,
    detail: Option<String>,
    mode: ProgressMode,
}

fn display_name(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
}

fn view_state(progress: &ScanProgress) -> ScanProgressState {
    match progress {
        ScanProgress::Discovering => ScanProgressState {
            title: strings::text(strings::SCAN_DISCOVERING),
            detail: None,
            mode: ProgressMode::Indeterminate,
        },
        ScanProgress::Scanning {
            processed,
            total,
            current_path,
        } => {
            let fraction = if *total == 0 {
                0.0
            } else {
                (*processed as f64 / *total as f64).clamp(0.0, 1.0)
            };
            ScanProgressState {
                title: strings::scan_progress(*processed, *total),
                detail: display_name(current_path),
                mode: ProgressMode::Determinate(fraction),
            }
        }
        ScanProgress::Fetching { done, total } => {
            let fraction = if *total == 0 {
                0.0
            } else {
                (*done as f64 / *total as f64).clamp(0.0, 1.0)
            };
            ScanProgressState {
                title: strings::fetch_progress(*done, *total),
                detail: Some(strings::text(strings::FETCH_DETAIL)),
                mode: ProgressMode::Determinate(fraction),
            }
        }
    }
}

/// Sidebar card widget showing scan progress with a spinner, percent label,
/// progress bar, and detail label. Replaces the old headerbar banner.
/// A generation token stops an old pulse timeout whenever the phase changes
/// or a scan finishes, so repeated scans never retain stale GTK callbacks.
#[derive(Clone)]
pub(super) struct ScanProgressView {
    inner: Rc<ScanProgressWidgets>,
}

struct ScanProgressWidgets {
    revealer: gtk4::Revealer,
    container: gtk4::Box,
    spinner: gtk4::Spinner,
    title: gtk4::Label,
    percent: gtk4::Label,
    progress: gtk4::ProgressBar,
    detail: gtk4::Label,
    pulse_generation: Rc<Cell<u64>>,
    phase: Rc<Cell<DisplayPhase>>,
}

#[derive(Clone)]
pub(super) struct WeakScanProgressView(Weak<ScanProgressWidgets>);

impl WeakScanProgressView {
    pub(super) fn upgrade(&self) -> Option<ScanProgressView> {
        self.0.upgrade().map(|inner| ScanProgressView { inner })
    }
}

impl ScanProgressView {
    pub(super) fn new() -> Self {
        let spinner = gtk4::Spinner::builder()
            .spinning(false)
            .build();
        spinner.add_css_class("scan-card-spinner");

        let title = gtk4::Label::builder()
            .label("")
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        title.add_css_class("scan-card-title");

        let percent = gtk4::Label::builder()
            .label("")
            .halign(gtk4::Align::End)
            .build();
        percent.add_css_class("scan-card-percent");

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        header.append(&spinner);
        header.append(&title);
        header.append(&percent);

        let progress = gtk4::ProgressBar::builder()
            .hexpand(true)
            .build();
        progress.set_pulse_step(PULSE_STEP);

        let detail = gtk4::Label::builder()
            .label("")
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        detail.add_css_class("scan-card-detail");

        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        container.add_css_class("scan-card");
        container.append(&header);
        container.append(&progress);
        container.append(&detail);

        let revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::Crossfade)
            .transition_duration(150)
            .child(&container)
            .reveal_child(false)
            .build();

        Self {
            inner: Rc::new(ScanProgressWidgets {
                revealer,
                container,
                spinner,
                title,
                percent,
                progress,
                detail,
                pulse_generation: Rc::new(Cell::new(0)),
                phase: Rc::new(Cell::new(DisplayPhase::Hidden)),
            }),
        }
    }

    pub(super) fn downgrade(&self) -> WeakScanProgressView {
        WeakScanProgressView(Rc::downgrade(&self.inner))
    }

    pub(super) fn widget(&self) -> &gtk4::Revealer {
        &self.inner.revealer
    }

    pub(super) fn show(&self, progress: &ScanProgress) {
        let state = view_state(progress);
        self.inner.title.set_label(&strings::text(strings::SCAN_CARD_TITLE));
        self.inner.spinner.set_spinning(true);
        self.inner.revealer.set_reveal_child(true);

        match state.mode {
            ProgressMode::Indeterminate => {
                self.inner.percent.set_label("");
                self.inner.detail.set_label("");
                self.inner.detail.set_visible(false);
                if self.inner.phase.replace(DisplayPhase::Discovering) != DisplayPhase::Discovering
                {
                    self.start_pulsing();
                }
            }
            ProgressMode::Determinate(fraction) => {
                self.cancel_pulsing();
                self.inner.progress.set_fraction(fraction);
                let pct = format!("{}%", (fraction * 100.0).round() as u32);
                self.inner.percent.set_label(&pct);
                if let Some(detail) = &state.detail {
                    self.inner.detail.set_label(detail);
                    self.inner.detail.set_visible(true);
                }
                let new_phase = match progress {
                    ScanProgress::Fetching { .. } => DisplayPhase::Fetching,
                    _ => DisplayPhase::Scanning,
                };
                self.inner.phase.set(new_phase);
            }
        }

        // Update tooltip with queue info for the Fetching phase
        match progress {
            ScanProgress::Fetching { done, total } => {
                let remaining = total.saturating_sub(*done);
                self.inner.container.set_tooltip_text(Some(
                    &strings::scan_card_tooltip(remaining),
                ));
            }
            _ => {
                self.inner.container.set_tooltip_text(None);
            }
        }
    }

    pub(super) fn finish(&self) {
        self.cancel_pulsing();
        self.inner.phase.set(DisplayPhase::Hidden);
        self.inner.spinner.set_spinning(false);
        self.inner.revealer.set_reveal_child(false);
        self.inner.progress.set_fraction(0.0);
    }

    fn start_pulsing(&self) {
        let generation = self.inner.pulse_generation.get().wrapping_add(1);
        self.inner.pulse_generation.set(generation);
        self.inner.progress.set_fraction(0.0);
        self.inner.progress.pulse();

        let progress = self.inner.progress.downgrade();
        let pulse_generation = self.inner.pulse_generation.clone();
        glib::timeout_add_local(PULSE_INTERVAL, move || {
            if pulse_generation.get() != generation {
                return glib::ControlFlow::Break;
            }
            let Some(progress) = progress.upgrade() else {
                return glib::ControlFlow::Break;
            };
            progress.pulse();
            glib::ControlFlow::Continue
        });
    }

    fn cancel_pulsing(&self) {
        self.inner
            .pulse_generation
            .set(self.inner.pulse_generation.get().wrapping_add(1));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use reprise_core::library::scanner::ScanProgress;

    use super::{view_state, ProgressMode, ScanProgressView};

    #[test]
    fn discovery_uses_an_indeterminate_progress_state() {
        let state = view_state(&ScanProgress::Discovering);

        assert_eq!(state.title, "Finding music files…");
        assert_eq!(state.detail, None);
        assert_eq!(state.mode, ProgressMode::Indeterminate);
    }

    #[test]
    fn scanning_shows_counts_filename_and_clamped_fraction() {
        let state = view_state(&ScanProgress::Scanning {
            processed: 7,
            total: 4,
            current_path: PathBuf::from("/music/Album/a very long song.flac"),
        });

        assert_eq!(state.title, "7 of 4 files scanned");
        assert_eq!(state.detail.as_deref(), Some("a very long song.flac"));
        assert_eq!(state.mode, ProgressMode::Determinate(1.0));
    }

    #[test]
    fn empty_library_has_a_finite_zero_fraction() {
        let state = view_state(&ScanProgress::Scanning {
            processed: 0,
            total: 0,
            current_path: PathBuf::new(),
        });

        assert_eq!(state.title, "0 of 0 files scanned");
        assert_eq!(state.detail, None);
        assert_eq!(state.mode, ProgressMode::Determinate(0.0));
    }

    #[test]
    fn fetching_shows_counts_and_detail() {
        let state = view_state(&ScanProgress::Fetching {
            done: 12,
            total: 48,
        });

        assert_eq!(state.title, "12 of 48");
        assert!(state.detail.as_deref().unwrap().contains("covers"));
        assert_eq!(state.mode, ProgressMode::Determinate(0.25));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn widgets_reveal_progress_and_hide_after_finish() {
        if gtk4::init().is_err() {
            return;
        }
        let view = ScanProgressView::new();
        view.show(&ScanProgress::Scanning {
            processed: 2,
            total: 4,
            current_path: PathBuf::from("/music/song.flac"),
        });

        assert!(view.inner.revealer.reveals_child());
        assert!(view.inner.spinner.is_spinning());
        assert_eq!(view.inner.percent.label(), "50%");
        assert_eq!(view.inner.progress.fraction(), 0.5);

        view.finish();
        assert!(!view.inner.revealer.reveals_child());
        assert!(!view.inner.spinner.is_spinning());
    }
}
