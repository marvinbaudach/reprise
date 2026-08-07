//! Local device naming, kept separate from the runtime's transfer machinery.

use std::rc::Rc;

use reprise_core::device_sync::settings::rename_device;

use super::DeviceSyncRuntime;

impl DeviceSyncRuntime {
    pub fn rename_remembered_device(
        self: &Rc<Self>,
        device_id: &str,
        local_name: &str,
    ) -> Result<(), String> {
        let rememberable = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .is_some_and(|device| device.descriptor.persistent_id.is_some());
        if !rememberable {
            return Err("this device has no stable identity to rename".into());
        }

        // A placeholder counts as "no name of my own", exactly like an empty
        // field. This is what makes `adopt_detected_device_name`'s refresh safe:
        // because a placeholder can never be stored deliberately, a stored one
        // can only have come from seeding the row at creation, so adopting a
        // better detected name over it cannot discard a user's choice.
        let local_name = local_name.trim();
        let keeps_detected_name = local_name.is_empty()
            || reprise_platform_linux::device_sync::is_placeholder_name(local_name);
        let name = if keeps_detected_name {
            self.device_states
                .borrow()
                .iter()
                .find(|device| device.descriptor.id == device_id)
                .map(|device| device.descriptor.name.clone())
                .ok_or_else(|| "device is not connected".to_string())?
        } else {
            local_name.to_string()
        };
        rename_device(&self.conn, device_id, &name).map_err(|error| error.to_string())?;
        if let Some(device) = self
            .device_states
            .borrow_mut()
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            device.settings.device_name = name;
        }
        self.notify();
        Ok(())
    }
}
