//! The Doctor page while a job runs. Progress, and nothing that pretends to be
//! a result.
//!
//! A running scan and a finished scan are two different pages. This one carries
//! the heading, the track counter, a bar, at most two forecast counters and a
//! single `Cancel`. No `Scan again`, no `Review`, no `Undo`, no
//! `checked · skipped`, no "results are kept" — those all describe a scan that
//! has ended, and saying them at 1 % is how the old screen managed to be
//! in-progress and final at the same time.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::progress_card::DoctorJobKind;
use super::summary_model::LiveCounters;
use crate::ui::strings;

type Callback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

pub(super) struct DoctorRunningPanel {
    root: gtk4::Box,
    heading: gtk4::Label,
    tracks: gtk4::Label,
    progress: gtk4::ProgressBar,
    will_fix: gtk4::Label,
    waiting: gtk4::Label,
    cancel: gtk4::Button,
    on_cancel: Callback,
}

impl DoctorRunningPanel {
    pub(super) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
        root.set_valign(gtk4::Align::Start);

        let heading = gtk4::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["title-2"])
            .build();
        root.append(&heading);

        let tracks = gtk4::Label::builder()
            .xalign(0.0)
            .css_classes(["dim-label"])
            .build();
        root.append(&tracks);

        let progress = gtk4::ProgressBar::builder().hexpand(true).build();
        progress.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::DOCTOR_PROGRESS,
        ))]);
        root.append(&progress);

        let will_fix = counter_label();
        let waiting = counter_label();
        root.append(&will_fix);
        root.append(&waiting);

        let cancel = gtk4::Button::builder()
            .label(strings::text(strings::CANCEL))
            .halign(gtk4::Align::Start)
            .build();
        root.append(&cancel);

        let on_cancel: Callback = Rc::new(RefCell::new(None));
        {
            let on_cancel = on_cancel.clone();
            cancel.connect_clicked(move |_| {
                if let Some(callback) = on_cancel.borrow().clone() {
                    callback();
                }
            });
        }

        Self {
            root,
            heading,
            tracks,
            progress,
            will_fix,
            waiting,
            cancel,
            on_cancel,
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(super) fn connect_cancel(&self, callback: impl Fn() + 'static) {
        *self.on_cancel.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn render(
        &self,
        kind: DoctorJobKind,
        completed: usize,
        total: usize,
        counters: LiveCounters,
    ) {
        self.heading.set_label(&strings::text(match kind {
            DoctorJobKind::Scan => strings::DOCTOR_SCANNING,
            DoctorJobKind::Apply => strings::DOCTOR_UPDATING_TAGS,
            DoctorJobKind::Revert => strings::DOCTOR_REVERTING_TAGS,
        }));
        self.tracks
            .set_label(&strings::doctor_track_progress(completed, total));
        self.progress.set_fraction(fraction(completed, total));
        // The forecast only means something while the scan is still finding
        // things. Once the quiet write is running there is nothing left to
        // forecast, and while a revert runs the numbers would be backwards.
        let counters = (kind == DoctorJobKind::Scan).then_some(counters);
        set_counter(
            &self.will_fix,
            counters.map(|counters| counters.will_fix_quietly),
            strings::doctor_will_fix_quietly,
        );
        set_counter(
            &self.waiting,
            counters.map(|counters| counters.waiting_for_you),
            strings::doctor_waiting_for_you,
        );
    }

    /// Cancelling a revert mid-write is a different promise from cancelling a
    /// scan, so the button only offers itself where the coordinator can honour
    /// it — which is every job it started with a cancellation flag, i.e. all
    /// three. Kept as a hook so a future job kind cannot silently inherit it.
    pub(super) fn set_cancellable(&self, cancellable: bool) {
        self.cancel.set_visible(cancellable);
    }
}

fn counter_label() -> gtk4::Label {
    gtk4::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .visible(false)
        .build()
}

/// A counter that would read `0` is not a counter, it is noise. Hide it.
fn set_counter(label: &gtk4::Label, count: Option<usize>, render: fn(usize) -> String) {
    match count.filter(|count| *count > 0) {
        Some(count) => {
            label.set_label(&render(count));
            label.set_visible(true);
        }
        None => {
            label.set_label("");
            label.set_visible(false);
        }
    }
}

fn fraction(completed: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (completed as f64 / total as f64).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_2c_progress_fraction_survives_an_unknown_total() {
        assert!((fraction(0, 0) - 0.0).abs() < f64::EPSILON);
        assert!((fraction(742, 1648) - 0.450_242_718_446_601_9).abs() < 1e-9);
        assert!((fraction(9, 4) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn doc_2c_the_running_page_offers_cancel_and_nothing_else() {
        if gtk4::init().is_err() {
            return;
        }
        let panel = DoctorRunningPanel::new();
        panel.render(
            DoctorJobKind::Scan,
            742,
            1648,
            LiveCounters {
                will_fix_quietly: 511,
                waiting_for_you: 39,
            },
        );
        let mut buttons = Vec::new();
        let mut child = panel.widget().first_child();
        while let Some(widget) = child {
            if let Some(button) = widget.downcast_ref::<gtk4::Button>() {
                buttons.push(button.label().map(|label| label.to_string()));
            }
            child = widget.next_sibling();
        }
        assert_eq!(buttons.len(), 1, "cancel is the only button: {buttons:?}");
        assert_eq!(buttons[0].as_deref(), Some("Cancel"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn doc_2c_a_zero_counter_is_not_rendered() {
        if gtk4::init().is_err() {
            return;
        }
        let panel = DoctorRunningPanel::new();
        panel.render(
            DoctorJobKind::Scan,
            3,
            10,
            LiveCounters {
                will_fix_quietly: 0,
                waiting_for_you: 4,
            },
        );
        assert!(!panel.will_fix.is_visible());
        assert!(panel.waiting.is_visible());
        assert_eq!(panel.waiting.label(), "4 waiting for you");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn doc_2c_the_quiet_write_forecasts_nothing() {
        if gtk4::init().is_err() {
            return;
        }
        let panel = DoctorRunningPanel::new();
        panel.render(
            DoctorJobKind::Apply,
            1,
            2,
            LiveCounters {
                will_fix_quietly: 511,
                waiting_for_you: 39,
            },
        );
        assert_eq!(panel.heading.label(), "Updating tags…");
        assert!(!panel.will_fix.is_visible());
        assert!(!panel.waiting.is_visible());
    }
}
