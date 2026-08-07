//! Scan status presentation embedded in the Preferences dialog chrome.

use std::cell::Cell;
use std::path::Path;
use std::rc::{Rc, Weak};
use std::time::Instant;

use gtk4::glib;
use reprise_core::library::scanner::ScanProgress;

use super::scan_chip::ScanChip;
use super::scan_edge_line::ScanEdgeLine;
use super::scan_progress::remaining_visible_time;
use super::strings;

#[derive(Clone, Copy, Debug, PartialEq)]
enum EdgeState {
    Indeterminate,
    Fraction(f64),
}

#[derive(Debug, PartialEq)]
struct ChromeState {
    line: EdgeState,
    label: String,
    tooltip: Option<String>,
}

fn fraction(done: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0)
    }
}

fn percent(fraction: f64) -> u32 {
    (fraction.clamp(0.0, 1.0) * 100.0).round() as u32
}

fn display_name(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
}

fn chrome_state(progress: &ScanProgress) -> ChromeState {
    match progress {
        ScanProgress::Discovering => ChromeState {
            line: EdgeState::Indeterminate,
            label: strings::text(strings::SCAN_CHIP_SCANNING),
            tooltip: None,
        },
        ScanProgress::Scanning {
            total: None,
            current_path,
            ..
        } => ChromeState {
            line: EdgeState::Indeterminate,
            label: strings::text(strings::SCAN_CHIP_SCANNING),
            tooltip: display_name(current_path),
        },
        ScanProgress::Scanning {
            processed,
            total: Some(total),
            ..
        } => {
            let fraction = fraction(*processed, *total);
            ChromeState {
                line: EdgeState::Fraction(fraction),
                label: strings::scan_chip_scanning(percent(fraction)),
                tooltip: Some(strings::scan_progress(*processed, *total)),
            }
        }
        ScanProgress::Fetching { done, total } => {
            let fraction = fraction(*done, *total);
            ChromeState {
                line: EdgeState::Fraction(fraction),
                label: strings::scan_chip_fetching(percent(fraction)),
                tooltip: Some(strings::scan_card_tooltip(total.saturating_sub(*done))),
            }
        }
    }
}

fn batch_label(title: &str, fraction: f64) -> String {
    strings::scan_chip_batch(title, percent(fraction))
}

#[derive(Clone)]
pub(in crate::ui) struct ScanChromeView {
    inner: Rc<ScanChromeWidgets>,
}

struct ScanChromeWidgets {
    chip: ScanChip,
    line: ScanEdgeLine,
    visibility_generation: Rc<Cell<u64>>,
    visible_since: Cell<Option<Instant>>,
}

#[derive(Clone)]
pub(in crate::ui) struct WeakScanChromeView(Weak<ScanChromeWidgets>);

impl WeakScanChromeView {
    pub(in crate::ui) fn upgrade(&self) -> Option<ScanChromeView> {
        self.0.upgrade().map(|inner| ScanChromeView { inner })
    }
}

impl ScanChromeView {
    pub(in crate::ui) fn new() -> Self {
        Self {
            inner: Rc::new(ScanChromeWidgets {
                chip: ScanChip::new(),
                line: ScanEdgeLine::new(),
                visibility_generation: Rc::new(Cell::new(0)),
                visible_since: Cell::new(None),
            }),
        }
    }

    pub(in crate::ui) fn chip_widget(&self) -> &gtk4::Widget {
        self.inner.chip.widget()
    }

    pub(in crate::ui) fn line_widget(&self) -> &gtk4::Widget {
        self.inner.line.widget()
    }

    pub(in crate::ui) fn set_on_activate(&self, callback: impl Fn() + 'static) {
        self.inner.chip.set_on_activate(callback);
    }

    pub(in crate::ui) fn set_on_cancel(&self, callback: impl Fn() + 'static) {
        self.inner.chip.set_on_cancel(callback);
    }

    pub(in crate::ui) fn show(&self, progress: &ScanProgress) {
        self.begin_visibility();
        let state = chrome_state(progress);
        match state.line {
            EdgeState::Indeterminate => self.inner.line.set_indeterminate(),
            EdgeState::Fraction(fraction) => self.inner.line.set_fraction(fraction),
        }
        self.inner
            .chip
            .set_running(&state.label, state.tooltip.as_deref());
    }

    pub(in crate::ui) fn show_batch(&self, title: &str, detail: &str, fraction: f64) {
        self.begin_visibility();
        self.inner.line.set_fraction(fraction);
        self.inner.chip.set_running(
            &batch_label(title, fraction),
            (!detail.is_empty()).then_some(detail),
        );
    }

    pub(in crate::ui) fn show_unavailable(&self, root: &Path) {
        self.begin_visibility();
        self.inner.line.hide();
        self.inner.chip.set_warning(
            &strings::text(strings::SCAN_CHIP_WARNING),
            Some(&strings::library_folder_not_mounted(
                &root.to_string_lossy(),
            )),
        );
    }

    pub(in crate::ui) fn finish(&self) {
        let Some(visible_since) = self.inner.visible_since.take() else {
            return;
        };
        let generation = self.inner.visibility_generation.get();
        if let Some(delay) = remaining_visible_time(visible_since.elapsed()) {
            let weak = self.downgrade();
            let visibility_generation = self.inner.visibility_generation.clone();
            glib::timeout_add_local_once(delay, move || {
                if visibility_generation.get() == generation {
                    if let Some(view) = weak.upgrade() {
                        view.hide_now();
                    }
                }
            });
        } else {
            self.hide_now();
        }
    }

    pub(in crate::ui) fn downgrade(&self) -> WeakScanChromeView {
        WeakScanChromeView(Rc::downgrade(&self.inner))
    }

    fn begin_visibility(&self) {
        self.inner
            .visibility_generation
            .set(self.inner.visibility_generation.get().wrapping_add(1));
        if self.inner.visible_since.get().is_none() {
            self.inner.visible_since.set(Some(Instant::now()));
        }
    }

    fn hide_now(&self) {
        self.inner.chip.hide();
        self.inner.line.hide();
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use reprise_core::library::scanner::ScanProgress;

    use super::*;

    #[test]
    fn scan_phases_map_to_compact_chrome_without_losing_detail() {
        assert_eq!(
            chrome_state(&ScanProgress::Discovering),
            ChromeState {
                line: EdgeState::Indeterminate,
                label: "Scanning".to_owned(),
                tooltip: None,
            }
        );
        assert_eq!(
            chrome_state(&ScanProgress::Scanning {
                processed: 39,
                total: Some(100),
                current_path: PathBuf::from("/music/track.flac"),
            }),
            ChromeState {
                line: EdgeState::Fraction(0.39),
                label: "Scanning · 39%".to_owned(),
                tooltip: Some("39 of 100 files scanned".to_owned()),
            }
        );
        assert_eq!(
            chrome_state(&ScanProgress::Fetching { done: 8, total: 10 }),
            ChromeState {
                line: EdgeState::Fraction(0.8),
                label: "Fetching · 80%".to_owned(),
                tooltip: Some("Covers & lyrics: 2 queued".to_owned()),
            }
        );
    }

    #[test]
    fn unknown_scan_total_uses_the_filename_and_indeterminate_line() {
        assert_eq!(
            chrome_state(&ScanProgress::Scanning {
                processed: 4,
                total: None,
                current_path: PathBuf::from("/music/Album/song.flac"),
            }),
            ChromeState {
                line: EdgeState::Indeterminate,
                label: "Scanning".to_owned(),
                tooltip: Some("song.flac".to_owned()),
            }
        );
    }

    #[test]
    fn batch_label_rounds_and_clamps_the_visible_percentage() {
        assert_eq!(
            batch_label("Checking missing lyrics…", 0.386),
            "Checking missing lyrics… · 39%"
        );
        assert_eq!(batch_label("Done", 2.0), "Done · 100%");
    }
}
