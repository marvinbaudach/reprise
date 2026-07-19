use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DoctorJobKind {
    Scan,
    Apply,
    Revert,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProgressPresentation {
    title: String,
    detail: String,
    percent: u32,
}

fn progress_presentation(
    kind: DoctorJobKind,
    completed: usize,
    total: usize,
) -> ProgressPresentation {
    let fraction = if total == 0 {
        0.0
    } else {
        (completed as f64 / total as f64).clamp(0.0, 1.0)
    };
    ProgressPresentation {
        title: strings::text(match kind {
            DoctorJobKind::Scan => strings::DOCTOR_SCANNING,
            DoctorJobKind::Apply => strings::DOCTOR_UPDATING_TAGS,
            DoctorJobKind::Revert => strings::DOCTOR_REVERTING_TAGS,
        }),
        detail: strings::doctor_track_progress(completed, total),
        percent: (fraction * 100.0).round() as u32,
    }
}

type Callback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

#[derive(Clone)]
pub(super) struct DoctorProgressCard {
    revealer: gtk4::Revealer,
    container: gtk4::Box,
    spinner: gtk4::Spinner,
    title: gtk4::Label,
    percent: gtk4::Label,
    progress: gtk4::ProgressBar,
    detail: gtk4::Label,
    cancel: gtk4::Button,
    on_cancel: Callback,
    on_activate: Callback,
}

impl DoctorProgressCard {
    pub(super) fn new() -> Self {
        let spinner = gtk4::Spinner::new();
        spinner.add_css_class("scan-card-spinner");
        let title = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .css_classes(["scan-card-title"])
            .build();
        let percent = gtk4::Label::builder()
            .xalign(1.0)
            .css_classes(["scan-card-percent"])
            .build();
        let cancel = gtk4::Button::builder()
            .label(strings::text(strings::CANCEL))
            .css_classes(["flat"])
            .build();
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        header.append(&spinner);
        header.append(&title);
        header.append(&percent);
        header.append(&cancel);
        let progress = gtk4::ProgressBar::builder().hexpand(true).build();
        progress.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::DOCTOR_PROGRESS,
        ))]);
        let detail = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .css_classes(["scan-card-detail"])
            .build();
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        container.add_css_class("scan-card");
        container.append(&header);
        container.append(&progress);
        container.append(&detail);
        // a11y-semantics: role=button name=library-doctor state=progress action=enter/space
        container.set_focusable(true);
        container.set_accessible_role(gtk4::AccessibleRole::Button);
        container.update_property(&[
            gtk4::accessible::Property::Label(&strings::text(strings::LIBRARY_DOCTOR)),
            gtk4::accessible::Property::KeyShortcuts("Enter Space"),
        ]);
        let revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::Crossfade)
            .transition_duration(crate::ui::motion::STANDARD_MS)
            .child(&container)
            .build();
        let on_cancel: Callback = Rc::new(RefCell::new(None));
        let on_activate: Callback = Rc::new(RefCell::new(None));
        {
            let callback = on_cancel.clone();
            cancel.connect_clicked(move |_| {
                if let Some(callback) = callback.borrow().clone() {
                    callback();
                }
            });
        }
        // input-parity: ACC-8 keyboard=doctor-card-enter-space
        let click = gtk4::GestureClick::new();
        {
            let callback = on_activate.clone();
            click.connect_released(move |gesture, _, _, _| {
                if gesture.current_button() == 1 {
                    if let Some(callback) = callback.borrow().clone() {
                        callback();
                    }
                }
            });
        }
        container.add_controller(click);
        let keys = gtk4::EventControllerKey::new();
        {
            let callback = on_activate.clone();
            keys.connect_key_pressed(move |_, key, _, _| {
                if matches!(key, gtk4::gdk::Key::Return | gtk4::gdk::Key::space) {
                    if let Some(callback) = callback.borrow().clone() {
                        callback();
                    }
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
        }
        container.add_controller(keys);
        Self {
            revealer,
            container,
            spinner,
            title,
            percent,
            progress,
            detail,
            cancel,
            on_cancel,
            on_activate,
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Revealer {
        &self.revealer
    }

    pub(super) fn set_on_cancel(&self, callback: impl Fn() + 'static) {
        *self.on_cancel.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn set_on_activate(&self, callback: impl Fn() + 'static) {
        *self.on_activate.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn show(&self, kind: DoctorJobKind, completed: usize, total: usize) {
        let presentation = progress_presentation(kind, completed, total);
        self.title.set_label(&presentation.title);
        self.detail.set_label(&presentation.detail);
        self.percent
            .set_label(&format!("{}%", presentation.percent));
        self.progress
            .set_fraction(f64::from(presentation.percent) / 100.0);
        self.container.update_property(&[
            gtk4::accessible::Property::Label(&presentation.title),
            gtk4::accessible::Property::Description(&presentation.detail),
        ]);
        self.spinner.set_spinning(true);
        self.cancel.set_visible(true);
        self.revealer.set_reveal_child(true);
    }

    pub(super) fn hide(&self) {
        self.spinner.set_spinning(false);
        self.revealer.set_reveal_child(false);
    }
}

#[cfg(test)]
mod tests {
    use super::{progress_presentation, DoctorJobKind};

    #[test]
    fn doc_5c_progress_uses_tracks_as_the_primary_currency() {
        let apply = progress_presentation(DoctorJobKind::Apply, 42, 128);
        assert_eq!(apply.title, "Updating tags…");
        assert_eq!(apply.detail, "42/128 tracks");
        assert_eq!(apply.percent, 33);

        let revert = progress_presentation(DoctorJobKind::Revert, 3, 4);
        assert_eq!(revert.title, "Reverting tags…");
        assert_eq!(revert.detail, "3/4 tracks");
        assert_eq!(revert.percent, 75);
    }
}
