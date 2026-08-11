//! Progress projection for generated playlist-target attachments.

use reprise_core::device_sync::PlannedSyncPhase;

use super::*;

pub(super) fn set_content_phase(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    phase: PlannedSyncPhase,
) {
    {
        let mut devices = runtime.device_states.borrow_mut();
        let Some(device) = devices
            .iter_mut()
            .find(|device| device.descriptor.id == work.device_id)
        else {
            return;
        };
        let current = device
            .machine
            .as_ref()
            .is_some_and(|machine| Rc::ptr_eq(machine, &work.machine));
        if !current {
            return;
        }
        device.sync_phase = phase;
    }
    runtime.notify();
}

#[allow(clippy::too_many_arguments)]
pub(super) fn syncing_phase(
    step: SyncStep,
    done: usize,
    total: usize,
    current_track: String,
    bytes_done: u64,
    bytes_total: u64,
) -> PlannedSyncPhase {
    PlannedSyncPhase::Syncing {
        step,
        done: u32::try_from(done).unwrap_or(u32::MAX),
        total: u32::try_from(total).unwrap_or(u32::MAX),
        current_track,
        bytes_done,
        bytes_total,
    }
}
