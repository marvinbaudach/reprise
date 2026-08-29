//! Run-wide work-unit and byte accounting for device synchronization.

use super::MirrorPlan;

/// Monotonic progress shared by every phase of one synchronization run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkLedger {
    done: u32,
    total: u32,
    bytes_done: u64,
    bytes_total: u64,
    unit_bytes_done: u64,
    unit_bytes_total: u64,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::machine::{DeviceSyncMachine, Effect, Event, PlannedSyncPhase};
    use super::super::{
        AnalysisSidecarWrite, DesiredManagedFile, DeviceFileRecord, DevicePlaylistRecord,
        ManagedRemoval, MirrorPlan, PlaylistWrite, SelectionSource, SyncTrack, TransferAction,
    };

    #[test]
    fn progress_is_monotonic_across_every_kind_of_planned_work() {
        let mut plan = MirrorPlan::default();
        plan.copy.push(desired_track());
        plan.analysis_writes.push(AnalysisSidecarWrite {
            track_id: 1,
            device_path: "Reprise/1.reprise-analysis".into(),
            size_bytes: 10,
            existing_size_bytes: None,
        });
        plan.playlist_writes.push(playlist_write(7));
        plan.playlist_removals.push(playlist_record(8));
        plan.remove
            .push(ManagedRemoval::Inventory(existing_track()));
        plan.transfer_bytes = 110;

        let mut machine =
            DeviceSyncMachine::new("serial-1".into(), plan).with_track_metadata_list();
        let mut pending = machine.dispatch(Event::Start);
        let mut phases = vec![machine.phase().clone()];
        while let Some(effect) = pending.pop() {
            let event = answer(&mut machine, &mut phases, effect);
            let Some(event) = event else { break };
            pending = machine.dispatch(event);
            phases.push(machine.phase().clone());
        }

        let progress = phases.iter().filter_map(progress).collect::<Vec<_>>();
        assert!(progress.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        assert!(progress.windows(2).all(|pair| pair[0].2 <= pair[1].2));
        assert!(progress.iter().all(|(_, total, _)| *total == 6));
        assert_eq!(progress.iter().filter(|item| item.2 == 1.0).count(), 1);
    }

    #[test]
    fn a_last_step_failure_keeps_every_previously_verified_playlist() {
        let mut plan = MirrorPlan::default();
        plan.playlist_writes.push(playlist_write(7));
        plan.playlist_writes.push(playlist_write(8));
        let mut machine =
            DeviceSyncMachine::new("serial-1".into(), plan).with_track_metadata_list();

        machine.dispatch(Event::Start);
        assert_eq!(
            machine.dispatch(Event::PartialsCleaned(Ok(()))),
            vec![Effect::WritePlaylist { index: 0 }]
        );
        machine.dispatch(Event::PlaylistWritten(Ok(())));
        machine.dispatch(Event::PlaylistRecorded(Ok(())));
        machine.dispatch(Event::PlaylistWritten(Ok(())));
        assert_eq!(
            machine.dispatch(Event::PlaylistRecorded(Ok(()))),
            vec![Effect::WriteTrackMetadataList]
        );
        assert!(matches!(
            machine
                .dispatch(Event::TrackMetadataListWritten(Err("staging full".into())))
                .as_slice(),
            [Effect::Finished(
                super::super::machine::SyncOutcome::Failed { .. }
            )]
        ));

        assert_eq!(
            machine.verified_sources(),
            vec![SelectionSource::Playlist(7), SelectionSource::Playlist(8)]
        );
    }

    fn answer(
        machine: &mut DeviceSyncMachine,
        phases: &mut Vec<PlannedSyncPhase>,
        effect: Effect,
    ) -> Option<Event> {
        Some(match effect {
            Effect::CleanPartials => Event::PartialsCleaned(Ok(())),
            Effect::CopyTrack { bytes, .. } => {
                machine.dispatch(Event::CopyProgress { copied: bytes / 2 });
                phases.push(machine.phase().clone());
                Event::TrackCopied(Ok(bytes))
            }
            Effect::RecordFile { .. } => Event::FileRecorded(Ok(())),
            Effect::WriteAnalysis { index } => {
                Event::AnalysisWritten(Ok(machine.plan().analysis_writes[index].size_bytes))
            }
            Effect::WritePlaylist { .. } => Event::PlaylistWritten(Ok(())),
            Effect::RecordPlaylist { .. } => Event::PlaylistRecorded(Ok(())),
            Effect::RemovePlaylist { .. } => Event::PlaylistRemoved(Ok(())),
            Effect::ForgetPlaylist { .. } => Event::PlaylistForgotten(Ok(())),
            Effect::RemoveTrack { .. } => Event::TrackRemoved(Ok(())),
            Effect::ForgetFile { .. } => Event::FileForgotten(Ok(())),
            Effect::WriteTrackMetadataList => Event::TrackMetadataListWritten(Ok(())),
            Effect::Finished(_) => return None,
            unexpected => panic!("unexpected effect: {unexpected:?}"),
        })
    }

    fn progress(phase: &PlannedSyncPhase) -> Option<(u32, u32, f64)> {
        match phase {
            PlannedSyncPhase::Syncing {
                done,
                total,
                unit_bytes_done,
                unit_bytes_total,
                ..
            } => Some((
                *done,
                *total,
                (f64::from(*done)
                    + if *unit_bytes_total == 0 {
                        0.0
                    } else {
                        *unit_bytes_done as f64 / *unit_bytes_total as f64
                    })
                    / f64::from(*total),
            )),
            PlannedSyncPhase::Finishing => Some((6, 6, 1.0)),
            _ => None,
        }
    }

    fn desired_track() -> DesiredManagedFile {
        DesiredManagedFile {
            track: SyncTrack {
                id: 1,
                source_path: PathBuf::from("/music/1.flac"),
                original_name: "1.flac".into(),
                title: "Track 1".into(),
                artist: "Artist".into(),
                album: "Album".into(),
                album_artist: "Artist".into(),
                track_number: Some(1),
                duration_ms: 180_000,
                bitrate_kbps: Some(1000),
                size_bytes: 100,
                source_mtime: 0,
            },
            device_path: "Reprise/1.opus".into(),
            target_bytes: 100,
            profile_fingerprint: "fingerprint".into(),
            action: TransferAction::CopyOriginal,
        }
    }

    fn playlist_write(id: i64) -> PlaylistWrite {
        PlaylistWrite {
            source: SelectionSource::Playlist(id),
            source_name: format!("Playlist {id}"),
            device_path: format!("Reprise/Playlist {id}.m3u8"),
            entries: Vec::new(),
            contents: "#EXTM3U\n".into(),
        }
    }

    fn playlist_record(id: i64) -> DevicePlaylistRecord {
        DevicePlaylistRecord {
            device_serial: "serial-1".into(),
            source: SelectionSource::Playlist(id),
            source_name: format!("Playlist {id}"),
            device_path: format!("Reprise/Playlist {id}.m3u8"),
            last_synced_at: None,
        }
    }

    fn existing_track() -> DeviceFileRecord {
        DeviceFileRecord {
            device_serial: "serial-1".into(),
            track_id: 9,
            source_path: "/music/9.flac".into(),
            source_size: 10,
            source_mtime: 0,
            device_path: "Reprise/9.opus".into(),
            device_size: 10,
            profile_fingerprint: "old".into(),
            pinned: false,
        }
    }
}

impl WorkLedger {
    pub fn for_plan(plan: &MirrorPlan, writes_track_metadata_list: bool) -> Self {
        let total = plan
            .copy
            .len()
            .saturating_add(plan.replace.len())
            .saturating_add(plan.analysis_writes.len())
            .saturating_add(plan.playlist_writes.len())
            .saturating_add(plan.playlist_removals.len())
            .saturating_add(plan.remove.len())
            .saturating_add(usize::from(writes_track_metadata_list));
        Self {
            done: 0,
            total: u32::try_from(total).unwrap_or(u32::MAX),
            bytes_done: 0,
            bytes_total: plan.transfer_bytes,
            unit_bytes_done: 0,
            unit_bytes_total: 0,
        }
    }

    pub fn begin_unit(&mut self, bytes_total: u64) {
        self.unit_bytes_done = 0;
        self.unit_bytes_total = bytes_total;
    }

    pub fn observe_unit_bytes(&mut self, bytes: u64) {
        self.unit_bytes_done = self.unit_bytes_done.max(bytes.min(self.unit_bytes_total));
    }

    pub fn complete_unit(&mut self, bytes_written: u64) {
        self.bytes_done = self
            .bytes_done
            .saturating_add(bytes_written)
            .min(self.bytes_total);
        self.done = self.done.saturating_add(1).min(self.total);
        self.unit_bytes_done = 0;
        self.unit_bytes_total = 0;
    }

    pub fn done(&self) -> u32 {
        self.done
    }

    pub fn total(&self) -> u32 {
        self.total
    }

    pub fn bytes_done(&self) -> u64 {
        self.bytes_done
    }

    pub fn bytes_total(&self) -> u64 {
        self.bytes_total
    }

    pub fn unit_bytes_done(&self) -> u64 {
        self.unit_bytes_done
    }

    pub fn unit_bytes_total(&self) -> u64 {
        self.unit_bytes_total
    }
}
