//! In-place status patches for the YouTube channel detail rows.

use super::{podcasts_groups, YoutubeChannelDetail};

impl YoutubeChannelDetail {
    pub(in crate::ui::podcasts) fn update_played_state(&self, episode_id: i64, played_at: i64) {
        let row = {
            let mut groups = self.groups.borrow_mut();
            let Some(row) = groups
                .iter_mut()
                .flat_map(|group| group.group.episodes.iter_mut())
                .find(|row| row.id == episode_id)
            else {
                return;
            };
            row.played_at = Some(played_at);
            row.position_ms = 0;
            row.clone()
        };
        let widgets = self.download_widgets.borrow().get(&episode_id).cloned();
        if let Some(widgets) = widgets {
            podcasts_groups::update_episode_status(&widgets, &row);
        }
    }

    pub(in crate::ui::podcasts) fn update_position_state(&self, episode_id: i64, position_ms: i64) {
        let (row, display_changed) = {
            let mut groups = self.groups.borrow_mut();
            let Some(row) = groups
                .iter_mut()
                .flat_map(|group| group.group.episodes.iter_mut())
                .find(|row| row.id == episode_id)
            else {
                return;
            };
            let display_changed =
                super::super::podcasts_presentation::update_resume_position(row, position_ms);
            (row.clone(), display_changed)
        };
        if display_changed {
            let widgets = self.download_widgets.borrow().get(&episode_id).cloned();
            if let Some(widgets) = widgets {
                podcasts_groups::update_episode_status(&widgets, &row);
            }
        }
    }
}
