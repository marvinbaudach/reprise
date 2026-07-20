//! One rendering edge for every track, album, and artist metadata link.

use std::rc::Rc;

use reprise_core::browser::navigation::NavigationIntent;

use super::library_shell::{self, ActiveContentFocus};
use crate::ui::nav_history::NavHistory;
use crate::ui::sidebar::Sidebar;
use crate::ui::track_list::TrackList;

#[derive(Clone)]
pub(in crate::ui) struct MetadataNavigator {
    history: Rc<NavHistory>,
    sidebar: Rc<Sidebar>,
    track_list: Rc<TrackList>,
    content_stack: gtk4::Stack,
    active_content_focus: ActiveContentFocus,
}

impl MetadataNavigator {
    pub(in crate::ui) fn new(
        history: Rc<NavHistory>,
        sidebar: Rc<Sidebar>,
        track_list: Rc<TrackList>,
        content_stack: gtk4::Stack,
        active_content_focus: ActiveContentFocus,
    ) -> Self {
        Self {
            history,
            sidebar,
            track_list,
            content_stack,
            active_content_focus,
        }
    }

    pub(in crate::ui) fn navigate(&self, intent: NavigationIntent, reason: &'static str) {
        let Some(place) = self
            .history
            .navigate_from(intent, self.track_list.browser_place())
        else {
            return;
        };
        library_shell::route_to_place(
            &place,
            &self.sidebar,
            &self.track_list,
            &self.content_stack,
            &self.active_content_focus,
            reason,
        );
    }
}
