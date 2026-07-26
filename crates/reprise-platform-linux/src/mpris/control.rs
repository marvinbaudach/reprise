//! Reprise-specific local D-Bus controls beyond the standard MPRIS surface.

use zbus::interface;

use reprise_core::media_integration::{
    read_agent_queue_state, MprisCommand, SharedAgentQueueState,
};

pub(super) struct RepriseControl {
    commands: async_channel::Sender<MprisCommand>,
    queue_state: SharedAgentQueueState,
}

impl RepriseControl {
    pub(super) fn new(
        commands: async_channel::Sender<MprisCommand>,
        queue_state: SharedAgentQueueState,
    ) -> Self {
        Self {
            commands,
            queue_state,
        }
    }

    fn dispatch(&self, command: MprisCommand) {
        if let Err(error) = self.commands.try_send(command) {
            let message = error.to_string();
            let command = error.into_inner();
            tracing::warn!(error = %message, ?command, "MPRIS command dropped: controller receiver is gone");
        }
    }
}

#[interface(name = "org.reprise.Player1")]
impl RepriseControl {
    fn play_track_ids(&self, ids: Vec<i64>) {
        if !ids.is_empty() {
            self.dispatch(MprisCommand::PlayTrackIds(ids));
        }
    }

    fn queue_snapshot(&self) -> (i64, Vec<i64>, Vec<i64>, u64, u64) {
        let state = read_agent_queue_state(&self.queue_state);
        (
            state.current_track_id.unwrap_or_default(),
            state.play_next_track_ids,
            state.context_track_ids,
            state.play_next_total as u64,
            state.context_total as u64,
        )
    }

    fn queue_add_next(&self, ids: Vec<i64>) {
        self.dispatch(MprisCommand::QueueAddNext(ids));
    }

    fn queue_add_last(&self, ids: Vec<i64>) {
        self.dispatch(MprisCommand::QueueAddLast(ids));
    }

    fn queue_clear(&self) {
        self.dispatch(MprisCommand::QueueClear);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use reprise_core::media_integration::AgentQueueState;

    use super::*;

    fn control() -> (
        RepriseControl,
        async_channel::Receiver<MprisCommand>,
        SharedAgentQueueState,
    ) {
        let (sender, receiver) = async_channel::unbounded();
        let state = Arc::new(Mutex::new(AgentQueueState::default()));
        (RepriseControl::new(sender, state.clone()), receiver, state)
    }

    #[test]
    fn play_track_ids_dispatches_order_and_empty_is_a_noop() {
        let (control, receiver, _) = control();
        control.play_track_ids(vec![]);
        assert!(receiver.try_recv().is_err());
        control.play_track_ids(vec![3, 1, 2]);
        assert_eq!(
            receiver.try_recv().unwrap(),
            MprisCommand::PlayTrackIds(vec![3, 1, 2])
        );
    }

    #[test]
    fn queue_snapshot_and_mutations_keep_the_wire_contract() {
        let (control, receiver, state) = control();
        *state.lock().unwrap() = AgentQueueState {
            current_track_id: Some(4),
            play_next_track_ids: vec![5, 6],
            context_track_ids: vec![7],
            play_next_total: 2,
            context_total: 1,
        };

        assert_eq!(control.queue_snapshot(), (4, vec![5, 6], vec![7], 2, 1));
        control.queue_add_next(vec![8]);
        control.queue_add_last(vec![9]);
        control.queue_clear();
        assert_eq!(
            receiver.try_recv().unwrap(),
            MprisCommand::QueueAddNext(vec![8])
        );
        assert_eq!(
            receiver.try_recv().unwrap(),
            MprisCommand::QueueAddLast(vec![9])
        );
        assert_eq!(receiver.try_recv().unwrap(), MprisCommand::QueueClear);
    }
}
