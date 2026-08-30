use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use reprise_core::library_doctor::{LibraryDoctor, TagWriteSlotStatus};

use super::LibraryDoctorCoordinator;

const SLOT_POLL_INTERVAL: Duration = Duration::from_secs(1);

fn slot_poll_required(slot_busy: bool, doctor_visible: bool) -> bool {
    slot_busy || doctor_visible
}

impl LibraryDoctorCoordinator {
    pub(super) fn observe_tag_write_start(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(50), move || {
            if let Some(coordinator) = weak.upgrade() {
                coordinator.refresh_tag_write_slot();
            }
        });
    }

    pub(super) fn refresh_tag_write_slot(self: &Rc<Self>) {
        let Some(db_dir) = self.db_path.parent() else {
            tracing::warn!("cannot inspect the tag-write slot: database has no parent directory");
            return;
        };
        let status = match LibraryDoctor::new(&self.conn).tag_write_slot_status(db_dir) {
            Ok(status) => status,
            Err(error) => {
                tracing::warn!(%error, "could not inspect the tag-write slot");
                self.ensure_tag_write_slot_poll();
                return;
            }
        };
        match status {
            TagWriteSlotStatus::Free | TagWriteSlotStatus::Orphaned(_) => {
                self.slot_busy.set(false);
                if let Some(review) = self.review.borrow().as_ref() {
                    review.set_write_slot_busy(false);
                }
                if !self.running.get() {
                    self.progress.hide();
                }
            }
            TagWriteSlotStatus::Busy(owner) => {
                self.slot_busy.set(true);
                if let Some(review) = self.review.borrow().as_ref() {
                    review.set_write_slot_busy(true);
                }
                let own_write = self.running.get()
                    && !matches!(self.job_kind.get(), Some(super::DoctorJobKind::Scan));
                self.progress.show_slot(&owner, own_write);
            }
        }
        self.ensure_tag_write_slot_poll();
    }

    fn ensure_tag_write_slot_poll(self: &Rc<Self>) {
        if self.slot_poll.borrow().is_some()
            || !slot_poll_required(self.slot_busy.get(), self.navigation.is_visible())
        {
            return;
        }
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local(SLOT_POLL_INTERVAL, move || {
            let Some(coordinator) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            coordinator.refresh_tag_write_slot();
            if slot_poll_required(
                coordinator.slot_busy.get(),
                coordinator.navigation.is_visible(),
            ) {
                glib::ControlFlow::Continue
            } else {
                coordinator.slot_poll.borrow_mut().take();
                glib::ControlFlow::Break
            }
        });
        self.slot_poll.borrow_mut().replace(source);
    }
}

#[cfg(test)]
mod tests {
    use super::slot_poll_required;

    #[test]
    fn slot_poll_never_runs_permanently_while_idle_and_closed() {
        assert!(!slot_poll_required(false, false));
        assert!(slot_poll_required(true, false));
        assert!(slot_poll_required(false, true));
        assert!(slot_poll_required(true, true));
    }
}
