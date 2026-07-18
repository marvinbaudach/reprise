use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::{Rc, Weak};
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::scanner::ScanProgress;

use super::strings;

const PULSE_INTERVAL: Duration = Duration::from_millis(100);
const PULSE_STEP: f64 = 0.08;

type OnCancel = Rc<dyn Fn()>;
type OnCancelSlot = Rc<RefCell<Option<OnCancel>>>;

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

#[derive(Debug, PartialEq, Eq)]
struct ScanUnavailableState {
    title: String,
    detail: String,
}

fn unavailable_state(root: &Path) -> ScanUnavailableState {
    ScanUnavailableState {
        title: strings::unavailable_title(),
        detail: strings::library_folder_not_mounted(&root.to_string_lossy()),
    }
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
pub(in crate::ui) struct ScanProgressView {
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
    on_cancel: OnCancelSlot,
}

#[derive(Clone)]
pub(in crate::ui) struct WeakScanProgressView(Weak<ScanProgressWidgets>);

impl WeakScanProgressView {
    pub(in crate::ui) fn upgrade(&self) -> Option<ScanProgressView> {
        self.0.upgrade().map(|inner| ScanProgressView { inner })
    }
}

impl ScanProgressView {
    pub(in crate::ui) fn new() -> Self {
        let spinner = gtk4::Spinner::builder().spinning(false).build();
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

        let progress = gtk4::ProgressBar::builder().hexpand(true).build();
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
            .transition_duration(crate::ui::motion::STANDARD_MS)
            .child(&container)
            .reveal_child(false)
            .build();

        let on_cancel: OnCancelSlot = Rc::new(RefCell::new(None));

        // Right-click (button 3) on the card triggers cancel.
        let click = gtk4::GestureClick::new();
        click.set_button(3);
        let on_cancel_click = on_cancel.clone();
        click.connect_released(move |_, _, _, _| {
            let callback = on_cancel_click.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        });
        container.add_controller(click);

        // Long-press on the card triggers cancel (touchscreen support).
        let long_press = gtk4::GestureLongPress::new();
        let on_cancel_lp = on_cancel.clone();
        long_press.connect_pressed(move |_, _, _| {
            let callback = on_cancel_lp.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        });
        container.add_controller(long_press);

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
                on_cancel,
            }),
        }
    }

    pub(in crate::ui) fn downgrade(&self) -> WeakScanProgressView {
        WeakScanProgressView(Rc::downgrade(&self.inner))
    }

    /// Sets the callback invoked when the user right-clicks or long-presses
    /// the scan card. Intended to be wired to `ScanControls::request_cancel`.
    pub(in crate::ui) fn set_on_cancel(&self, callback: impl Fn() + 'static) {
        *self.inner.on_cancel.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Revealer {
        &self.inner.revealer
    }

    pub(in crate::ui) fn show(&self, progress: &ScanProgress) {
        let state = view_state(progress);
        self.inner
            .title
            .set_label(&strings::text(strings::SCAN_CARD_TITLE));
        self.inner.spinner.set_spinning(true);
        self.inner.revealer.set_reveal_child(true);
        self.inner.progress.set_visible(true);

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
                } else {
                    self.inner.detail.set_label("");
                    self.inner.detail.set_visible(false);
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
                self.inner
                    .container
                    .set_tooltip_text(Some(&strings::scan_card_tooltip(remaining)));
            }
            _ => {
                self.inner.container.set_tooltip_text(None);
            }
        }
    }

    /// Shows determinate progress with custom title and detail strings,
    /// independent of the `ScanProgress` enum. Used for the cover download
    /// batch whose progress model (`BatchProgress`) lives in the UI layer.
    pub(in crate::ui) fn show_batch(&self, title: &str, detail: &str, fraction: f64) {
        self.cancel_pulsing();
        self.inner.title.set_label(title);
        self.inner.spinner.set_spinning(true);
        self.inner.revealer.set_reveal_child(true);
        self.inner.progress.set_visible(true);
        self.inner.progress.set_fraction(fraction.clamp(0.0, 1.0));
        let pct = format!("{}%", (fraction * 100.0).round() as u32);
        self.inner.percent.set_label(&pct);
        if detail.is_empty() {
            self.inner.detail.set_label("");
            self.inner.detail.set_visible(false);
        } else {
            self.inner.detail.set_label(detail);
            self.inner.detail.set_visible(true);
        }
        self.inner.phase.set(DisplayPhase::Scanning);
        self.inner.container.set_tooltip_text(None);
    }

    pub(in crate::ui) fn show_unavailable(&self, root: &Path) {
        let state = unavailable_state(root);
        self.cancel_pulsing();
        self.inner.phase.set(DisplayPhase::Hidden);
        self.inner.spinner.set_spinning(false);
        self.inner.title.set_label(&state.title);
        self.inner.percent.set_label("");
        self.inner.progress.set_fraction(0.0);
        self.inner.progress.set_visible(false);
        self.inner.detail.set_label(&state.detail);
        self.inner.detail.set_visible(true);
        self.inner.container.set_tooltip_text(None);
        self.inner.revealer.set_reveal_child(true);
    }

    pub(in crate::ui) fn finish(&self) {
        self.cancel_pulsing();
        self.inner.phase.set(DisplayPhase::Hidden);
        self.inner.spinner.set_spinning(false);
        self.inner.revealer.set_reveal_child(false);
        self.inner.progress.set_fraction(0.0);
    }

    fn start_pulsing(&self) -> bool {
        let generation = self.inner.pulse_generation.get().wrapping_add(1);
        self.inner.pulse_generation.set(generation);
        self.inner.progress.set_fraction(0.0);
        if !crate::ui::motion::animations_enabled() {
            return false;
        }
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
            if !crate::ui::motion::animations_enabled() {
                progress.set_fraction(0.0);
                return glib::ControlFlow::Break;
            }
            progress.pulse();
            glib::ControlFlow::Continue
        });
        true
    }

    fn cancel_pulsing(&self) {
        self.inner
            .pulse_generation
            .set(self.inner.pulse_generation.get().wrapping_add(1));
    }
}

/// A lightweight progress indicator embedded in the EmptyLibrary status page.
/// Shown during a first scan so the user sees scanning feedback even before
/// any tracks have appeared in the list. Same `show`/`finish` interface as
/// `ScanProgressView` so `ScanControls` can push to it uniformly.
#[derive(Clone)]
pub(in crate::ui) struct EmptyScanIndicator {
    inner: Rc<EmptyScanWidgets>,
}

struct EmptyScanWidgets {
    container: gtk4::Box,
    spinner: gtk4::Spinner,
    label: gtk4::Label,
}

impl EmptyScanIndicator {
    pub(in crate::ui) fn new() -> Self {
        let spinner = gtk4::Spinner::builder().spinning(false).build();
        spinner.add_css_class("scan-card-spinner");

        let label = gtk4::Label::builder().label("").build();
        label.add_css_class("dim-label");

        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        container.set_halign(gtk4::Align::Center);
        container.append(&spinner);
        container.append(&label);
        container.set_visible(false);

        Self {
            inner: Rc::new(EmptyScanWidgets {
                container,
                spinner,
                label,
            }),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.inner.container
    }

    pub(in crate::ui) fn show(&self, progress: &ScanProgress) {
        self.inner.spinner.set_spinning(true);
        self.inner.container.set_visible(true);
        match progress {
            ScanProgress::Discovering => {
                self.inner
                    .label
                    .set_label(&strings::text(strings::SCAN_DISCOVERING));
            }
            ScanProgress::Scanning {
                processed, total, ..
            } => {
                self.inner
                    .label
                    .set_label(&strings::scan_progress(*processed, *total));
            }
            ScanProgress::Fetching { done, total } => {
                self.inner
                    .label
                    .set_label(&strings::fetch_progress(*done, *total));
            }
        }
    }

    pub(in crate::ui) fn finish(&self) {
        self.inner.spinner.set_spinning(false);
        self.inner.container.set_visible(false);
    }

    pub(in crate::ui) fn downgrade(&self) -> WeakEmptyScanIndicator {
        WeakEmptyScanIndicator(Rc::downgrade(&self.inner))
    }
}

#[derive(Clone)]
pub(in crate::ui) struct WeakEmptyScanIndicator(Weak<EmptyScanWidgets>);

impl WeakEmptyScanIndicator {
    pub(in crate::ui) fn upgrade(&self) -> Option<EmptyScanIndicator> {
        self.0.upgrade().map(|inner| EmptyScanIndicator { inner })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use reprise_core::library::scanner::ScanProgress;

    use super::{unavailable_state, view_state, ProgressMode, ScanProgressView};

    #[test]
    fn unavailable_root_replaces_progress_with_an_honest_mount_status() {
        let state = unavailable_state(std::path::Path::new("/media/NAS/Music"));
        assert_eq!(state.title, "Library folder unavailable");
        assert_eq!(state.detail, "/media/NAS/Music not mounted");
    }

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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mot_7_disabled_animations_never_start_the_scan_pulse_timer() {
        gtk4::init().unwrap();
        let settings = gtk4::Settings::default().unwrap();
        let previous = settings.is_gtk_enable_animations();
        settings.set_gtk_enable_animations(false);
        let view = ScanProgressView::new();

        assert!(!view.start_pulsing());
        assert_eq!(view.inner.progress.fraction(), 0.0);
        assert_eq!(
            view.inner.revealer.transition_duration(),
            crate::ui::motion::STANDARD_MS
        );

        settings.set_gtk_enable_animations(previous);
    }
}
