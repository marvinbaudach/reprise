use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::path::{Component, Path};
use std::rc::Rc;

use gio::prelude::*;
use reprise_core::device_sync::safe_component;
use reprise_core::library::m3u::{parse_m3u, M3uEntry};

const ENUMERATE_ATTRIBUTES: &str = "standard::name,standard::type,standard::size";
const ENUMERATE_BATCH_SIZE: i32 = 64;
const MANAGED_ROOT: [&str; 2] = ["Music", "Reprise"];
const PARTIAL_SUFFIX: &str = ".reprise-part";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceDescriptor {
    pub id: String,
    pub name: String,
    pub root_uri: String,
    pub reconnectable: bool,
    pub icon: gio::Icon,
}

pub fn project_descriptor(
    root_uri: &str,
    uuid: Option<&str>,
    name: &str,
) -> Option<DeviceDescriptor> {
    if !root_uri.starts_with("mtp://") {
        return None;
    }
    let stable_uuid = uuid.filter(|value| !value.trim().is_empty());
    Some(DeviceDescriptor {
        id: stable_uuid.unwrap_or(root_uri).to_string(),
        name: name.to_string(),
        root_uri: root_uri.to_string(),
        reconnectable: stable_uuid.is_some(),
        icon: gio::ThemedIcon::new("phone-symbolic").upcast(),
    })
}

pub fn descriptor_from_mount(mount: &gio::Mount) -> Option<DeviceDescriptor> {
    let root_uri = mount.root().uri();
    let uuid = mount.uuid();
    let mut descriptor = project_descriptor(&root_uri, uuid.as_deref(), &mount.name())?;
    descriptor.icon = mount.icon();
    Some(descriptor)
}

type DeviceCallback = Rc<dyn Fn(Vec<DeviceDescriptor>)>;

#[derive(Clone)]
pub struct DeviceMonitor {
    monitor: gio::VolumeMonitor,
    callbacks: Rc<RefCell<Vec<DeviceCallback>>>,
}

impl DeviceMonitor {
    pub fn new() -> Self {
        let monitor = gio::VolumeMonitor::get();
        let callbacks = Rc::new(RefCell::new(Vec::new()));
        let signal_callbacks = callbacks.clone();
        monitor.connect_mount_added(move |monitor, _| {
            notify_subscribers(monitor, &signal_callbacks);
        });
        let signal_callbacks = callbacks.clone();
        monitor.connect_mount_changed(move |monitor, _| {
            notify_subscribers(monitor, &signal_callbacks);
        });
        let signal_callbacks = callbacks.clone();
        monitor.connect_mount_removed(move |monitor, _| {
            notify_subscribers(monitor, &signal_callbacks);
        });
        Self { monitor, callbacks }
    }

    pub fn devices(&self) -> Vec<DeviceDescriptor> {
        projected_devices(&self.monitor)
    }

    pub fn subscribe(&self, callback: DeviceCallback) {
        callback(self.devices());
        self.callbacks.borrow_mut().push(callback);
    }
}

impl Default for DeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

fn projected_devices(monitor: &gio::VolumeMonitor) -> Vec<DeviceDescriptor> {
    let mut devices = monitor
        .mounts()
        .iter()
        .filter_map(descriptor_from_mount)
        .collect::<Vec<_>>();
    devices.sort_by(|left, right| left.name.cmp(&right.name));
    devices
}

fn notify_subscribers(
    monitor: &gio::VolumeMonitor,
    subscribers: &Rc<RefCell<Vec<DeviceCallback>>>,
) {
    let devices = projected_devices(monitor);
    let subscribers = subscribers.borrow().clone();
    for subscriber in subscribers {
        subscriber(devices.clone());
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceFile {
    pub relative_path: String,
    pub name: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevicePlaylist {
    pub name: String,
    pub entries: Vec<M3uEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceContents {
    pub files: Vec<DeviceFile>,
    pub playlists: Vec<DevicePlaylist>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyOutcome {
    Copied,
    Skipped,
}

#[derive(Debug)]
pub enum DeviceIoError {
    InvalidRelativePath,
    Io(gio::glib::Error),
}

impl fmt::Display for DeviceIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRelativePath => formatter.write_str("invalid managed device path"),
            Self::Io(error) => write!(formatter, "device I/O failed: {error}"),
        }
    }
}

impl std::error::Error for DeviceIoError {}

impl From<gio::glib::Error> for DeviceIoError {
    fn from(error: gio::glib::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone)]
pub struct DeviceStorage {
    root: gio::File,
}

impl DeviceStorage {
    pub fn from_root(root: &gio::File) -> Self {
        Self { root: root.clone() }
    }

    pub fn from_uri(uri: &str) -> Self {
        Self::from_root(&gio::File::for_uri(uri))
    }

    pub async fn inspect(&self) -> Result<DeviceContents, DeviceIoError> {
        let music = self.root.child("Music");
        let mut pending = VecDeque::from([(music, String::new())]);
        let mut contents = DeviceContents::default();
        while let Some((directory, prefix)) = pending.pop_front() {
            let enumerator = match directory
                .enumerate_children_future(
                    ENUMERATE_ATTRIBUTES,
                    gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                    gio::glib::Priority::DEFAULT,
                )
                .await
            {
                Ok(enumerator) => enumerator,
                Err(error) if error.matches(gio::IOErrorEnum::NotFound) && prefix.is_empty() => {
                    return Ok(contents);
                }
                Err(error) => return Err(error.into()),
            };
            loop {
                let batch = enumerator
                    .next_files_future(ENUMERATE_BATCH_SIZE, gio::glib::Priority::DEFAULT)
                    .await?;
                if batch.is_empty() {
                    break;
                }
                for info in batch {
                    let name = info.name().to_string_lossy().into_owned();
                    let relative_path = join_relative(&prefix, &name);
                    let child = directory.child(&name);
                    if info.file_type() == gio::FileType::Directory {
                        pending.push_back((child, relative_path));
                    } else if is_audio_file(&name) {
                        contents.files.push(DeviceFile {
                            relative_path,
                            name,
                            size_bytes: info.size().max(0) as u64,
                        });
                    } else if prefix == "Reprise" && is_playlist_file(&name) {
                        let (bytes, _) = child.load_contents_future().await?;
                        let playlist_name = Path::new(&name)
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .unwrap_or(&name)
                            .to_string();
                        contents.playlists.push(DevicePlaylist {
                            name: playlist_name,
                            entries: parse_m3u(&String::from_utf8_lossy(&bytes)),
                        });
                    }
                }
            }
        }
        contents
            .files
            .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        contents
            .playlists
            .sort_by(|left, right| left.name.cmp(&right.name));
        Ok(contents)
    }

    pub async fn available_bytes(&self) -> Result<Option<u64>, DeviceIoError> {
        let info = self
            .root
            .query_filesystem_info_future(
                gio::FILE_ATTRIBUTE_FILESYSTEM_FREE,
                gio::glib::Priority::DEFAULT,
            )
            .await?;
        if info.has_attribute(gio::FILE_ATTRIBUTE_FILESYSTEM_FREE) {
            Ok(Some(
                info.attribute_uint64(gio::FILE_ATTRIBUTE_FILESYSTEM_FREE),
            ))
        } else {
            Ok(None)
        }
    }

    pub async fn copy_track<P>(
        &self,
        source: &gio::File,
        relative_path: &str,
        expected_size: u64,
        cancellable: &gio::Cancellable,
        progress: P,
    ) -> Result<CopyOutcome, DeviceIoError>
    where
        P: FnMut(u64, u64) + 'static,
    {
        let components = safe_relative_components(relative_path)?;
        self.ensure_managed_directories(&components[..components.len() - 1])
            .await?;
        let target = self.managed_child(&components);
        if target_size(&target).await? == Some(expected_size) {
            return Ok(CopyOutcome::Skipped);
        }
        let target_name = components.last().expect("validated nonempty path");
        let partial_components = components[..components.len() - 1]
            .iter()
            .cloned()
            .chain([format!("{target_name}{PARTIAL_SUFFIX}")])
            .collect::<Vec<_>>();
        let partial = self.managed_child(&partial_components);
        let progress = Rc::new(RefCell::new(progress));
        let callback_progress = progress.clone();
        let (sender, receiver) = async_channel::bounded(1);
        source.copy_async(
            &partial,
            gio::FileCopyFlags::OVERWRITE,
            gio::glib::Priority::DEFAULT,
            Some(cancellable),
            Some(Box::new(move |copied, total| {
                (callback_progress.borrow_mut())(copied.max(0) as u64, total.max(0) as u64);
            })),
            move |result| {
                let _ = sender.try_send(result);
            },
        );
        let copied = receiver
            .recv()
            .await
            .map_err(|_| DeviceIoError::InvalidRelativePath)?;
        if let Err(error) = copied {
            delete_if_present(&partial).await;
            return Err(error.into());
        }
        if let Err(error) = partial
            .move_future(
                &target,
                gio::FileCopyFlags::OVERWRITE,
                gio::glib::Priority::DEFAULT,
            )
            .0
            .await
        {
            delete_if_present(&partial).await;
            return Err(error.into());
        }
        (progress.borrow_mut())(expected_size, expected_size);
        Ok(CopyOutcome::Copied)
    }

    pub async fn replace_playlist(
        &self,
        playlist: &str,
        contents: Vec<u8>,
    ) -> Result<(), DeviceIoError> {
        let playlist = safe_component(playlist, "Playlist");
        self.ensure_managed_directories(&[]).await?;
        let final_file = self.managed_child(&[format!("{playlist}.m3u8")]);
        let partial = self.managed_child(&[format!("{playlist}.m3u8{PARTIAL_SUFFIX}")]);
        partial
            .replace_contents_future(
                contents,
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION,
            )
            .await
            .map_err(|(_, error)| DeviceIoError::Io(error))?;
        if let Err(error) = partial
            .move_future(
                &final_file,
                gio::FileCopyFlags::OVERWRITE,
                gio::glib::Priority::DEFAULT,
            )
            .0
            .await
        {
            delete_if_present(&partial).await;
            return Err(error.into());
        }
        Ok(())
    }

    pub async fn read_playlist(&self, playlist: &str) -> Result<Vec<M3uEntry>, DeviceIoError> {
        let playlist = safe_component(playlist, "Playlist");
        let file = self.managed_child(&[format!("{playlist}.m3u8")]);
        match file.load_contents_future().await {
            Ok((bytes, _)) => Ok(parse_m3u(&String::from_utf8_lossy(&bytes))),
            Err(error) if error.matches(gio::IOErrorEnum::NotFound) => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    async fn ensure_managed_directories(
        &self,
        relative_directories: &[String],
    ) -> Result<(), DeviceIoError> {
        let mut current = self.root.clone();
        for component in MANAGED_ROOT
            .iter()
            .map(|value| (*value).to_string())
            .chain(relative_directories.iter().cloned())
        {
            current = current.child(component);
            match current
                .make_directory_future(gio::glib::Priority::DEFAULT)
                .await
            {
                Ok(()) => {}
                Err(error) if error.matches(gio::IOErrorEnum::Exists) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn managed_child(&self, relative_components: &[String]) -> gio::File {
        MANAGED_ROOT
            .iter()
            .map(|component| (*component).to_string())
            .chain(relative_components.iter().cloned())
            .fold(self.root.clone(), |parent, component| {
                parent.child(component)
            })
    }
}

fn safe_relative_components(path: &str) -> Result<Vec<String>, DeviceIoError> {
    if path.is_empty() || path.chars().any(char::is_control) {
        return Err(DeviceIoError::InvalidRelativePath);
    }
    let components = Path::new(path)
        .components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or(DeviceIoError::InvalidRelativePath),
            _ => Err(DeviceIoError::InvalidRelativePath),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if components.is_empty() {
        Err(DeviceIoError::InvalidRelativePath)
    } else {
        Ok(components)
    }
}

async fn target_size(file: &gio::File) -> Result<Option<u64>, DeviceIoError> {
    match file
        .query_info_future(
            gio::FILE_ATTRIBUTE_STANDARD_SIZE,
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::glib::Priority::DEFAULT,
        )
        .await
    {
        Ok(info) => Ok(Some(info.size().max(0) as u64)),
        Err(error) if error.matches(gio::IOErrorEnum::NotFound) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

async fn delete_if_present(file: &gio::File) {
    if let Err(error) = file.delete_future(gio::glib::Priority::DEFAULT).await {
        if !error.matches(gio::IOErrorEnum::NotFound) {
            tracing::warn!(%error, "failed to remove partial device sync file");
        }
    }
}

fn join_relative(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

fn is_audio_file(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp3" | "flac" | "ogg" | "opus" | "m4a" | "aac" | "wav"
            )
        })
}

fn is_playlist_file(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("m3u8"))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::future::Future;
    use std::rc::Rc;

    use gio::prelude::*;
    use reprise_core::library::m3u::M3uEntry;
    use tempfile::TempDir;

    use super::*;

    fn run<T>(future: impl Future<Output = T>) -> T {
        let context = gio::glib::MainContext::new();
        context
            .with_thread_default(|| context.block_on(future))
            .unwrap()
    }

    fn fixture() -> (TempDir, DeviceStorage) {
        let temp = tempfile::tempdir().unwrap();
        let root = gio::File::for_path(temp.path());
        (temp, DeviceStorage::from_root(&root))
    }

    #[test]
    fn descriptor_projection_accepts_only_mtp_roots() {
        assert!(project_descriptor("file:///tmp", Some("uuid"), "Disk").is_none());
        assert!(project_descriptor("gphoto2://phone", Some("uuid"), "Camera").is_none());
        assert!(project_descriptor("mtp://phone", Some("uuid"), "Phone").is_some());
    }

    #[test]
    fn descriptor_prefers_uuid_for_stable_reconnects() {
        let descriptor = project_descriptor("mtp://phone", Some("serial-1"), "Pixel").unwrap();
        assert_eq!(descriptor.id, "serial-1");
        assert!(descriptor.reconnectable);
    }

    #[test]
    fn descriptor_falls_back_to_uri_without_claiming_reconnect_support() {
        let descriptor = project_descriptor("mtp://phone", None, "Pixel").unwrap();
        assert_eq!(descriptor.id, "mtp://phone");
        assert!(!descriptor.reconnectable);
    }

    #[test]
    fn missing_music_directory_inspects_as_empty() {
        let (_temp, storage) = fixture();
        let contents = run(storage.inspect()).unwrap();
        assert!(contents.files.is_empty());
        assert!(contents.playlists.is_empty());
    }

    #[test]
    fn inspection_finds_audio_and_reprise_playlists_but_not_other_files() {
        let (temp, storage) = fixture();
        fs::create_dir_all(temp.path().join("Music/Reprise/Road")).unwrap();
        fs::write(temp.path().join("Music/Reprise/Road/1-song.flac"), b"audio").unwrap();
        fs::write(temp.path().join("Music/loose.mp3"), b"audio2").unwrap();
        fs::write(temp.path().join("Music/notes.txt"), b"ignore").unwrap();
        fs::write(
            temp.path().join("Music/Reprise/Road.m3u8"),
            b"#EXTM3U\nRoad/1-song.flac\n",
        )
        .unwrap();

        let contents = run(storage.inspect()).unwrap();
        let paths = contents
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, ["Reprise/Road/1-song.flac", "loose.mp3"]);
        assert_eq!(contents.playlists.len(), 1);
        assert_eq!(contents.playlists[0].name, "Road");
        assert_eq!(
            contents.playlists[0].entries,
            vec![M3uEntry {
                path: "Road/1-song.flac".into()
            }]
        );
    }

    #[test]
    fn copy_creates_managed_directories_and_reports_progress() {
        let (temp, storage) = fixture();
        let source_path = temp.path().join("source.flac");
        fs::write(&source_path, vec![7_u8; 32 * 1024]).unwrap();
        let progress = Rc::new(RefCell::new(Vec::new()));
        let observed = progress.clone();
        let outcome = run(storage.copy_track(
            &gio::File::for_path(&source_path),
            "Road/7-source.flac",
            32 * 1024,
            &gio::Cancellable::new(),
            move |copied, total| observed.borrow_mut().push((copied, total)),
        ))
        .unwrap();

        assert_eq!(outcome, CopyOutcome::Copied);
        assert_eq!(
            fs::read(temp.path().join("Music/Reprise/Road/7-source.flac")).unwrap(),
            vec![7_u8; 32 * 1024]
        );
        assert!(progress
            .borrow()
            .windows(2)
            .all(|pair| pair[0].0 <= pair[1].0));
        assert_eq!(
            progress.borrow().last().copied(),
            Some((32 * 1024, 32 * 1024))
        );
    }

    #[test]
    fn same_size_destination_is_skipped_without_overwrite() {
        let (temp, storage) = fixture();
        fs::create_dir_all(temp.path().join("Music/Reprise/Road")).unwrap();
        fs::write(temp.path().join("source.flac"), b"new!").unwrap();
        fs::write(
            temp.path().join("Music/Reprise/Road/7-source.flac"),
            b"old!",
        )
        .unwrap();
        let outcome = run(storage.copy_track(
            &gio::File::for_path(temp.path().join("source.flac")),
            "Road/7-source.flac",
            4,
            &gio::Cancellable::new(),
            |_copied, _total| {},
        ))
        .unwrap();
        assert_eq!(outcome, CopyOutcome::Skipped);
        assert_eq!(
            fs::read(temp.path().join("Music/Reprise/Road/7-source.flac")).unwrap(),
            b"old!"
        );
    }

    #[test]
    fn copy_rejects_paths_outside_the_managed_root() {
        let (temp, storage) = fixture();
        fs::write(temp.path().join("source.flac"), b"x").unwrap();
        let result = run(storage.copy_track(
            &gio::File::for_path(temp.path().join("source.flac")),
            "../outside.flac",
            1,
            &gio::Cancellable::new(),
            |_copied, _total| {},
        ));
        assert!(matches!(result, Err(DeviceIoError::InvalidRelativePath)));
        assert!(!temp.path().join("Music/outside.flac").exists());
    }

    #[test]
    fn playlist_replace_and_read_round_trip() {
        let (_temp, storage) = fixture();
        run(storage.replace_playlist("Road", b"#EXTM3U\nRoad/7-song.flac\n".to_vec())).unwrap();
        assert_eq!(
            run(storage.read_playlist("Road")).unwrap(),
            vec![M3uEntry {
                path: "Road/7-song.flac".into()
            }]
        );
    }

    #[test]
    fn pre_cancelled_copy_leaves_no_partial_file() {
        let (temp, storage) = fixture();
        fs::write(temp.path().join("source.flac"), vec![1_u8; 1024]).unwrap();
        let cancellable = gio::Cancellable::new();
        cancellable.cancel();
        let result = run(storage.copy_track(
            &gio::File::for_path(temp.path().join("source.flac")),
            "Road/1-source.flac",
            1024,
            &cancellable,
            |_copied, _total| {},
        ));
        assert!(result.is_err());
        assert!(!temp
            .path()
            .join("Music/Reprise/Road/1-source.flac.reprise-part")
            .exists());
    }

    #[test]
    fn local_fixture_reports_available_space_when_supported() {
        let (_temp, storage) = fixture();
        assert!(run(storage.available_bytes()).unwrap().is_some());
    }
}
