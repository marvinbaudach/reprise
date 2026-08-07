//! Shared scan cancellation, completion, and progress fan-out.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::scanner::ScanProgress;

use super::scan_chrome::{ScanChromeView, WeakScanChromeView};
use super::scan_progress::{EmptyScanIndicator, ScanProgressView, WeakEmptyScanIndicator};
use super::strings;

type OnScanComplete = Rc<dyn Fn()>;
type OnCancelRequested = Rc<dyn Fn()>;
type OnPresentationChanged = dyn Fn(Option<String>);

pub(in crate::ui) struct ScanPresentationSubscription {
    _callback: Rc<OnPresentationChanged>,
}

trait ScanPresentation {
    fn show(&self, progress: &ScanProgress);
    fn show_batch(&self, title: &str, detail: &str, fraction: f64);
    fn show_unavailable(&self, root: &Path);
    fn finish(&self);
}

impl ScanPresentation for ScanProgressView {
    fn show(&self, progress: &ScanProgress) {
        ScanProgressView::show(self, progress);
    }

    fn show_batch(&self, title: &str, detail: &str, fraction: f64) {
        ScanProgressView::show_batch(self, title, detail, fraction);
    }

    fn show_unavailable(&self, root: &Path) {
        ScanProgressView::show_unavailable(self, root);
    }

    fn finish(&self) {
        ScanProgressView::finish(self);
    }
}

impl ScanPresentation for ScanChromeView {
    fn show(&self, progress: &ScanProgress) {
        ScanChromeView::show(self, progress);
    }

    fn show_batch(&self, title: &str, detail: &str, fraction: f64) {
        ScanChromeView::show_batch(self, title, detail, fraction);
    }

    fn show_unavailable(&self, root: &Path) {
        ScanChromeView::show_unavailable(self, root);
    }

    fn finish(&self) {
        ScanChromeView::finish(self);
    }
}

#[derive(Clone)]
enum ScanSurface {
    Card(ScanProgressView),
    Chrome(ScanChromeView),
}

impl ScanPresentation for ScanSurface {
    fn show(&self, progress: &ScanProgress) {
        match self {
            Self::Card(view) => view.show(progress),
            Self::Chrome(view) => view.show(progress),
        }
    }

    fn show_batch(&self, title: &str, detail: &str, fraction: f64) {
        match self {
            Self::Card(view) => view.show_batch(title, detail, fraction),
            Self::Chrome(view) => view.show_batch(title, detail, fraction),
        }
    }

    fn show_unavailable(&self, root: &Path) {
        match self {
            Self::Card(view) => view.show_unavailable(root),
            Self::Chrome(view) => view.show_unavailable(root),
        }
    }

    fn finish(&self) {
        match self {
            Self::Card(view) => view.finish(),
            Self::Chrome(view) => view.finish(),
        }
    }
}

#[derive(Clone)]
enum PresentationState {
    Scan(ScanProgress),
    Batch {
        title: String,
        detail: String,
        fraction: f64,
    },
    Unavailable(PathBuf),
}

fn replay_presentation(
    state: Option<&PresentationState>,
    presentation: &(impl ScanPresentation + ?Sized),
) {
    match state {
        Some(PresentationState::Scan(progress)) => presentation.show(progress),
        Some(PresentationState::Batch {
            title,
            detail,
            fraction,
        }) => presentation.show_batch(title, detail, *fraction),
        Some(PresentationState::Unavailable(root)) => presentation.show_unavailable(root),
        None => {}
    }
}

fn presentation_detail(state: Option<&PresentationState>) -> Option<String> {
    match state {
        Some(PresentationState::Scan(ScanProgress::Discovering)) => {
            Some(strings::text(strings::SCAN_DISCOVERING))
        }
        Some(PresentationState::Scan(ScanProgress::Scanning {
            total: None,
            current_path,
            ..
        })) => current_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .or_else(|| Some(strings::text(strings::SCAN_DISCOVERING))),
        Some(PresentationState::Scan(ScanProgress::Scanning {
            processed,
            total: Some(total),
            ..
        })) => Some(strings::scan_progress(*processed, *total)),
        Some(PresentationState::Scan(ScanProgress::Fetching { done, total })) => {
            Some(strings::scan_card_tooltip(total.saturating_sub(*done)))
        }
        Some(PresentationState::Batch { title, detail, .. }) => Some(if detail.is_empty() {
            title.clone()
        } else {
            detail.clone()
        }),
        Some(PresentationState::Unavailable(root)) => {
            Some(strings::library_folder_not_mounted(&root.to_string_lossy()))
        }
        None => None,
    }
}

fn cloned_slot<T: Clone>(slot: &RefCell<Option<T>>) -> Option<T> {
    slot.borrow().clone()
}

#[derive(Clone, Default)]
pub(in crate::ui) struct ScanCompletion(Rc<RefCell<Vec<OnScanComplete>>>);

impl ScanCompletion {
    pub(in crate::ui) fn add(&self, callback: impl Fn() + 'static) {
        self.0.borrow_mut().push(Rc::new(callback));
    }

    pub(in crate::ui) fn notify(&self) {
        let callbacks = self.0.borrow().clone();
        for callback in callbacks {
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
    foreground_progress: Rc<RefCell<Vec<WeakScanChromeView>>>,
    last_presentation: Rc<RefCell<Option<PresentationState>>>,
    presentation_observers: Rc<RefCell<Vec<Weak<OnPresentationChanged>>>>,
    completion: ScanCompletion,
    cancellation: ScanCancellation,
    on_cancel_requested: Rc<RefCell<Vec<OnCancelRequested>>>,
    empty_indicator: Rc<RefCell<Option<WeakEmptyScanIndicator>>>,
    sidebar_toggle: Rc<RefCell<Option<glib::WeakRef<gtk4::ToggleButton>>>>,
    library_root_unavailable: Rc<Cell<bool>>,
}

impl ScanControls {
    pub(in crate::ui) fn new(button: &gtk4::Button, progress: &ScanProgressView) -> Self {
        Self {
            button: button.clone(),
            primary_progress: progress.clone(),
            foreground_progress: Rc::new(RefCell::new(Vec::new())),
            last_presentation: Rc::new(RefCell::new(None)),
            presentation_observers: Rc::new(RefCell::new(Vec::new())),
            completion: ScanCompletion::default(),
            cancellation: ScanCancellation::default(),
            on_cancel_requested: Rc::new(RefCell::new(Vec::new())),
            empty_indicator: Rc::new(RefCell::new(None)),
            sidebar_toggle: Rc::new(RefCell::new(None)),
            library_root_unavailable: Rc::new(Cell::new(false)),
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
        let observers = self.on_cancel_requested.borrow().clone();
        for observer in observers {
            observer();
        }
    }

    /// Registers a background job that the scan card's cancel gesture should
    /// also stop. The observer owns the decision whether the gesture was meant
    /// for it — the scan's own cancellation flag stays private to the scan, so
    /// a batch never cancels a scan and a scan never cancels a batch.
    pub(in crate::ui) fn add_on_cancel_requested(&self, callback: impl Fn() + 'static) {
        self.on_cancel_requested
            .borrow_mut()
            .push(Rc::new(callback));
    }

    pub(in crate::ui) fn reset_cancel(&self) {
        self.cancellation.reset();
    }

    pub(in crate::ui) fn is_cancel_requested(&self) -> bool {
        self.cancellation.is_requested()
    }

    pub(in crate::ui) fn attach_chrome_view(&self, progress: &ScanChromeView) {
        self.foreground_progress
            .borrow_mut()
            .push(progress.downgrade());
        let current = self.last_presentation.borrow().clone();
        replay_presentation(current.as_ref(), progress);
    }

    pub(in crate::ui) fn subscribe_presentation(
        &self,
        callback: impl Fn(Option<String>) + 'static,
    ) -> ScanPresentationSubscription {
        let callback: Rc<OnPresentationChanged> = Rc::new(callback);
        self.presentation_observers
            .borrow_mut()
            .push(Rc::downgrade(&callback));
        let detail = self.current_presentation_detail();
        callback(detail);
        ScanPresentationSubscription {
            _callback: callback,
        }
    }

    pub(in crate::ui) fn current_presentation_detail(&self) -> Option<String> {
        let current = self.last_presentation.borrow();
        presentation_detail(current.as_ref())
    }

    #[cfg(test)]
    pub(in crate::ui) fn foreground_progress_count(&self) -> usize {
        self.foreground_progress.borrow().len()
    }

    fn live_progress_views(&self) -> Vec<ScanSurface> {
        let foreground = {
            let mut weak_views = self.foreground_progress.borrow_mut();
            let mut live = Vec::with_capacity(weak_views.len());
            weak_views.retain(|weak| match weak.upgrade() {
                Some(view) => {
                    live.push(ScanSurface::Chrome(view));
                    true
                }
                None => false,
            });
            live
        };
        std::iter::once(ScanSurface::Card(self.primary_progress.clone()))
            .chain(foreground)
            .collect()
    }

    pub(in crate::ui) fn show_progress(&self, progress: &ScanProgress) {
        let phase_changed = {
            let current = self.last_presentation.borrow();
            !matches!(
                (current.as_ref(), progress),
                (
                    Some(PresentationState::Scan(ScanProgress::Discovering)),
                    ScanProgress::Discovering
                ) | (
                    Some(PresentationState::Scan(ScanProgress::Scanning { .. })),
                    ScanProgress::Scanning { .. }
                ) | (
                    Some(PresentationState::Scan(ScanProgress::Fetching { .. })),
                    ScanProgress::Fetching { .. }
                )
            )
        };
        *self.last_presentation.borrow_mut() = Some(PresentationState::Scan(progress.clone()));
        self.notify_presentation_changed();
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
        self.last_presentation.borrow_mut().take();
        self.notify_presentation_changed();
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
        *self.last_presentation.borrow_mut() =
            Some(PresentationState::Unavailable(root.to_path_buf()));
        self.notify_presentation_changed();
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

    pub(in crate::ui) fn show_batch_progress(&self, title: &str, detail: &str, fraction: f64) {
        *self.last_presentation.borrow_mut() = Some(PresentationState::Batch {
            title: title.to_owned(),
            detail: detail.to_owned(),
            fraction,
        });
        self.notify_presentation_changed();
        for view in self.live_progress_views() {
            view.show_batch(title, detail, fraction);
        }
        if let Some(button) = self.sidebar_toggle() {
            button.set_tooltip_text(Some(title));
        }
    }

    pub(in crate::ui) fn add_on_complete(&self, callback: impl Fn() + 'static) {
        self.completion.add(callback);
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

    fn notify_presentation_changed(&self) {
        let detail = self.current_presentation_detail();
        let callbacks = {
            let mut weak_callbacks = self.presentation_observers.borrow_mut();
            let mut callbacks = Vec::with_capacity(weak_callbacks.len());
            weak_callbacks.retain(|weak| match weak.upgrade() {
                Some(callback) => {
                    callbacks.push(callback);
                    true
                }
                None => false,
            });
            callbacks
        };
        for callback in callbacks {
            callback(detail.clone());
        }
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
        // Without an estimate there is no percentage to put in the tooltip
        // either — the same discovery wording the bar itself falls back to.
        ScanProgress::Scanning { total: None, .. } => strings::scan_tooltip_discovering(),
        ScanProgress::Scanning {
            processed,
            total: Some(total),
            ..
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
    use std::path::Path;

    use reprise_core::library::scanner::ScanProgress;

    use super::{replay_presentation, PresentationState, ScanPresentation};

    #[derive(Debug, PartialEq)]
    enum RecordedState {
        Batch(String, String, f64),
        Unavailable(String),
    }

    #[derive(Default)]
    struct RecordingPresentation(RefCell<Vec<RecordedState>>);

    impl ScanPresentation for RecordingPresentation {
        fn show(&self, _progress: &ScanProgress) {}

        fn show_batch(&self, title: &str, detail: &str, fraction: f64) {
            self.0.borrow_mut().push(RecordedState::Batch(
                title.to_owned(),
                detail.to_owned(),
                fraction,
            ));
        }

        fn show_unavailable(&self, root: &Path) {
            self.0.borrow_mut().push(RecordedState::Unavailable(
                root.to_string_lossy().into_owned(),
            ));
        }

        fn finish(&self) {}
    }

    #[test]
    fn cloned_slot_releases_the_borrow_before_reentrant_work() {
        let slot = RefCell::new(Some(String::from("indicator")));

        let cloned = super::cloned_slot(&slot);
        slot.borrow_mut().take();

        assert_eq!(cloned.as_deref(), Some("indicator"));
        assert!(slot.borrow().is_none());
    }

    #[test]
    fn attaching_replays_an_active_batch_and_an_unavailable_root() {
        let presentation = RecordingPresentation::default();
        replay_presentation(
            Some(&PresentationState::Batch {
                title: "Checking missing lyrics…".to_owned(),
                detail: "748 of 1,909 checked · 6 cached · 113 unavailable".to_owned(),
                fraction: 0.39,
            }),
            &presentation,
        );
        replay_presentation(
            Some(&PresentationState::Unavailable("/media/Music".into())),
            &presentation,
        );

        assert_eq!(
            presentation.0.into_inner(),
            vec![
                RecordedState::Batch(
                    "Checking missing lyrics…".to_owned(),
                    "748 of 1,909 checked · 6 cached · 113 unavailable".to_owned(),
                    0.39,
                ),
                RecordedState::Unavailable("/media/Music".to_owned()),
            ]
        );
    }
}
