use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VolumeProjection {
    pub(crate) name: String,
    pub(crate) root_uri: String,
    pub(crate) persistent_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MountProjection {
    pub(crate) name: String,
    pub(crate) root_uri: String,
    pub(crate) persistent_id: Option<String>,
    pub(crate) shadowed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectionSource {
    Volume(usize),
    Mount(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectedDevice {
    pub(crate) source: ProjectionSource,
    pub(crate) name: String,
    pub(crate) root_uri: String,
    pub(crate) persistent_id: Option<String>,
}

pub(crate) fn project_devices(
    volumes: &[VolumeProjection],
    mounts: &[MountProjection],
) -> Vec<ProjectedDevice> {
    let volume_roots = volumes
        .iter()
        .map(|volume| volume.root_uri.as_str())
        .collect::<HashSet<_>>();
    let mount_roots = mounts
        .iter()
        .map(|mount| mount.root_uri.as_str())
        .collect::<HashSet<_>>();

    let mut projected = volumes
        .iter()
        .enumerate()
        .filter(|(_, volume)| mount_roots.contains(volume.root_uri.as_str()))
        .map(|(index, volume)| ProjectedDevice {
            source: ProjectionSource::Volume(index),
            name: volume.name.clone(),
            root_uri: volume.root_uri.clone(),
            persistent_id: volume.persistent_id.clone(),
        })
        .chain(
            mounts
                .iter()
                .enumerate()
                .filter(|(_, mount)| {
                    !mount.shadowed && !volume_roots.contains(mount.root_uri.as_str())
                })
                .map(|(index, mount)| ProjectedDevice {
                    source: ProjectionSource::Mount(index),
                    name: mount.name.clone(),
                    root_uri: mount.root_uri.clone(),
                    persistent_id: mount.persistent_id.clone(),
                }),
        )
        .collect::<Vec<_>>();
    projected.sort_by(|left, right| left.name.cmp(&right.name));
    projected
}
