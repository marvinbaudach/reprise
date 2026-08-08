//! Reading one Reprise-owned file below a resolved device sync target.

use gio::prelude::*;

use super::*;

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
}
