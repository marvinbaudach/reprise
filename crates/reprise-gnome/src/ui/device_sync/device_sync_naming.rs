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

        let local_name = local_name.trim();
        let name = if local_name.is_empty() {
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
