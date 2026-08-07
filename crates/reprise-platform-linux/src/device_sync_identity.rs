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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsbFacts {
    pub serial: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

pub fn project_descriptor(
    root_uri: &str,
    uuid: Option<&str>,
    facts: &UsbFacts,
    mount_name: &str,
) -> Option<DeviceDescriptor> {
    if !root_uri.starts_with("mtp://") {
        return None;
    }
    let persistent_id = stable_device_identity(uuid, facts.serial.as_deref());
    Some(DeviceDescriptor {
        id: persistent_id
            .clone()
            .unwrap_or_else(|| root_uri.to_string()),
        persistent_id: persistent_id.clone(),
        name: detected_device_name(mount_name, facts),
        root_uri: root_uri.to_string(),
        reconnectable: persistent_id.is_some(),
        icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
    })
}

pub fn descriptor_from_mount(mount: &gio::Mount) -> Option<DeviceDescriptor> {
    let root_uri = mount.root().uri();
    let uuid = mount.uuid();
    let volume = mount.volume();
    let unix_device = volume
        .as_ref()
        .and_then(|volume| volume.identifier(gio::VOLUME_IDENTIFIER_KIND_UNIX_DEVICE));
    let facts = usb_facts_from_volume_identifier(
        unix_device.as_deref(),
        &root_uri,
        Path::new("/sys/bus/usb/devices"),
    );
    let mount_name = mount.name();
    let volume_name = volume.as_ref().map(gio::prelude::VolumeExt::name);
    let mut descriptor = project_descriptor(
        &root_uri,
        uuid.as_deref(),
        &facts,
        &mount_display_name(&mount_name, volume_name.as_deref()),
    )?;
    descriptor.icon = mount.icon();
    Some(descriptor)
}

pub(crate) fn mount_display_name(mount_name: &str, volume_name: Option<&str>) -> String {
    volume_name.unwrap_or(mount_name).to_string()
}

pub fn is_placeholder_name(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    name.is_empty()
        || matches!(
            name.as_str(),
            "mtp" | "mtp device" | "unknown" | "unknown device"
        )
        || name.starts_with("mtp:")
}

fn detected_device_name(mount_name: &str, facts: &UsbFacts) -> String {
    // No `root_uri` check here: `project_descriptor` has already established
    // that every URI reaching this point begins with `mtp://`, so a mount name
    // equal to it is caught by `is_placeholder_name`'s `mtp:` rule. The extra
    // comparison this replaced could never change the outcome.
    if !is_placeholder_name(mount_name) {
        return mount_name.trim().to_string();
    }
    if let Some(product) = facts
        .product
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let Some(manufacturer) = facts
            .manufacturer
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return product.to_string();
        };
        // Word-boundary, not substring: "ASUS" occurs inside "Pegasus", and a
        // raw `contains` would drop a legitimate manufacturer prefix on that
        // coincidence.
        if mentions_manufacturer(product, manufacturer) {
            return product.to_string();
        }
        return format!("{manufacturer} {product}");
    }
    facts
        .manufacturer
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(
            || "MTP device".to_string(),
            |value| format!("{value} device"),
        )
}

/// Whether `product` already names `manufacturer` as a word of its own, so
/// "Samsung Galaxy S24" is not prefixed into "Samsung Samsung Galaxy S24"
/// while "Pegasus 5" still earns its "ASUS" prefix.
fn mentions_manufacturer(product: &str, manufacturer: &str) -> bool {
    let manufacturer = manufacturer.to_ascii_lowercase();
    product
        .to_ascii_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word == manufacturer)
}

/// Finds the USB device represented by a GVfs MTP URI and returns its serial
/// without making device availability depend on sysfs. Missing, malformed,
/// unreadable, or transient entries simply mean "unrememberable".
pub fn usb_serial_from_sysfs(root_uri: &str, sysfs_root: &Path) -> Option<String> {
    let (bus, device) = mtp_usb_address(root_uri)?;
    usb_facts_for_address(bus, device, sysfs_root)?.serial
}

/// Resolves the USB serial from the stable volume identifier when GVfs
/// publishes one, retaining the legacy MTP URI address as a fallback.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn usb_serial_from_volume_identifier(
    unix_device: Option<&str>,
    root_uri: &str,
    sysfs_root: &Path,
) -> Option<String> {
    usb_facts_from_volume_identifier(unix_device, root_uri, sysfs_root).serial
}

pub(crate) fn usb_facts_from_volume_identifier(
    unix_device: Option<&str>,
    root_uri: &str,
    sysfs_root: &Path,
) -> UsbFacts {
    let volume_facts = unix_device
        .and_then(unix_device_usb_address)
        .and_then(|(bus, device)| usb_facts_for_address(bus, device, sysfs_root));
    let uri_facts = mtp_usb_address(root_uri)
        .and_then(|(bus, device)| usb_facts_for_address(bus, device, sysfs_root));
    merge_facts(volume_facts, uri_facts)
}

fn merge_facts(primary: Option<UsbFacts>, fallback: Option<UsbFacts>) -> UsbFacts {
    let primary = primary.unwrap_or_default();
    let fallback = fallback.unwrap_or_default();
    UsbFacts {
        serial: primary.serial.or(fallback.serial),
        manufacturer: primary.manufacturer.or(fallback.manufacturer),
        product: primary.product.or(fallback.product),
    }
}

pub fn usb_facts_for_address(bus: u32, device: u32, sysfs_root: &Path) -> Option<UsbFacts> {
    let entries = fs::read_dir(sysfs_root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if read_decimal(&path.join("busnum")) != Some(bus)
            || read_decimal(&path.join("devnum")) != Some(device)
        {
            continue;
        }
        let uevent = fs::read_to_string(path.join("uevent")).ok();
        return Some(UsbFacts {
            serial: uevent_value(uevent.as_deref(), "ID_SERIAL_SHORT")
                .or_else(|| read_nonempty(&path.join("serial"))),
            product: uevent_value(uevent.as_deref(), "ID_MODEL")
                .map(|value| normalize_uevent_name(&value))
                .or_else(|| read_nonempty(&path.join("product"))),
            manufacturer: uevent_value(uevent.as_deref(), "ID_VENDOR")
                .map(|value| normalize_uevent_name(&value))
                .or_else(|| read_nonempty(&path.join("manufacturer"))),
        });
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

fn uevent_value(contents: Option<&str>, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    contents?
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn normalize_uevent_name(value: &str) -> String {
    value.replace('_', " ")
}
