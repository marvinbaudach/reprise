use gtk4::prelude::Cast;
use reprise_core::db::Db;
use reprise_core::device_sync::settings::{
    list_remembered_devices, load_or_create_settings, rekey_legacy_device, save_settings,
    LegacyDeviceRekey,
};
use reprise_core::device_sync::{load_or_create_target, DeviceSettings, SyncTarget};
use reprise_platform_linux::device_sync::DeviceDescriptor;

pub(super) struct RememberedDeviceMemory {
    pub(super) descriptor: DeviceDescriptor,
    pub(super) settings: DeviceSettings,
    pub(super) target: SyncTarget,
    pub(super) last_verified_at: Option<i64>,
    pub(super) size_on_device_bytes: Option<u64>,
}

pub(super) fn load_remembered_device_memories(
    db: &Db,
) -> Result<Vec<RememberedDeviceMemory>, String> {
    list_remembered_devices(db)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|remembered| {
            let settings =
                load_or_create_settings(db, &remembered.stable_id, &remembered.local_name)
                    .map_err(|error| error.to_string())?;
            let target = load_or_create_target(db, &remembered.stable_id)
                .map_err(|error| error.to_string())?;
            Ok(RememberedDeviceMemory {
                descriptor: DeviceDescriptor {
                    id: remembered.stable_id.clone(),
                    persistent_id: Some(remembered.stable_id),
                    name: remembered.local_name,
                    root_uri: String::new(),
                    reconnectable: true,
                    icon: gtk4::gio::ThemedIcon::new("phone-symbolic").upcast(),
                },
                settings,
                target,
                last_verified_at: remembered.last_verified_at,
                size_on_device_bytes: remembered.size_on_device_bytes,
            })
        })
        .collect()
}

pub(super) fn load_device_memory(
    db: &Db,
    descriptor: &DeviceDescriptor,
) -> Result<(DeviceSettings, SyncTarget), String> {
    let Some(stable_id) = descriptor.persistent_id.as_deref() else {
        return Ok((
            DeviceSettings::transient(&descriptor.id, &descriptor.name),
            SyncTarget::default(),
        ));
    };
    if descriptor.root_uri.starts_with("mtp://") {
        match rekey_legacy_device(db, &descriptor.root_uri, stable_id) {
            Ok(LegacyDeviceRekey::StableKeyAlreadyExists) => {
                tracing::debug!(
                    stable_id,
                    "kept legacy MTP settings because the stable device key already exists"
                );
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    stable_id,
                    %error,
                    "could not re-key legacy MTP settings; the old row was kept"
                );
            }
        }
    }
    let mut settings = load_or_create_settings(db, stable_id, &descriptor.name)
        .map_err(|error| error.to_string())?;
    adopt_detected_device_name(db, &mut settings, descriptor)?;
    let target = load_or_create_target(db, stable_id).map_err(|error| error.to_string())?;
    Ok((settings, target))
}

/// Refreshes a name that was only ever seeded, never chosen.
///
/// Safe to write because `rename_remembered_device` refuses to *store* a
/// placeholder — it treats one like an empty field and falls back to the
/// detected name. A stored placeholder therefore always came from
/// `load_or_create_settings` seeding the row from a descriptor that had nothing
/// better at the time, and replacing it cannot discard anyone's choice.
pub(super) fn adopt_detected_device_name(
    db: &Db,
    settings: &mut DeviceSettings,
    descriptor: &DeviceDescriptor,
) -> Result<(), String> {
    if reprise_platform_linux::device_sync::is_placeholder_name(&settings.device_name)
        && !reprise_platform_linux::device_sync::is_placeholder_name(&descriptor.name)
    {
        settings.device_name = descriptor.name.clone();
        save_settings(db, settings).map_err(|error| error.to_string())?;
    }
    Ok(())
}
