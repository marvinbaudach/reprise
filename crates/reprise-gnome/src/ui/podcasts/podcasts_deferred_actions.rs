#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeferredAction {
    Download(i64),
    LoadMore { subscription_id: i64, end: usize },
}

#[derive(Default)]
pub(super) struct DeferredActions {
    queued: Vec<DeferredAction>,
}

impl DeferredActions {
    pub(super) fn push(&mut self, action: DeferredAction) {
        if !self.queued.contains(&action) {
            self.queued.push(action);
        }
    }

    pub(super) fn drain(&mut self) -> Vec<DeferredAction> {
        std::mem::take(&mut self.queued)
    }

    pub(super) fn prepend(&mut self, mut actions: Vec<DeferredAction>) {
        actions.append(&mut self.queued);
        self.queued = actions;
    }
}

pub(super) fn replay_until_refused(
    actions: &[DeferredAction],
    mut dispatch: impl FnMut(DeferredAction) -> bool,
) -> Vec<DeferredAction> {
    for (index, action) in actions.iter().copied().enumerate() {
        if !dispatch(action) {
            return actions[index..].to_vec();
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_3_c_offline_actions_replay_in_click_order_without_duplicates() {
        let mut actions = DeferredActions::default();
        actions.push(DeferredAction::Download(7));
        actions.push(DeferredAction::LoadMore {
            subscription_id: 3,
            end: 50,
        });
        actions.push(DeferredAction::Download(7));

        assert_eq!(
            actions.drain(),
            vec![
                DeferredAction::Download(7),
                DeferredAction::LoadMore {
                    subscription_id: 3,
                    end: 50,
                },
            ]
        );
    }

    #[test]
    fn net_3_c_reconnect_retains_a_refused_action_and_every_action_after_it() {
        let actions = vec![
            DeferredAction::Download(7),
            DeferredAction::LoadMore {
                subscription_id: 3,
                end: 50,
            },
            DeferredAction::Download(9),
        ];
        let mut attempted = Vec::new();

        let remaining = replay_until_refused(&actions, |action| {
            attempted.push(action);
            action == DeferredAction::Download(7)
        });

        assert_eq!(
            attempted,
            vec![
                DeferredAction::Download(7),
                DeferredAction::LoadMore {
                    subscription_id: 3,
                    end: 50,
                },
            ]
        );
        assert_eq!(
            remaining,
            vec![
                DeferredAction::LoadMore {
                    subscription_id: 3,
                    end: 50,
                },
                DeferredAction::Download(9),
            ]
        );
    }
}
