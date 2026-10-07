use super::*;

pub(super) fn work_ledger(plan: &MirrorPlan, writes_track_metadata_list: bool) -> WorkLedger {
    let mut accounting = plan.clone();
    accounting
        .analysis_writes
        .extend(plan.lyrics_writes.iter().map(|write| AnalysisSidecarWrite {
            track_id: write.track_id,
            device_path: write.device_path.clone(),
            size_bytes: write.size_bytes,
            existing_size_bytes: write.existing_size_bytes,
        }));
    accounting
        .analysis_writes
        .extend(plan.cue_writes.iter().map(|write| AnalysisSidecarWrite {
            track_id: write.track_id,
            device_path: write.device_path.clone(),
            size_bytes: write.size_bytes,
            existing_size_bytes: write.existing_size_bytes,
        }));
    WorkLedger::for_plan(&accounting, writes_track_metadata_list)
}

impl DeviceSyncMachine {
    /// The phase a run shows before its first step reports anything.
    ///
    /// Partial cleanup runs first but has no step of its own, so the run opens
    /// on whichever step will actually do the first visible work.
    pub(super) fn opening_phase(&self) -> PlannedSyncPhase {
        if self.transfers.is_empty() && self.plan.analysis_writes.is_empty() {
            if let Some(write) = self.plan.cue_writes.first() {
                let mut opening = self.ledger.clone();
                opening.begin_unit(write.size_bytes);
                return phase_transitions::syncing(
                    &opening,
                    SyncStep::Copying,
                    write.device_path.clone(),
                );
            }
            if let Some(write) = self.plan.lyrics_writes.first() {
                let mut opening = self.ledger.clone();
                opening.begin_unit(write.size_bytes);
                return phase_transitions::syncing(
                    &opening,
                    SyncStep::WritingLyrics,
                    write.device_path.clone(),
                );
            }
        }
        phase_transitions::opening(
            &self.transfers,
            &self.plan,
            &self.ledger,
            self.writes_track_metadata_list,
        )
    }

    pub(super) fn enter_analysis_writes(&mut self, from: usize) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        let Some(write) = self.plan.analysis_writes.get(from) else {
            return self.enter_cue_writes(0);
        };
        self.ledger.begin_unit(write.size_bytes);
        self.phase = phase_transitions::syncing(
            &self.ledger,
            SyncStep::WritingAnalysis,
            write.device_path.clone(),
        );
        self.awaiting = Awaiting::WriteAnalysis(from);
        vec![Effect::WriteAnalysis { index: from }]
    }

    pub(super) fn enter_lyrics_writes(&mut self, from: usize) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        let Some(write) = self.plan.lyrics_writes.get(from) else {
            return self.enter_playlists();
        };
        self.ledger.begin_unit(write.size_bytes);
        self.phase = phase_transitions::syncing(
            &self.ledger,
            SyncStep::WritingLyrics,
            write.device_path.clone(),
        );
        self.awaiting = Awaiting::WriteLyrics(from);
        vec![Effect::WriteLyrics { index: from }]
    }
}

impl DeviceSyncMachine {
    /// Records the rows of CUE tracks that share a file another track's row
    /// holds (CUE-15): against the file already on the device, or against the
    /// copy this run made. A track whose file never arrived fails with it.
    pub(super) fn enter_shared_records(&mut self, from: usize) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        for index in from..self.plan.shared_records.len() {
            let record = &self.plan.shared_records[index];
            let placed = match record.resident_size {
                Some(size) => Some((size, record.desired.device_path.clone())),
                None => self.copied_files.get(&record.carrier_track_id).cloned(),
            };
            let Some((device_size, device_path)) = placed else {
                let track_id = record.desired.track.id;
                self.fail_track(track_id);
                continue;
            };
            self.shared_path = Some(device_path.clone());
            self.awaiting = Awaiting::RecordShared(index);
            return vec![Effect::RecordSharedFile {
                index,
                device_size,
                device_path,
            }];
        }
        self.enter_analysis_writes(0)
    }

    /// A recorded CUE track's earlier file, when it had one of its own that
    /// nothing in this plan still names, goes after the run like a replaced one.
    pub(super) fn defer_shared_previous(&mut self, index: usize, recorded_path: &str) {
        let record = &self.plan.shared_records[index];
        let Some(previous) = &record.previous else {
            return;
        };
        let still_named = previous.device_path == recorded_path
            || self
                .plan
                .desired_files
                .iter()
                .any(|file| file.device_path == previous.device_path)
            || self
                .deferred_replacements
                .iter()
                .any(|(path, _)| *path == previous.device_path);
        if !still_named {
            self.deferred_replacements
                .push((previous.device_path.clone(), record.desired.track.id));
        }
    }

    pub(super) fn enter_cue_writes(&mut self, from: usize) -> Vec<Effect> {
        if self.cancelled {
            return self.finish();
        }
        for index in from..self.plan.cue_writes.len() {
            let write = &self.plan.cue_writes[index];
            // A sheet for audio that never arrived would describe nothing.
            if self.absent_device_paths.contains(&write.audio_device_path) {
                self.ledger.complete_unit(0);
                continue;
            }
            self.ledger.begin_unit(write.size_bytes);
            self.phase = phase_transitions::syncing(
                &self.ledger,
                SyncStep::Copying,
                write.device_path.clone(),
            );
            self.awaiting = Awaiting::WriteCue(index);
            return vec![Effect::WriteDerivedCue { index }];
        }
        self.enter_lyrics_writes(0)
    }
}
