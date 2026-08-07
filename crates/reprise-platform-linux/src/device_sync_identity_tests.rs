use std::fs;

use super::*;

#[test]
fn descriptor_projection_accepts_only_mtp_roots() {
    assert!(project_descriptor("file:///tmp", Some("uuid"), None, "Disk").is_none());
    assert!(project_descriptor("gphoto2://phone", Some("uuid"), None, "Camera").is_none());
    assert!(project_descriptor("mtp://phone", Some("uuid"), None, "Phone").is_some());
}

#[test]
fn descriptor_prefers_uuid_for_stable_reconnects() {
    let descriptor = project_descriptor(
        "mtp://phone",
        Some("mount-uuid"),
        Some("usb-serial"),
        "Pixel",
    )
    .unwrap();
    assert_eq!(descriptor.id, "mount-uuid");
    assert_eq!(descriptor.persistent_id.as_deref(), Some("mount-uuid"));
    assert!(descriptor.reconnectable);
}

#[test]
fn descriptor_uses_the_usb_serial_when_gvfs_has_no_uuid() {
    let descriptor = project_descriptor("mtp://phone", None, Some("usb-serial"), "Pixel").unwrap();
    assert_eq!(descriptor.id, "usb-serial");
    assert_eq!(descriptor.persistent_id.as_deref(), Some("usb-serial"));
    assert!(descriptor.reconnectable);
}

#[test]
fn descriptor_uses_the_uri_only_for_the_live_unrememberable_session() {
    let descriptor = project_descriptor("mtp://phone", None, None, "Pixel").unwrap();
    assert_eq!(descriptor.id, "mtp://phone");
    assert_eq!(descriptor.persistent_id, None);
    assert!(!descriptor.reconnectable);
}

#[test]
fn descriptor_gaining_a_usb_serial_becomes_reconnectable_without_losing_mtp_access() {
    let root_uri = "mtp://Google_Pixel_10_Pro_XL_59100DLCQ006SB/";
    let first = project_descriptor(root_uri, None, None, "Pixel 10 Pro XL").unwrap();
    let identified =
        project_descriptor(root_uri, None, Some("59100DLCQ006SB"), "Pixel 10 Pro XL").unwrap();

    assert_eq!(first.root_uri, identified.root_uri);
    assert!(!first.reconnectable);
    assert_eq!(identified.id, "59100DLCQ006SB");
    assert_eq!(identified.persistent_id.as_deref(), Some("59100DLCQ006SB"));
    assert!(identified.reconnectable);
}

#[test]
fn fallback_mount_prefers_its_owning_volume_name() {
    assert_eq!(
        mount_display_name("mtp", Some("Pixel 10 Pro XL")),
        "Pixel 10 Pro XL"
    );
    assert_eq!(mount_display_name("mtp", None), "mtp");
}

#[test]
fn mtp_49_volume_unix_device_identifier_resolves_the_usb_serial() {
    let sysfs = tempfile::tempdir().unwrap();
    let device = sysfs.path().join("3-1.4");
    fs::create_dir(&device).unwrap();
    fs::write(device.join("busnum"), "3\n").unwrap();
    fs::write(device.join("devnum"), "32\n").unwrap();
    fs::write(device.join("serial"), "59100DLCQ006SB\n").unwrap();

    let root_uri = "mtp://Google_Pixel_10_Pro_XL_59100DLCQ006SB/";
    assert_eq!(
        usb_serial_from_volume_identifier(Some("/dev/bus/usb/003/032"), root_uri, sysfs.path(),)
            .as_deref(),
        Some("59100DLCQ006SB")
    );
    assert_eq!(
        usb_serial_from_volume_identifier(None, "mtp://[usb:3,32]/", sysfs.path()).as_deref(),
        Some("59100DLCQ006SB"),
        "the legacy URI address remains a fallback"
    );
    assert_eq!(
        usb_serial_from_volume_identifier(
            Some("/dev/bus/usb/003/099"),
            "mtp://[usb:3,32]/",
            sysfs.path(),
        )
        .as_deref(),
        Some("59100DLCQ006SB"),
        "a stale volume address must not block the legacy fallback"
    );
    assert_eq!(
        usb_serial_from_volume_identifier(None, root_uri, sysfs.path()),
        None
    );
    assert_eq!(
        usb_serial_from_volume_identifier(
            Some("/dev/bus/usb/not-a-device"),
            root_uri,
            sysfs.path()
        ),
        None
    );
}

#[test]
fn mtp_49_sysfs_serial_resolution_prefers_id_serial_short_and_degrades_cleanly() {
    let sysfs = tempfile::tempdir().unwrap();
    let device = sysfs.path().join("1-13");
    fs::create_dir(&device).unwrap();
    fs::write(device.join("busnum"), "1\n").unwrap();
    fs::write(device.join("devnum"), "13\n").unwrap();
    fs::write(
        device.join("uevent"),
        "PRODUCT=18d1/4ee1/440\nID_SERIAL_SHORT=udev-serial\n",
    )
    .unwrap();
    fs::write(device.join("serial"), "sysfs-serial\n").unwrap();

    assert_eq!(
        usb_serial_from_sysfs("mtp://[usb:001,013]/", sysfs.path()).as_deref(),
        Some("udev-serial")
    );
    fs::write(device.join("uevent"), "PRODUCT=18d1/4ee1/440\n").unwrap();
    assert_eq!(
        usb_serial_from_sysfs("mtp://[usb:001,013]/", sysfs.path()).as_deref(),
        Some("sysfs-serial")
    );
    assert_eq!(
        usb_serial_from_sysfs("mtp://[usb:001,099]/", sysfs.path()),
        None
    );
    assert_eq!(usb_serial_from_sysfs("mtp://phone", sysfs.path()), None);
}
