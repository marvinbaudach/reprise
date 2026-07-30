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
}
