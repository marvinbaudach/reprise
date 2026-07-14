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
    }
}

/// Compact progress row displayed directly below its host window's header bar.
/// A generation token stops an old pulse timeout whenever the phase changes
/// or a scan finishes, so repeated scans never retain stale GTK callbacks.
#[derive(Clone)]
pub(super) struct ScanProgressView {
    inner: Rc<ScanProgressWidgets>,
}

struct ScanProgressWidgets {
    revealer: gtk4::Revealer,
    title: gtk4::Label,
    detail: gtk4::Label,
    progress: gtk4::ProgressBar,
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
        let title = gtk4::Label::builder()
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .xalign(0.0)
            .build();
        let detail = gtk4::Label::builder()
            .halign(gtk4::Align::End)
            .hexpand(true)
            .xalign(1.0)
            .build();
        detail.add_css_class("dim-label");
        detail.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);

        let labels = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        labels.append(&title);
        labels.append(&detail);

        let progress = gtk4::ProgressBar::builder().hexpand(true).build();
        progress.set_pulse_step(PULSE_STEP);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(6);
        content.set_margin_bottom(6);
        content.append(&labels);
        content.append(&progress);

        let revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .child(&content)
            .build();

        Self {
            inner: Rc::new(ScanProgressWidgets {
                revealer,
                title,
                detail,
                progress,
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
        self.inner.title.set_label(&state.title);
        self.inner
            .detail
            .set_label(state.detail.as_deref().unwrap_or(""));
        self.inner.detail.set_tooltip_text(state.detail.as_deref());
        self.inner.revealer.set_reveal_child(true);

        match state.mode {
            ProgressMode::Indeterminate => {
                if self.inner.phase.replace(DisplayPhase::Discovering) != DisplayPhase::Discovering
                {
                    self.start_pulsing();
                }
            }
            ProgressMode::Determinate(fraction) => {
                self.cancel_pulsing();
                self.inner.progress.set_fraction(fraction);
                self.inner.phase.set(DisplayPhase::Scanning);
            }
        }
    }

    pub(super) fn finish(&self) {
        self.cancel_pulsing();
        self.inner.phase.set(DisplayPhase::Hidden);
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
        assert_eq!(view.inner.title.label(), "2 of 4 files scanned");
        assert_eq!(view.inner.detail.label(), "song.flac");
        assert_eq!(view.inner.progress.fraction(), 0.5);

        view.finish();
        assert!(!view.inner.revealer.reveals_child());
    }
}
