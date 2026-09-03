//! Reading one Reprise-owned file below a resolved device sync target.

use super::*;
use reprise_core::device_sync::ManagedDeviceFile;

impl DeviceStorage {
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
            let result = safe_relative_components(relative_path)
                .and_then(|components| Self::managed_child(&storage, target_path, &components));
            let file = match result {
                Ok(file) => file,
                Err(error) => {
                    tracing::debug!(path = %relative_path, %error, "device sync: could not probe managed file");
                    continue;
                }
            };
            match target_size(&file).await {
                Ok(Some(size_bytes)) if size_bytes > 0 => recovered.push(ManagedDeviceFile {
                    relative_path: relative_path.clone(),
                    size_bytes,
                }),
                Ok(_) => {}
                Err(error) => {
                    tracing::debug!(path = %relative_path, %error, "device sync: could not probe managed file");
                }
            }
        }
        Ok(recovered)
    }
}
