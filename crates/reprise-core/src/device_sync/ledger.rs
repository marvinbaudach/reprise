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
            .saturating_add(bytes_written.max(self.unit_bytes_done))
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
            .saturating_add(self.unit_bytes_done)
            .min(self.bytes_total)
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

#[cfg(test)]
mod tests {
    use super::super::machine::{DeviceSyncMachine, Effect, Event};
    use super::super::{MirrorPlan, PlaylistWrite, SelectionSource};

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
            vec![Effect::WritePlaylist {
                index: 0,
                omit_relative_paths: Vec::new(),
            }]
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

    fn playlist_write(id: i64) -> PlaylistWrite {
        PlaylistWrite {
            source: SelectionSource::Playlist(id),
            source_name: format!("Playlist {id}"),
            device_path: format!("Reprise/Playlist {id}.m3u8"),
            entries: Vec::new(),
            contents: "#EXTM3U\n".into(),
        }
    }
}
