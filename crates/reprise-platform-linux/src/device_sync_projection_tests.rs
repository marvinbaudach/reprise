use super::projection::{
    project_devices, MountProjection, ProjectedDevice, ProjectionSource, VolumeProjection,
};

const ROOT: &str = "mtp://Google_Pixel_10_Pro_XL_59100DLCQ006SB/";
const SERIAL: &str = "59100DLCQ006SB";

fn volume() -> VolumeProjection {
    VolumeProjection {
        name: "Pixel 10 Pro XL".to_string(),
        root_uri: ROOT.to_string(),
        persistent_id: Some(SERIAL.to_string()),
    }
}

fn mount(name: &str, root_uri: &str, shadowed: bool) -> MountProjection {
    MountProjection {
        name: name.to_string(),
        root_uri: root_uri.to_string(),
        persistent_id: None,
        shadowed,
    }
}

#[test]
fn mtp_53_startup_projection_prefers_the_known_volume_at_t0() {
    let projected = project_devices(&[volume()], &[mount("mtp", ROOT, false)]);

    assert_eq!(
        projected,
        vec![ProjectedDevice {
            source: ProjectionSource::Volume(0),
            name: "Pixel 10 Pro XL".to_string(),
            root_uri: ROOT.to_string(),
            persistent_id: Some(SERIAL.to_string()),
        }]
    );
}

#[test]
fn mtp_53_projection_is_identical_after_volume_linkage() {
    let at_t0 = project_devices(&[volume()], &[mount("mtp", ROOT, false)]);
    let at_t1 = project_devices(
        &[volume()],
        &[
            mount("Pixel 10 Pro XL", ROOT, false),
            mount("mtp", ROOT, true),
        ],
    );

    assert_eq!(at_t1, at_t0);
    assert_eq!(at_t1.len(), 1);
    assert_eq!(at_t1[0].source, ProjectionSource::Volume(0));
    assert_eq!(at_t1[0].persistent_id.as_deref(), Some(SERIAL));
}

#[test]
fn mtp_53_unowned_unshadowed_mount_remains_a_fallback() {
    let root_uri = "mtp://exotic-backend/";
    let projected = project_devices(&[], &[mount("Portable player", root_uri, false)]);

    assert_eq!(
        projected,
        vec![ProjectedDevice {
            source: ProjectionSource::Mount(0),
            name: "Portable player".to_string(),
            root_uri: root_uri.to_string(),
            persistent_id: None,
        }]
    );
}

#[test]
fn mtp_53_volume_without_a_listed_mount_stays_hidden() {
    assert!(project_devices(&[volume()], &[]).is_empty());
}
