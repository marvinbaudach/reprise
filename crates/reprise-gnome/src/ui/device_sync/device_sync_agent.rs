//! Live, path-free bridge between the GTK-owned sync runtime and local agents.

use reprise_core::agent_device_sync::{
    AgentDeviceSyncBlocker, AgentDeviceSyncCategoryRow, AgentDeviceSyncChanges,
    AgentDeviceSyncCommand, AgentDeviceSyncControls, AgentDeviceSyncDevice, AgentDeviceSyncPhase,
    AgentDeviceSyncPlaylist, AgentDeviceSyncRequest, AgentDeviceSyncState, AgentDeviceSyncStorage,
    AgentDeviceSyncStorageAccess, AgentDeviceSyncStorageComposition,
    AgentDeviceSyncStorageKnowledge, AgentDeviceSyncStorageState, AgentDeviceSyncWarning,
    SharedAgentDeviceSyncState,
};
use reprise_core::device_sync::{
    DeviceStorageAccess, MirrorBlocker, SelectionSource, StorageComposition, StorageKnowledge,
    StorageProjectionState, SyncPageWarning,
};

use super::*;

impl DeviceSyncRuntime {
    pub fn bind_agent_device_sync(
        self: &Rc<Self>,
        shared: &SharedAgentDeviceSyncState,
        commands: async_channel::Receiver<AgentDeviceSyncRequest>,
    ) {
        let mirror = shared.clone();
        let subscription = self.subscribe(Rc::new(move |state| {
            *mirror
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = agent_state(state);
        }));
        self.agent_subscription.replace(Some(subscription));

        let weak = Rc::downgrade(self);
        gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
            while let Ok(request) = commands.recv().await {
                let Some(runtime) = weak.upgrade() else {
                    break;
                };
                let result = runtime.apply_agent_command(request.command);
                let _ = request.reply.send(result);
            }
        });
    }

    fn apply_agent_command(self: &Rc<Self>, command: AgentDeviceSyncCommand) -> Result<(), String> {
        match command {
            AgentDeviceSyncCommand::Configure {
                device_name,
                sources,
                profile,
            } => {
                let mut settings = self
                    .unique_connected_device(&device_name)
                    .map(|device| device.settings)
                    .ok_or_else(|| {
                        format!("device '{device_name}' is absent, disconnected, or ambiguous")
                    })?;
                let options = self
                    .selection_options()
                    .map_err(|error| format!("could not resolve sync playlist: {error}"))?;
                let unique = sources
                    .iter()
                    .cloned()
                    .collect::<std::collections::HashSet<_>>();
                if unique.len() != sources.len() {
                    return Err("playlist sources must not contain duplicates".into());
                }
                let available = options
                    .into_iter()
                    .map(|option| option.source)
                    .collect::<std::collections::HashSet<_>>();
                if let Some(source) = sources.iter().find(|source| !available.contains(*source)) {
                    return Err(format!(
                        "playlist source '{}' does not exist",
                        source_name(source)
                    ));
                }
                settings.selection = DeviceSelection::Sources(sources);
                settings.remove_deleted = true;
                settings.profile = profile;
                settings.opus_bitrate = 0;
                self.update_settings(settings)
            }
            AgentDeviceSyncCommand::Start { device_name } => {
                let device_id = self
                    .unique_connected_device(&device_name)
                    .map(|device| device.id)
                    .ok_or_else(|| {
                        format!("device '{device_name}' is absent, disconnected, or ambiguous")
                    })?;
                self.sync_now(&device_id).map_err(|error| error.to_string())
            }
            AgentDeviceSyncCommand::Cancel { device_name } => {
                let device_id = self
                    .unique_connected_device(&device_name)
                    .map(|device| device.id)
                    .ok_or_else(|| {
                        format!("device '{device_name}' is absent, disconnected, or ambiguous")
                    })?;
                self.cancel_current(&device_id);
                Ok(())
            }
            AgentDeviceSyncCommand::Eject { device_name } => {
                let device = self.unique_connected_device(&device_name).ok_or_else(|| {
                    format!("device '{device_name}' is absent, disconnected, or ambiguous")
                })?;
                if !device.page.controls.can_eject {
                    return Err(format!(
                        "device '{device_name}' cannot be ejected while busy"
                    ));
                }
                self.eject(&device.id);
                Ok(())
            }
        }
    }

    fn unique_connected_device(&self, name: &str) -> Option<DeviceView> {
        let devices = self
            .devices()
            .into_iter()
            .filter(|device| device.connected && device.name == name)
            .collect::<Vec<_>>();
        (devices.len() == 1).then(|| devices[0].clone())
    }
}

fn source_name(source: &SelectionSource) -> String {
    match source {
        SelectionSource::Playlist(id) => format!("playlist:{id}"),
        SelectionSource::Smart(id) => format!("smart:{id}"),
    }
}

fn agent_state(state: DeviceSyncState) -> AgentDeviceSyncState {
    AgentDeviceSyncState {
        devices: state.devices.into_iter().map(agent_device).collect(),
    }
}

fn agent_device(device: DeviceView) -> AgentDeviceSyncDevice {
    let (phase, bytes_done, bytes_total, current_track) = agent_phase(device.sync_phase);
    let page = device.page;
    AgentDeviceSyncDevice {
        name: device.name,
        connected: device.connected,
        last_synced_at: device.last_sync.map(|last_sync| last_sync.timestamp()),
        managed_tracks: device.managed_track_count,
        profile: device.settings.profile,
        playlists: page
            .playlists
            .into_iter()
            .map(|playlist| AgentDeviceSyncPlaylist {
                source: playlist.source,
                name: playlist.name,
                selected: playlist.selected,
                available: playlist.available,
                entry_count: playlist.entry_count,
                unique_track_count: playlist.unique_track_count,
                unavailable_count: playlist.unavailable_count,
                target_bytes: playlist.target_bytes,
                last_synced_at: playlist.last_synced_at,
            })
            .collect(),
        unique_track_count: page.unique_track_count,
        target_bytes: page.target_bytes,
        changes: AgentDeviceSyncChanges {
            additions: page.changes.additions,
            replacements: page.changes.replacements,
            removals: page.changes.removals,
            retained_unavailable: page.changes.retained_unavailable,
            playlist_writes: page.changes.playlist_writes,
            playlist_removals: page.changes.playlist_removals,
            transfer_bytes: page.changes.transfer_bytes,
        },
        storage: AgentDeviceSyncStorage {
            target_name: page.storage.target_name,
            access: storage_access(page.storage.access),
            state: storage_state(page.storage.state),
            transfer_bytes: page.storage.transfer_bytes,
            current: storage_composition(&page.storage.current),
            after_sync: page.storage.after_sync.as_ref().map(storage_composition),
        },
        blockers: page.blockers.into_iter().map(agent_blocker).collect(),
        warnings: agent_warnings(page.warnings),
        controls: AgentDeviceSyncControls {
            editable: page.controls.editable,
            can_start: page.controls.can_start,
            can_cancel: page.controls.can_cancel,
            can_eject: page.controls.can_eject,
        },
        phase,
        bytes_done,
        bytes_total,
        bytes_per_second: device.bytes_per_second,
        current_track,
        // Block H (MCP parity): reuses the exact `content_rows`/
        // `category_readings` the GTK device page already renders (`MTP-38`/
        // `MTP-22`) — no second computation.
        categories: device
            .content_rows
            .into_iter()
            .zip(device.category_readings)
            .map(|(row, reading)| AgentDeviceSyncCategoryRow {
                kind: row.kind,
                target_path: row.target_path,
                target_enabled: row.target_enabled,
                size_on_device_bytes: row.size_on_device_bytes,
                cap_bytes: row.cap_bytes,
                reading,
            })
            .collect(),
    }
}

fn storage_access(access: DeviceStorageAccess) -> AgentDeviceSyncStorageAccess {
    match access {
        DeviceStorageAccess::Writable => AgentDeviceSyncStorageAccess::Writable,
        DeviceStorageAccess::ReadOnly => AgentDeviceSyncStorageAccess::ReadOnly,
        DeviceStorageAccess::Unknown => AgentDeviceSyncStorageAccess::Unknown,
    }
}

fn storage_state(state: StorageProjectionState) -> AgentDeviceSyncStorageState {
    match state {
        StorageProjectionState::Fits => AgentDeviceSyncStorageState::Fits,
        StorageProjectionState::Insufficient { shortfall_bytes } => {
            AgentDeviceSyncStorageState::Insufficient { shortfall_bytes }
        }
        StorageProjectionState::CapacityUnknown => AgentDeviceSyncStorageState::CapacityUnknown,
        StorageProjectionState::Inconsistent => AgentDeviceSyncStorageState::Inconsistent,
        StorageProjectionState::Blocked => AgentDeviceSyncStorageState::Blocked,
    }
}

fn storage_composition(composition: &StorageComposition) -> AgentDeviceSyncStorageComposition {
    AgentDeviceSyncStorageComposition {
        total_bytes: composition.total_bytes,
        reprise_music_bytes: composition.reprise_music_bytes,
        other_music_bytes: composition.other_music_bytes,
        other_used_bytes: composition.other_used_bytes,
        free_bytes: composition.free_bytes,
        knowledge: match composition.knowledge {
            StorageKnowledge::Complete => AgentDeviceSyncStorageKnowledge::Complete,
            StorageKnowledge::CapacityUnknown => AgentDeviceSyncStorageKnowledge::CapacityUnknown,
            StorageKnowledge::Inconsistent => AgentDeviceSyncStorageKnowledge::Inconsistent,
        },
    }
}

fn agent_blocker(blocker: MirrorBlocker) -> AgentDeviceSyncBlocker {
    match blocker {
        MirrorBlocker::NoPlaylistsSelected => AgentDeviceSyncBlocker::NoPlaylistsSelected,
        MirrorBlocker::MissingPlaylist(source) => AgentDeviceSyncBlocker::MissingPlaylist(source),
        MirrorBlocker::DuplicatePlaylist(source) => {
            AgentDeviceSyncBlocker::DuplicatePlaylist(source)
        }
    }
}

fn agent_warning(warning: SyncPageWarning) -> AgentDeviceSyncWarning {
    match warning {
        SyncPageWarning::UnavailableNotOnDevice { .. } => {
            AgentDeviceSyncWarning::UnavailableNotOnDevice
        }
        SyncPageWarning::UnsafeManagedItem => AgentDeviceSyncWarning::UnsafeManagedItem,
    }
}

fn agent_warnings(warnings: Vec<SyncPageWarning>) -> Vec<AgentDeviceSyncWarning> {
    let mut projected = Vec::new();
    for warning in warnings.into_iter().map(agent_warning) {
        if !projected.contains(&warning) {
            projected.push(warning);
        }
    }
    projected
}

fn agent_phase(phase: PlannedSyncPhase) -> (AgentDeviceSyncPhase, u64, u64, String) {
    match phase {
        PlannedSyncPhase::Idle => (AgentDeviceSyncPhase::Idle, 0, 0, String::new()),
        PlannedSyncPhase::ComputingDelta => {
            (AgentDeviceSyncPhase::ComputingDelta, 0, 0, String::new())
        }
        PlannedSyncPhase::Finishing => (AgentDeviceSyncPhase::Finishing, 0, 0, String::new()),
        PlannedSyncPhase::Syncing {
            step,
            current_track,
            bytes_done,
            bytes_total,
            ..
        } => (
            match step {
                SyncStep::Removing => AgentDeviceSyncPhase::Removing,
                SyncStep::Transcoding => AgentDeviceSyncPhase::Transcoding,
                SyncStep::Copying => AgentDeviceSyncPhase::Copying,
                SyncStep::WritingPlaylists => AgentDeviceSyncPhase::WritingPlaylists,
            },
            bytes_done,
            bytes_total,
            current_track,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_free_agent_warnings_are_unique_by_category() {
        assert_eq!(
            agent_warnings(vec![
                SyncPageWarning::UnavailableNotOnDevice { track_id: 1 },
                SyncPageWarning::UnavailableNotOnDevice { track_id: 2 },
                SyncPageWarning::UnsafeManagedItem,
                SyncPageWarning::UnsafeManagedItem,
            ]),
            vec![
                AgentDeviceSyncWarning::UnavailableNotOnDevice,
                AgentDeviceSyncWarning::UnsafeManagedItem,
            ]
        );
    }
}
