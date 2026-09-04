//! Reading one Reprise-owned file below a resolved device sync target.

use super::*;
use reprise_core::device_sync::ManagedDeviceFile;

impl DeviceStorage {
    /// Reports whether the resolved managed target folder currently exists.
    pub async fn managed_target_exists(
        &self,
        storage_id: Option<StorageId>,
        target_path: &str,
    ) -> Result<bool, DeviceIoError> {
        let storage = self.resolve_target_storage(storage_id).await?;
        let target = Self::managed_child(&storage, target_path, &[])?;
        match target
            .query_info_future(
                gio::FILE_ATTRIBUTE_STANDARD_TYPE,
                gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                gio::glib::Priority::DEFAULT,
            )
            .await
        {
            Ok(info) => Ok(info.file_type() == gio::FileType::Directory),
            Err(error) if error.matches(gio::IOErrorEnum::NotFound) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    /// Reads one managed file without manufacturing an error for absence.
    ///
    /// A phone report is optional input to a synchronization run: malformed
    /// bytes are handled by the caller, while an absent file is the normal
    /// pre-feature and no-new-actions state.
    pub async fn read_managed(
        &self,
        storage_id: Option<StorageId>,
        target_path: &str,
        relative_path: &str,
    ) -> Result<Option<Vec<u8>>, DeviceIoError> {
        let components = safe_relative_components(relative_path)?;
        let storage = self.resolve_target_storage(storage_id).await?;
        let file = Self::managed_child(&storage, target_path, &components)?;
        match file.load_contents_future().await {
            Ok((bytes, _)) => Ok(Some(bytes.to_vec())),
            Err(error) if error.matches(gio::IOErrorEnum::NotFound) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Recovers present, non-empty files from a managed walk that came back short.
    pub async fn probe_managed(
        &self,
        storage_id: Option<StorageId>,
        target_path: &str,
        relative_paths: &[String],
    ) -> Result<Vec<ManagedDeviceFile>, DeviceIoError> {
        let storage = self.resolve_target_storage(storage_id).await?;
        let mut recovered = Vec::new();
        for relative_path in relative_paths {
            let components = safe_relative_components(relative_path)?;
            let file = Self::managed_child(&storage, target_path, &components)?;
            match file
                .query_info_future(
                    "standard::type,standard::size",
                    gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                    gio::glib::Priority::DEFAULT,
                )
                .await
            {
                Ok(info) if info.file_type() == gio::FileType::Regular && info.size() > 0 => {
                    recovered.push(ManagedDeviceFile {
                        relative_path: relative_path.clone(),
                        size_bytes: info.size() as u64,
                    });
                }
                Ok(_) => {}
                Err(error) if error.matches(gio::IOErrorEnum::NotFound) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(recovered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_managed_rejects_paths_outside_the_managed_root() {
        let (temp, storage) = super::super::tests::fixture();

        let result = super::super::tests::run(storage.probe_managed(
            None,
            "/Music/Reprise",
            &["../outside".into()],
        ));

        assert!(matches!(result, Err(DeviceIoError::InvalidRelativePath)));
        assert!(!temp.path().join("Music/outside").exists());
    }

    #[test]
    fn probe_managed_does_not_recover_a_directory_at_a_track_path() {
        let (temp, storage) = super::super::tests::fixture();
        let directory = temp.path().join("Music/Reprise/Artist/Track.opus");
        std::fs::create_dir_all(&directory).unwrap();

        let recovered = super::super::tests::run(storage.probe_managed(
            None,
            "/Music/Reprise",
            &["Artist/Track.opus".into()],
        ))
        .unwrap();

        assert!(recovered.is_empty());
    }
}
