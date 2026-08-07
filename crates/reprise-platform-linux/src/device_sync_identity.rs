use std::fs;
use std::path::Path;

use gio::prelude::*;
use reprise_core::device_sync::stable_device_identity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceDescriptor {
    /// Session identifier. Stable when `persistent_id` exists; otherwise the
    /// volatile URI is used only to address this one live connection.
    pub id: String,
    /// Durable settings key, never an `mtp://` transport URI.
    pub persistent_id: Option<String>,
    pub name: String,
    pub root_uri: String,
    pub reconnectable: bool,
    pub icon: gio::Icon,
}

pub fn project_descriptor(
    root_uri: &str,
    uuid: Option<&str>,
    usb_serial: Option<&str>,
    name: &str,
) -> Option<DeviceDescriptor> {
    if !root_uri.starts_with("mtp://") {
        return None;
    }
    let persistent_id = stable_device_identity(uuid, usb_serial);
    Some(DeviceDescriptor {
        id: persistent_id
            .clone()
            .unwrap_or_else(|| root_uri.to_string()),
        persistent_id: persistent_id.clone(),
        name: name.to_string(),
        root_uri: root_uri.to_string(),
        reconnectable: persistent_id.is_some(),
        icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
    })
}

pub fn descriptor_from_mount(mount: &gio::Mount) -> Option<DeviceDescriptor> {
    let root_uri = mount.root().uri();
    let uuid = mount.uuid();
    let unix_device = mount
        .volume()
        .and_then(|volume| volume.identifier(gio::VOLUME_IDENTIFIER_KIND_UNIX_DEVICE));
    let usb_serial = usb_serial_from_volume_identifier(
        unix_device.as_deref(),
        &root_uri,
        Path::new("/sys/bus/usb/devices"),
    );
    let mut descriptor = project_descriptor(
        &root_uri,
        uuid.as_deref(),
        usb_serial.as_deref(),
        &mount.name(),
    )?;
    descriptor.icon = mount.icon();
    Some(descriptor)
}

/// Finds the USB device represented by a GVfs MTP URI and reads its stable
/// serial without making device availability depend on sysfs. Missing,
/// malformed, unreadable, or transient entries simply mean "unrememberable".
pub fn usb_serial_from_sysfs(root_uri: &str, sysfs_root: &Path) -> Option<String> {
    let (bus, device) = mtp_usb_address(root_uri)?;
    usb_serial_for_address(bus, device, sysfs_root)
}

/// Resolves the USB serial from the stable volume identifier when GVfs
/// publishes one, retaining the legacy MTP URI address as a fallback.
pub(crate) fn usb_serial_from_volume_identifier(
    unix_device: Option<&str>,
    root_uri: &str,
    sysfs_root: &Path,
) -> Option<String> {
    if let Some((bus, device)) = unix_device.and_then(unix_device_usb_address) {
        if let Some(serial) = usb_serial_for_address(bus, device, sysfs_root) {
            return Some(serial);
        }
    }
    usb_serial_from_sysfs(root_uri, sysfs_root)
}

fn usb_serial_for_address(bus: u32, device: u32, sysfs_root: &Path) -> Option<String> {
    let entries = fs::read_dir(sysfs_root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if read_decimal(&path.join("busnum")) != Some(bus)
            || read_decimal(&path.join("devnum")) != Some(device)
        {
            continue;
        }
        if let Some(serial) = read_uevent_serial(&path.join("uevent")) {
            return Some(serial);
        }
        if let Some(serial) = read_nonempty(&path.join("serial")) {
            return Some(serial);
        }
    }
    None
}

fn unix_device_usb_address(identifier: &str) -> Option<(u32, u32)> {
    let relative = Path::new(identifier).strip_prefix("/dev/bus/usb").ok()?;
    let mut components = relative.iter();
    let bus = components.next()?.to_str()?.parse().ok()?;
    let device = components.next()?.to_str()?.parse().ok()?;
    components.next().is_none().then_some((bus, device))
}

fn mtp_usb_address(root_uri: &str) -> Option<(u32, u32)> {
    let address = root_uri.strip_prefix("mtp://[usb:")?.strip_suffix("]/")?;
    let (bus, device) = address.split_once(',')?;
    Some((bus.parse().ok()?, device.parse().ok()?))
}

fn read_decimal(path: &Path) -> Option<u32> {
    read_nonempty(path)?.parse().ok()
}

fn read_nonempty(path: &Path) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn read_uevent_serial(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("ID_SERIAL_SHORT="))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
