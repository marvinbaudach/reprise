use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library_doctor::DoctorScanPhase;

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

fn scan_progress_presentation(
    phase: DoctorScanPhase,
    completed: usize,
    total: usize,
) -> ProgressPresentation {
    let mut presentation = progress_presentation(DoctorJobKind::Scan, completed, total);
    presentation.title = strings::text(match phase {
        DoctorScanPhase::ReadingTags => strings::DOCTOR_PHASE_LOCAL,
        DoctorScanPhase::CheckingRemote => strings::DOCTOR_PHASE_REMOTE,
        DoctorScanPhase::Fingerprinting => strings::DOCTOR_PHASE_FINGERPRINT,
    });
    presentation
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
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["scan-card-percent"])
            .build();
        let cancel_label = strings::text(strings::CANCEL);
        let cancel = gtk4::Button::builder()
            .icon_name(crate::ui::scan_card_css::SIDEBAR_CANCEL_ICON)
            .tooltip_text(&cancel_label)
            .css_classes(["flat", "scan-card-cancel"])
            .build();
        cancel.update_property(&[gtk4::accessible::Property::Label(&cancel_label)]);
        let title_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 7);
        title_row.append(&spinner);
        title_row.append(&title);
        let status_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 7);
        status_row.set_margin_start(19);
        status_row.append(&percent);
        status_row.append(&cancel);
        let header = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        header.append(&title_row);
        header.append(&status_row);
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
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        container.set_height_request(crate::ui::scan_card_css::JOB_CARD_HEIGHT_PX);
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
            let cancel = cancel.clone();
            click.connect_released(move |gesture, _, x, y| {
                if gesture.current_button() == 1 {
                    let Some(container) = gesture.widget() else {
                        return;
                    };
                    let cancel_bounds = cancel.compute_bounds(&container);
                    if !card_body_activates(cancel_bounds.as_ref(), x, y) {
                        return;
                    }
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
            let cancel = cancel.clone();
            keys.connect_key_pressed(move |_, key, _, _| {
                if cancel.has_focus() {
                    return glib::Propagation::Proceed;
                }
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
        self.show_presentation(&presentation);
    }

    pub(super) fn show_scan(&self, phase: DoctorScanPhase, completed: usize, total: usize) {
        self.show_presentation(&scan_progress_presentation(phase, completed, total));
    }

    fn show_presentation(&self, presentation: &ProgressPresentation) {
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

fn card_body_activates(cancel_bounds: Option<&gtk4::graphene::Rect>, x: f64, y: f64) -> bool {
    cancel_bounds.is_none_or(|bounds| {
        x < f64::from(bounds.x())
            || x > f64::from(bounds.x() + bounds.width())
            || y < f64::from(bounds.y())
            || y > f64::from(bounds.y() + bounds.height())
    })
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    use super::{
        card_body_activates, progress_presentation, scan_progress_presentation, DoctorJobKind,
        DoctorProgressCard,
    };
    use reprise_core::library_doctor::DoctorScanPhase;

    /// NPP-1: the sidebar is 240px, and the card is a passenger in it.
    const SIDEBAR_WIDTH: i32 = 240;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn doc_5c_the_card_label_stays_whole_at_sidebar_width() {
        if gtk4::init().is_err() {
            return;
        }
        let card = DoctorProgressCard::new();
        card.show(DoctorJobKind::Scan, 742, 1648);
        card.revealer.set_reveal_child(true);
        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        column.set_size_request(SIDEBAR_WIDTH, -1);
        column.append(card.widget());
        let window = gtk4::Window::builder()
            .default_width(SIDEBAR_WIDTH)
            .default_height(600)
            .child(&column)
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert_eq!(card.title.label(), "Checking tracks…");
        assert_eq!(
            card.cancel.icon_name().as_deref(),
            Some(crate::ui::scan_card_css::SIDEBAR_CANCEL_ICON)
        );
        assert_eq!(card.cancel.tooltip_text().as_deref(), Some("Cancel"));
        assert!(
            !card.title.layout().is_ellipsized(),
            "the job label must be readable, not truncated to a few characters"
        );
        // If anything has to give, it is the detail line — that is what its own
        // ellipsis is for.
        assert_eq!(
            card.detail.ellipsize(),
            gtk4::pango::EllipsizeMode::End,
            "the detail line absorbs the shortfall"
        );
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn npp_1_doctor_job_card_minimum_width_fits_the_sidebar() {
        gtk4::init().unwrap();
        crate::ui::style::install();
        let card = DoctorProgressCard::new();
        card.show_scan(DoctorScanPhase::CheckingRemote, 2_177, 2_177);
        card.detail
            .set_label("2,177 of 2,177 checked · 0 cached · 0 unavailable");

        let minimum = card.widget().measure(gtk4::Orientation::Horizontal, -1).0;
        assert!(
            minimum <= 232,
            "the visible Library Doctor job card must fit inside the 240px sidebar's 232px card slot, got {minimum}px"
        );
        assert_eq!(card.title.ellipsize(), gtk4::pango::EllipsizeMode::End);
        assert_eq!(card.detail.ellipsize(), gtk4::pango::EllipsizeMode::End);
        assert_eq!(
            card.widget().measure(gtk4::Orientation::Vertical, 232).0,
            crate::ui::scan_card_css::JOB_CARD_HEIGHT_PX
        );
    }

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

    #[test]
    fn doc_2c_the_sidebar_card_names_every_scan_phase() {
        assert_eq!(
            scan_progress_presentation(DoctorScanPhase::ReadingTags, 1, 2).title,
            "Reading tags…"
        );
        assert_eq!(
            scan_progress_presentation(DoctorScanPhase::CheckingRemote, 1, 2).title,
            "Checking against MusicBrainz…"
        );
        assert_eq!(
            scan_progress_presentation(DoctorScanPhase::Fingerprinting, 1, 2).title,
            "Fingerprinting audio…"
        );
    }

    #[test]
    fn doc_5e_the_card_body_activates_and_cancel_does_not() {
        let cancel = gtk4::graphene::Rect::new(180.0, 4.0, 56.0, 24.0);

        assert!(card_body_activates(Some(&cancel), 40.0, 16.0));
        assert!(!card_body_activates(Some(&cancel), 200.0, 16.0));
    }
}
