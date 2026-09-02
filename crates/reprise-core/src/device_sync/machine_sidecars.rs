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
    WorkLedger::for_plan(&accounting, writes_track_metadata_list)
}

impl DeviceSyncMachine {
    /// The phase a run shows before its first step reports anything.
    ///
    /// Partial cleanup runs first but has no step of its own, so the run opens
    /// on whichever step will actually do the first visible work.
    pub(super) fn opening_phase(&self) -> PlannedSyncPhase {
        if self.transfers.is_empty() && self.plan.analysis_writes.is_empty() {
            if let Some(write) = self.plan.lyrics_writes.first() {
                let mut opening = self.ledger.clone();
                opening.begin_unit(write.size_bytes);
                return phase_transitions::syncing(
                    &opening,
                    SyncStep::WritingAnalysis,
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
            return self.enter_lyrics_writes(0);
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
            SyncStep::WritingAnalysis,
            write.device_path.clone(),
        );
        self.awaiting = Awaiting::WriteLyrics(from);
        vec![Effect::WriteLyrics { index: from }]
    }
}
