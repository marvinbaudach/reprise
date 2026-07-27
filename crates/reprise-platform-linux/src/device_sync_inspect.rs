//! Read-only projection of music and Reprise-managed podcast device content.

use super::*;

impl DeviceStorage {
    /// Lists all audio below `Music` and only Reprise-owned audio below
    /// `Podcasts/Reprise`. Device deletion remains independently root-scoped.
    pub async fn inspect(&self) -> Result<DeviceContents, DeviceIoError> {
        let storage = self.storage_root().await?;
        let mut contents = DeviceContents::default();
        inspect_music(&storage, &mut contents).await?;
        inspect_podcasts(&storage, &mut contents).await?;
        contents
            .files
            .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        contents
            .playlists
            .sort_by(|left, right| left.name.cmp(&right.name));
        contents
            .podcast_files
            .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Ok(contents)
    }
}

async fn inspect_music(
    storage: &gio::File,
    contents: &mut DeviceContents,
) -> Result<(), DeviceIoError> {
    let mut pending = VecDeque::from([(storage.child("Music"), String::new())]);
    while let Some((directory, prefix)) = pending.pop_front() {
        let Some(enumerator) = enumerate(&directory, prefix.is_empty()).await? else {
            continue;
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
                    contents.files.push(device_file(relative_path, name, &info));
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
    Ok(())
}

async fn inspect_podcasts(
    storage: &gio::File,
    contents: &mut DeviceContents,
) -> Result<(), DeviceIoError> {
    let root = storage.child("Podcasts").child("Reprise");
    let mut pending = VecDeque::from([(root, String::new())]);
    while let Some((directory, prefix)) = pending.pop_front() {
        let Some(enumerator) = enumerate(&directory, prefix.is_empty()).await? else {
            continue;
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
                    contents
                        .podcast_files
                        .push(device_file(relative_path, name, &info));
                }
            }
        }
    }
    Ok(())
}

async fn enumerate(
    directory: &gio::File,
    root: bool,
) -> Result<Option<gio::FileEnumerator>, DeviceIoError> {
    match directory
        .enumerate_children_future(
            ENUMERATE_ATTRIBUTES,
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gio::glib::Priority::DEFAULT,
        )
        .await
    {
        Ok(enumerator) => Ok(Some(enumerator)),
        Err(error) if root && error.matches(gio::IOErrorEnum::NotFound) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn device_file(relative_path: String, name: String, info: &gio::FileInfo) -> DeviceFile {
    DeviceFile {
        relative_path,
        name,
        size_bytes: info.size().max(0) as u64,
    }
}
