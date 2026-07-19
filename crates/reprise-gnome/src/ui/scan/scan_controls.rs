//! Shared scan cancellation, progress fan-out, and trigger state.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::scanner::ScanProgress;

use super::audio_analysis_runtime::AudioAnalysisRuntime;
use super::scan_progress::{
    EmptyScanIndicator, ScanProgressView, WeakEmptyScanIndicator, WeakScanProgressView,
};
use super::strings;

type OnScanComplete = Rc<dyn Fn()>;
type OnScanStateChanged = Rc<dyn Fn(bool)>;

fn cloned_slot<T: Clone>(slot: &RefCell<Option<T>>) -> Option<T> {
    slot.borrow().clone()
}

#[derive(Clone, Default)]
pub(in crate::ui) struct ScanCompletion(Rc<RefCell<Option<OnScanComplete>>>);

impl ScanCompletion {
    pub(in crate::ui) fn set(&self, callback: impl Fn() + 'static) {
        self.0.borrow_mut().replace(Rc::new(callback));
    }

    pub(in crate::ui) fn notify(&self) {
        let callback = self.0.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
    }
}

#[derive(Clone, Default)]
pub(in crate::ui) struct ScanCancellation(Arc<AtomicBool>);

impl ScanCancellation {
    pub(in crate::ui) fn request(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub(in crate::ui) fn reset(&self) {
        self.0.store(false, Ordering::Relaxed);
    }

    pub(in crate::ui) fn is_requested(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub(in crate::ui) struct ScanControls {
    pub(in crate::ui) button: gtk4::Button,
    primary_progress: ScanProgressView,
    pub(in crate::ui) foreground_progress: Rc<RefCell<Vec<WeakScanProgressView>>>,
    current_progress: Rc<RefCell<Option<ScanProgress>>>,
    completion: ScanCompletion,
    cancellation: ScanCancellation,
    on_scan_state_changed: Rc<RefCell<Option<OnScanStateChanged>>>,
    empty_indicator: Rc<RefCell<Option<WeakEmptyScanIndicator>>>,
    sidebar_toggle: Rc<RefCell<Option<glib::WeakRef<gtk4::ToggleButton>>>>,
    library_root_unavailable: Rc<Cell<bool>>,
    audio_analysis: Rc<RefCell<Option<AudioAnalysisRuntime>>>,
}

impl ScanControls {
    pub(in crate::ui) fn new(button: &gtk4::Button, progress: &ScanProgressView) -> Self {
        Self {
            button: button.clone(),
            primary_progress: progress.clone(),
            foreground_progress: Rc::new(RefCell::new(Vec::new())),
            current_progress: Rc::new(RefCell::new(None)),
            completion: ScanCompletion::default(),
            cancellation: ScanCancellation::default(),
            on_scan_state_changed: Rc::new(RefCell::new(None)),
            empty_indicator: Rc::new(RefCell::new(None)),
            sidebar_toggle: Rc::new(RefCell::new(None)),
            library_root_unavailable: Rc::new(Cell::new(false)),
            audio_analysis: Rc::new(RefCell::new(None)),
        }
    }

    pub(in crate::ui) fn set_empty_indicator(&self, indicator: &EmptyScanIndicator) {
        *self.empty_indicator.borrow_mut() = Some(indicator.downgrade());
    }

    pub(in crate::ui) fn set_sidebar_toggle(&self, button: &gtk4::ToggleButton) {
        let weak = glib::WeakRef::new();
        weak.set(Some(button));
        *self.sidebar_toggle.borrow_mut() = Some(weak);
    }

    pub(in crate::ui) fn is_scanning(&self) -> bool {
        !self.button.is_sensitive()
    }

    pub(in crate::ui) fn set_library_root_unavailable(&self, unavailable: bool) {
        self.library_root_unavailable.set(unavailable);
    }

    pub(in crate::ui) fn library_root_unavailable(&self) -> bool {
        self.library_root_unavailable.get()
    }

    pub(in crate::ui) fn request_cancel(&self) {
        self.cancellation.request();
    }

    pub(in crate::ui) fn reset_cancel(&self) {
        self.cancellation.reset();
    }

    pub(in crate::ui) fn is_cancel_requested(&self) -> bool {
        self.cancellation.is_requested()
    }

    pub(in crate::ui) fn set_audio_analysis(&self, runtime: &AudioAnalysisRuntime) {
        self.audio_analysis.borrow_mut().replace(runtime.clone());
    }

    pub(in crate::ui) fn wake_audio_analysis(&self) {
        let runtime = self.audio_analysis.borrow().clone();
        if let Some(runtime) = runtime {
            runtime.wake();
        }
    }

    pub(in crate::ui) fn set_on_scan_state_changed(&self, callback: impl Fn(bool) + 'static) {
        *self.on_scan_state_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn notify_scan_state(&self) {
        let callback = self.on_scan_state_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(self.is_scanning());
        }
    }

    pub(in crate::ui) fn attach_progress_view(&self, progress: &ScanProgressView) {
        self.foreground_progress
            .borrow_mut()
            .push(progress.downgrade());
        let current = self.current_progress.borrow().clone();
        if let Some(current) = current {
            progress.show(&current);
        }
    }

    fn live_progress_views(&self) -> Vec<ScanProgressView> {
        let foreground = {
            let mut weak_views = self.foreground_progress.borrow_mut();
            let mut live = Vec::with_capacity(weak_views.len());
            weak_views.retain(|weak| match weak.upgrade() {
                Some(view) => {
                    live.push(view);
                    true
                }
                None => false,
            });
            live
        };
        std::iter::once(self.primary_progress.clone())
            .chain(foreground)
            .collect()
    }

    pub(in crate::ui) fn show_progress(&self, progress: &ScanProgress) {
        let phase_changed = {
            let current = self.current_progress.borrow();
            !matches!(
                (current.as_ref(), progress),
                (Some(ScanProgress::Discovering), ScanProgress::Discovering)
                    | (
                        Some(ScanProgress::Scanning { .. }),
                        ScanProgress::Scanning { .. }
                    )
                    | (
                        Some(ScanProgress::Fetching { .. }),
                        ScanProgress::Fetching { .. }
                    )
            )
        };
        *self.current_progress.borrow_mut() = Some(progress.clone());
        log_progress(progress, phase_changed);
        for view in self.live_progress_views() {
            view.show(progress);
        }
        let indicator = cloned_slot(&self.empty_indicator)
            .as_ref()
            .and_then(WeakEmptyScanIndicator::upgrade);
        if let Some(indicator) = indicator {
            indicator.show(progress);
        }
        if let Some(button) = self.sidebar_toggle() {
            button.set_tooltip_text(Some(&progress_tooltip(progress)));
        }
    }

    pub(in crate::ui) fn finish_progress(&self) {
        self.current_progress.borrow_mut().take();
        for view in self.live_progress_views() {
            view.finish();
        }
        let indicator = cloned_slot(&self.empty_indicator)
            .as_ref()
            .and_then(WeakEmptyScanIndicator::upgrade);
        if let Some(indicator) = indicator {
            indicator.finish();
        }
        if let Some(button) = self.sidebar_toggle() {
            button.set_tooltip_text(Some(&strings::text(strings::SIDEBAR_TOGGLE)));
        }
    }

    pub(in crate::ui) fn show_root_unavailable(&self, root: &std::path::Path) {
        for view in self.live_progress_views() {
            view.show_unavailable(root);
        }
        let indicator = cloned_slot(&self.empty_indicator)
            .as_ref()
            .and_then(WeakEmptyScanIndicator::upgrade);
        if let Some(indicator) = indicator {
            indicator.finish();
        }
        if let Some(button) = self.sidebar_toggle() {
            button.set_tooltip_text(Some(&strings::unavailable_title()));
        }
    }

    pub(in crate::ui) fn show_cover_progress(&self, title: &str, detail: &str, fraction: f64) {
        for view in self.live_progress_views() {
            view.show_batch(title, detail, fraction);
        }
        if let Some(button) = self.sidebar_toggle() {
            button.set_tooltip_text(Some(title));
        }
    }

    pub(in crate::ui) fn set_on_complete(&self, callback: impl Fn() + 'static) {
        self.completion.set(callback);
    }

    pub(in crate::ui) fn notify_complete(&self) {
        self.completion.notify();
    }

    fn sidebar_toggle(&self) -> Option<gtk4::ToggleButton> {
        self.sidebar_toggle
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
    }
}

fn log_progress(progress: &ScanProgress, phase_changed: bool) {
    match progress {
        ScanProgress::Discovering => {
            if phase_changed {
                tracing::info!("scan progress: discovering");
            }
        }
        ScanProgress::Scanning {
            processed,
            total,
            current_path,
        } => {
            if phase_changed {
                tracing::info!(
                    processed,
                    total,
                    file = %current_path.display(),
                    "scan progress: scanning"
                );
            } else {
                tracing::debug!(
                    processed,
                    total,
                    file = %current_path.display(),
                    "scan progress: scanning"
                );
            }
        }
        ScanProgress::Fetching { done, total } => {
            if phase_changed {
                tracing::info!(done, total, "scan progress: fetching");
            } else {
                tracing::debug!(done, total, "scan progress: fetching");
            }
        }
    }
}

fn progress_tooltip(progress: &ScanProgress) -> String {
    match progress {
        ScanProgress::Discovering => strings::scan_tooltip_discovering(),
        ScanProgress::Scanning {
            processed, total, ..
        } => strings::scan_tooltip_progress(progress_percent(*processed, *total)),
        ScanProgress::Fetching { done, total } => {
            strings::scan_tooltip_progress(progress_percent(*done, *total))
        }
    }
}

fn progress_percent(done: u64, total: u64) -> u32 {
    if total == 0 {
        0
    } else {
        (done as f64 / total as f64 * 100.0).round() as u32
    }
}

#[cfg(test)]
mod refcell_tests {
    use std::cell::RefCell;

    #[test]
    fn cloned_slot_releases_the_borrow_before_reentrant_work() {
        let slot = RefCell::new(Some(String::from("indicator")));

        let cloned = super::cloned_slot(&slot);
        slot.borrow_mut().take();

        assert_eq!(cloned.as_deref(), Some("indicator"));
        assert!(slot.borrow().is_none());
    }
}
