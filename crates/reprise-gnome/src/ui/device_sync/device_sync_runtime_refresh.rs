use std::collections::HashSet;
use std::rc::Rc;

use reprise_core::device_sync::settings::{
    load_device_files, mark_device_playlists_synced, record_device_verification,
};
use reprise_core::device_sync::{
    aggregate_balance, should_auto_start, AutoStartFacts, DeviceStorageInspection, SelectionSource,
};

use super::{compact, DeviceSyncRuntime, PlannedSyncPhase, RefreshPurpose, SyncFailure};

impl DeviceSyncRuntime {
    /// Same as [`Self::refresh_contents`], except this refresh is the first
    /// one after the device connected (`apply_devices`, both a brand-new
    /// device and a reconnect) — the only refresh `MTP-30`'s auto-start
    /// decision is allowed to fire from. A manual "Refresh" click or the
    /// post-sync verify refresh must never re-trigger it.
    pub(super) fn refresh_contents_on_connect(self: &Rc<Self>, device_id: &str) {
        self.refresh_contents_with_delta(device_id, true, RefreshPurpose::Normal, true);
    }

    pub(super) fn refresh_contents_after_sync(
        self: &Rc<Self>,
        device_id: &str,
        sources: Vec<SelectionSource>,
    ) {
        self.refresh_contents_with_delta(
            device_id,
            true,
            RefreshPurpose::VerifySync(sources),
            false,
        );
    }

    pub(super) fn refresh_contents_with_delta(
        self: &Rc<Self>,
        device_id: &str,
        recompute_delta: bool,
        purpose: RefreshPurpose,
        just_connected: bool,
    ) {
        let target = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| device.target.clone());
        let Some(target) = target else {
            return;
        };
        let request = {
            let mut devices = self.device_states.borrow_mut();
            let Some(device) = devices
                .iter_mut()
                .find(|device| device.descriptor.id == device_id)
            else {
                return;
            };
            if !device.connected || !device.session_state.opens_session() {
                return;
            }
            device.scan_generation = device.scan_generation.saturating_add(1);
            device.scanning = true;
            device.scan_error = None;
            if just_connected {
                device.residency_proven = false;
                device.short_scan = None;
            }
            Some((device.descriptor.root_uri.clone(), device.scan_generation))
        };
        self.notify();
        let Some((root_uri, generation)) = request else {
            return;
        };
        let backend = self.backend.clone();
        let weak = self.weak_self.borrow().clone();
        let id = device_id.to_string();
        gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
            let mut result = backend.inspect(root_uri.clone(), target.clone()).await;
            let Some(runtime) = weak.upgrade() else {
                return;
            };
            let current_generation = runtime
                .device_states
                .borrow()
                .iter()
                .find(|device| device.descriptor.id == id)
                .is_some_and(|device| device.scan_generation == generation);
            if !current_generation {
                return;
            }
            let mut residency_proven = false;
            let mut short_scan = None;
            if let Ok(inspection) = &mut result {
                match load_device_files(&runtime.conn, &id) {
                    Ok(inventory) => {
                        let walked = inspection
                            .managed_files
                            .iter()
                            .map(|file| file.relative_path.to_lowercase())
                            .collect::<HashSet<_>>();
                        let mut doubtful_keys = HashSet::new();
                        let mut doubtful = Vec::new();
                        for file in &inventory {
                            let audio_key = file.device_path.to_lowercase();
                            if !walked.contains(&audio_key) && doubtful_keys.insert(audio_key) {
                                doubtful.push(file.device_path.clone());
                            }
                            if let Some(sidecar) =
                                reprise_core::device_sync::analysis_sidecar::device_path_for_track(
                                    &file.device_path,
                                )
                            {
                                let sidecar_key = sidecar.to_lowercase();
                                if !walked.contains(&sidecar_key) {
                                    let has_analysis = match reprise_core::device_sync::analysis_sidecar::AnalysisSidecar::for_track(
                                        &runtime.conn,
                                        file.track_id,
                                    ) {
                                        Ok(analysis) => analysis.is_some(),
                                        Err(error) => {
                                            tracing::warn!(
                                                track_id = file.track_id,
                                                %error,
                                                "could not load analysis sidecar data for device scan repair"
                                            );
                                            false
                                        }
                                    };
                                    if has_analysis && doubtful_keys.insert(sidecar_key) {
                                        doubtful.push(sidecar);
                                    }
                                }
                            }
                        }
                        let doubtful_count = doubtful.len();
                        if doubtful_count == 0 {
                            residency_proven = true;
                        } else {
                            match backend
                                .managed_target_exists(
                                    root_uri.clone(),
                                    target.path.clone(),
                                    target.storage_id,
                                )
                                .await
                            {
                                Ok(true) => match backend
                                    .probe_managed_files(
                                        root_uri,
                                        target.path,
                                        target.storage_id,
                                        doubtful,
                                    )
                                    .await
                                {
                                    Ok(recovered) => {
                                        let recovered_count = recovered.len();
                                        inspection.managed_files.extend(recovered);
                                        residency_proven = true;
                                        if recovered_count > 0 {
                                            tracing::warn!(
                                                doubtful = doubtful_count,
                                                recovered = recovered_count,
                                                "device scan came back short"
                                            );
                                            short_scan =
                                                Some((doubtful_count, recovered_count));
                                        }
                                    }
                                    Err(error) => {
                                        tracing::warn!(
                                            %error,
                                            doubtful = doubtful_count,
                                            "device scan residency could not be proven"
                                        );
                                    }
                                },
                                Ok(false) => tracing::warn!(
                                    doubtful = doubtful_count,
                                    "device scan target folder is unavailable"
                                ),
                                Err(error) => {
                                    tracing::warn!(
                                        %error,
                                        doubtful = doubtful_count,
                                        "device scan target folder could not be checked"
                                    );
                                }
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "device scan residency inventory could not be loaded");
                    }
                }
            }
            let current_generation = runtime
                .device_states
                .borrow()
                .iter()
                .find(|device| device.descriptor.id == id)
                .is_some_and(|device| device.scan_generation == generation);
            if !current_generation {
                return;
            }
            let verified_track_count = result
                .as_ref()
                .ok()
                .map(|inspection| {
                    inspection
                        .managed_files
                        .iter()
                        .filter(|file| compact::is_verified_track_file(file))
                        .count()
                });
            let inspection_error = result.as_ref().err().cloned();
            {
                let mut devices = runtime.device_states.borrow_mut();
                if let Some(device) = devices.iter_mut().find(|device| device.descriptor.id == id) {
                    if device.scan_generation != generation {
                        return;
                    }
                    device.scanning = false;
                    match result {
                        Ok(DeviceStorageInspection {
                            snapshot,
                            managed_files,
                            partial_paths,
                            lyrics_files,
                        }) => {
                            device.storage = snapshot;
                            device.managed_files = managed_files;
                            device.partial_paths = partial_paths;
                            device.lyrics_files = lyrics_files;
                            device.scan_error = None;
                            device.ever_inspected = true;
                            device.residency_proven = residency_proven;
                            device.short_scan = short_scan;
                        }
                        Err(error) => {
                            device.scan_error = Some(error);
                            device.residency_proven = false;
                            device.short_scan = None;
                        }
                    }
                }
            }
            if !recompute_delta {
                runtime.notify();
            } else {
                let planning_error = runtime.recompute_delta_silent(&id).err();
                let mut playlist_timestamp_error = None;
                let verified_at = match &purpose {
                    RefreshPurpose::VerifySync(sources)
                        if inspection_error.is_none() && planning_error.is_none() =>
                    {
                        let verified_at = chrono::Utc::now();
                        let rememberable = runtime
                            .device_states
                            .borrow()
                            .iter()
                            .find(|device| device.descriptor.id == id)
                            .is_some_and(|device| device.descriptor.persistent_id.is_some());
                        if rememberable {
                            let size_on_device = runtime
                                .device_states
                                .borrow()
                                .iter()
                                .find(|device| device.descriptor.id == id)
                                .map_or(0, |device| {
                                    compact::verified_track_bytes(&device.managed_files)
                                });
                            if let Err(error) = mark_device_playlists_synced(
                                &runtime.conn,
                                &id,
                                sources,
                                verified_at.timestamp(),
                            ) {
                                playlist_timestamp_error = Some(format!(
                                    "could not record verified playlist synchronization: {error}"
                                ));
                                None
                            } else if let Err(error) = record_device_verification(
                                &runtime.conn,
                                &id,
                                verified_at.timestamp(),
                                size_on_device,
                            ) {
                                playlist_timestamp_error = Some(format!(
                                    "could not remember verified device state: {error}"
                                ));
                                None
                            } else {
                                Some(verified_at)
                            }
                        } else {
                            Some(verified_at)
                        }
                    }
                    _ => None,
                };
                if let Some(device) = runtime
                    .device_states
                    .borrow_mut()
                    .iter_mut()
                    .find(|device| device.descriptor.id == id)
                {
                    match &purpose {
                        RefreshPurpose::VerifySync(sources)
                            if inspection_error.is_none()
                                && planning_error.is_none()
                                && playlist_timestamp_error.is_none() =>
                        {
                            if let Some(verified_at) = verified_at {
                                for row in &mut device.page.playlists {
                                    if sources.contains(&row.source) {
                                        row.last_synced_at = Some(verified_at.timestamp());
                                    }
                                }
                                device.last_sync = Some(verified_at);
                                device.verified_managed_track_count = verified_track_count;
                                let verified_size =
                                    compact::verified_track_bytes(&device.managed_files);
                                device.last_verified_size_bytes = Some(verified_size);
                                device.size_on_device_bytes = Some(verified_size);
                                device.sync_error = None;
                            }
                        }
                        RefreshPurpose::VerifySync(_) => {
                            device.sync_phase = PlannedSyncPhase::Idle;
                            device.sync_error = Some(SyncFailure {
                                message: inspection_error.clone().map_or_else(
                                    || {
                                        planning_error.clone().unwrap_or_else(|| {
                                            playlist_timestamp_error.clone().unwrap_or_else(|| {
                                                "device content verification failed".into()
                                            })
                                        })
                                    },
                                    |error| {
                                        format!(
                                            "could not verify device contents after synchronization: {error}"
                                        )
                                    },
                                ),
                                failed_tracks: Vec::new(),
                            });
                        }
                        RefreshPurpose::Normal => {
                            if let Some(error) = planning_error.clone() {
                                device.sync_phase = PlannedSyncPhase::Idle;
                                device.sync_error = Some(SyncFailure {
                                    message: error,
                                    failed_tracks: Vec::new(),
                                });
                            }
                        }
                    }
                }
                runtime.notify();
                let resume_initiator = {
                    let mut devices = runtime.device_states.borrow_mut();
                    devices
                        .iter_mut()
                        .find(|device| device.descriptor.id == id)
                        .and_then(|device| {
                            let resume = device.resume_initiator.is_some()
                                && device.connected
                                && !device.is_active();
                            if resume {
                                device.resume_initiator.take()
                            } else {
                                None
                            }
                        })
                };
                if let Some(initiator) = resume_initiator {
                    if let Err(error) = runtime.start_sync(&id, initiator) {
                        tracing::warn!(device_id = id, %error, "could not resume device synchronization");
                    }
                } else if just_connected {
                    // `MTP-30`: gather every fact from one short borrow, drop
                    // it, and only then decide — same discipline as
                    // `should_resume` above. A refused or failed automatic
                    // start is silent apart from this log: the user did not
                    // press anything, so it must never raise a modal or an
                    // error banner.
                    let facts = {
                        let devices = runtime.device_states.borrow();
                        devices
                            .iter()
                            .find(|device| device.descriptor.id == id)
                            .map(|device| AutoStartFacts {
                                just_connected,
                                sync_automatically: device.settings.sync_automatically,
                                scan_ok: device.scan_error.is_none(),
                                planning_ok: planning_error.is_none(),
                                device_connected: device.connected,
                                device_busy: device.is_busy(),
                                balance: aggregate_balance(&[device.target_reading()]),
                            })
                    };
                    if facts.is_some_and(should_auto_start) {
                        if let Err(error) = runtime.sync_automatically(&id) {
                            tracing::warn!(device_id = id, %error, "could not start automatic device synchronization");
                        }
                    }
                }
            }
        });
    }
}
