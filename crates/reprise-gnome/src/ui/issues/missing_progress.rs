use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gtk4::prelude::*;

use crate::ui::strings;

const PROGRESS_HEIGHT_PX: u32 = 3;

#[derive(Debug, Clone, PartialEq)]
struct RelinkProgressState {
    title: String,
    percent: String,
    detail: String,
    fraction: f64,
    spinner: bool,
    progress_height_px: u32,
    cancel_label: String,
}

fn relink_progress_state(processed: u32, total: u32, group_size: u32) -> RelinkProgressState {
    let fraction = if total == 0 {
        0.0
    } else {
        (f64::from(processed) / f64::from(total)).clamp(0.0, 1.0)
    };
    RelinkProgressState {
        title: strings::issue_text(strings::MISSING_RELINK_PROGRESS_TITLE),
        percent: format!("{}%", (fraction * 100.0).round() as u32),
        detail: strings::missing_relink_progress_detail(processed, total, group_size),
        fraction,
        spinner: true,
        progress_height_px: PROGRESS_HEIGHT_PX,
        cancel_label: strings::issue_text(strings::CANCEL),
    }
}

type OnActivate = Rc<dyn Fn(reprise_core::view_source::ViewSource)>;

#[derive(Clone, Default)]
struct RelinkProgressActivation(Rc<RefCell<Option<OnActivate>>>);

impl RelinkProgressActivation {
    fn set(&self, callback: impl Fn(reprise_core::view_source::ViewSource) + 'static) {
        *self.0.borrow_mut() = Some(Rc::new(callback));
    }

    fn primary_click(&self) {
        let callback = self.0.borrow().clone();
        if let Some(callback) = callback {
            callback(reprise_core::view_source::ViewSource::Missing);
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct RelinkCancellation(Arc<AtomicBool>);

impl RelinkCancellation {
    pub(super) fn request(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(super) fn token(&self) -> Arc<AtomicBool> {
        self.0.clone()
    }
}

#[derive(Clone)]
pub(super) struct RelinkProgressView {
    inner: Rc<RelinkProgressWidgets>,
}

struct RelinkProgressWidgets {
    revealer: gtk4::Revealer,
    spinner: gtk4::Spinner,
    title: gtk4::Label,
    percent: gtk4::Label,
    progress: gtk4::ProgressBar,
    detail: gtk4::Label,
    open: gtk4::Button,
    cancel: gtk4::Button,
    cancellation: Rc<RefCell<Option<RelinkCancellation>>>,
    activation: RelinkProgressActivation,
    running: Cell<bool>,
}

impl RelinkProgressView {
    pub(super) fn new() -> Self {
        let spinner = gtk4::Spinner::new();
        spinner.add_css_class("scan-card-spinner");
        let title = gtk4::Label::builder()
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .build();
        title.add_css_class("scan-card-title");
        let percent = gtk4::Label::builder().halign(gtk4::Align::End).build();
        percent.add_css_class("scan-card-percent");
        let cancel = gtk4::Button::with_label(&strings::issue_text(strings::CANCEL));
        cancel.add_css_class("flat");
        cancel.add_css_class("scan-card-cancel");
        let open = gtk4::Button::with_label(&strings::text(strings::SIDEBAR_MISSING_FILES));
        open.add_css_class("flat");
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        header.append(&spinner);
        header.append(&title);
        header.append(&percent);
        header.append(&open);
        header.append(&cancel);

        let progress = gtk4::ProgressBar::new();
        progress.update_property(&[gtk4::accessible::Property::Label(&strings::issue_text(
            strings::MISSING_RELINK_PROGRESS_TITLE,
        ))]);
        progress.set_hexpand(true);
        progress.set_height_request(PROGRESS_HEIGHT_PX as i32);
        let detail = gtk4::Label::builder()
            .halign(gtk4::Align::Start)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
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

        let cancellation: Rc<RefCell<Option<RelinkCancellation>>> = Rc::new(RefCell::new(None));
        let cancellation_click = cancellation.clone();
        cancel.connect_clicked(move |_| {
            let current = cancellation_click.borrow().clone();
            if let Some(current) = current {
                current.request();
            }
        });
        let activation = RelinkProgressActivation::default();
        let activate_click = activation.clone();
        open.connect_clicked(move |_| {
            activate_click.primary_click();
        });

        Self {
            inner: Rc::new(RelinkProgressWidgets {
                revealer,
                spinner,
                title,
                percent,
                progress,
                detail,
                open,
                cancel,
                cancellation,
                activation,
                running: Cell::new(false),
            }),
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Revealer {
        &self.inner.revealer
    }

    pub(super) fn set_on_activate(
        &self,
        callback: impl Fn(reprise_core::view_source::ViewSource) + 'static,
    ) {
        self.inner.activation.set(callback);
    }

    pub(super) fn start(&self, group_size: u32, cancellation: RelinkCancellation) -> bool {
        if self.inner.running.replace(true) {
            return false;
        }
        *self.inner.cancellation.borrow_mut() = Some(cancellation);
        self.inner.open.set_sensitive(true);
        self.show(0, 0, group_size);
        self.inner.revealer.set_reveal_child(true);
        true
    }

    pub(super) fn show(&self, processed: u32, total: u32, group_size: u32) {
        let state = relink_progress_state(processed, total, group_size);
        self.inner.title.set_label(&state.title);
        self.inner.percent.set_label(&state.percent);
        self.inner.detail.set_label(&state.detail);
        self.inner.progress.set_fraction(state.fraction);
        self.inner.spinner.set_spinning(state.spinner);
        self.inner.cancel.set_label(&state.cancel_label);
    }

    pub(super) fn finish(&self) {
        self.inner.cancellation.borrow_mut().take();
        self.inner.running.set(false);
        self.inner.open.set_sensitive(false);
        self.inner.spinner.set_spinning(false);
        self.inner.progress.set_fraction(0.0);
        self.inner.revealer.set_reveal_child(false);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gtk4::prelude::*;
    use reprise_core::view_source::ViewSource;

    use super::{
        relink_progress_state, RelinkCancellation, RelinkProgressActivation, RelinkProgressView,
    };

    // UX FB-8: Relink uses the shared sidebar-bottom progress
    // card contract with spinner, title, percent, 3px bar, detail, view
    // navigation, and cancellation.
    #[test]
    fn fb_8_relink_search_uses_the_complete_sidebar_progress_card_contract() {
        let state = relink_progress_state(4, 9, 3);

        assert_eq!(state.title, "Searching for missing tracks");
        assert_eq!(state.percent, "44%");
        assert_eq!(state.detail, "4 of 9 files checked · 3 tracks to relink");
        assert!((state.fraction - (4.0 / 9.0)).abs() < f64::EPSILON);
        assert!(state.spinner);
        assert_eq!(state.progress_height_px, 3);
        assert_eq!(state.cancel_label, "Cancel");

        let activated_target = Rc::new(RefCell::new(None));
        let activated_target_for_callback = activated_target.clone();
        let activation = RelinkProgressActivation::default();
        activation.set(move |target| {
            activated_target_for_callback.borrow_mut().replace(target);
        });
        activation.primary_click();
        assert_eq!(*activated_target.borrow(), Some(ViewSource::Missing));

        let cancellation = RelinkCancellation::default();
        let worker_token = cancellation.token();
        cancellation.request();
        assert!(worker_token.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn relink_progress_primary_gesture_dispatches_missing_navigation() {
        gtk4::init().unwrap();
        let view = RelinkProgressView::new();
        assert_eq!(
            view.widget().transition_duration(),
            crate::ui::motion::STANDARD_MS
        );
        assert_eq!(
            view.widget().transition_type(),
            gtk4::RevealerTransitionType::Crossfade
        );
        let activated_target = Rc::new(RefCell::new(None));
        let activated_target_for_callback = activated_target.clone();
        view.set_on_activate(move |target| {
            activated_target_for_callback.borrow_mut().replace(target);
        });
        view.inner.open.emit_clicked();

        assert_eq!(*activated_target.borrow(), Some(ViewSource::Missing));
    }
}
