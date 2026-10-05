use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use super::library_shell::{active_content_target, ActiveContentTarget};
use super::track_list::TrackList;

#[derive(Clone)]
pub(in crate::ui) struct ActiveContentFocus {
    content_stack: glib::WeakRef<gtk4::Stack>,
    focus_tracks: Rc<dyn Fn() -> bool>,
}

impl ActiveContentFocus {
    pub(in crate::ui) fn new(content_stack: &gtk4::Stack, track_list: &Rc<TrackList>) -> Self {
        let track_list = Rc::downgrade(track_list);
        let focus_tracks = Rc::new(move || {
            track_list
                .upgrade()
                .is_some_and(|track_list| track_list.focus_visible_content())
        });
        Self::from_focus_action(content_stack, focus_tracks)
    }

    pub(super) fn from_focus_action(
        content_stack: &gtk4::Stack,
        focus_tracks: Rc<dyn Fn() -> bool>,
    ) -> Self {
        Self {
            content_stack: content_stack.downgrade(),
            focus_tracks,
        }
    }

    pub(in crate::ui) fn focus(&self) -> bool {
        let Some(content_stack) = self.content_stack.upgrade() else {
            return false;
        };
        let content_name = content_stack.visible_child_name();
        match active_content_target(content_name.as_deref()) {
            Some(ActiveContentTarget::Tracks) => (self.focus_tracks)(),
            Some(
                ActiveContentTarget::Stats
                | ActiveContentTarget::Concerts
                | ActiveContentTarget::Releases
                | ActiveContentTarget::Podcasts
                | ActiveContentTarget::Youtube
                | ActiveContentTarget::Radio
                | ActiveContentTarget::LibraryDoctor,
            ) => content_stack
                .visible_child()
                .is_some_and(|child| focus_widget_or_descendant(&child)),
            None => false,
        }
    }

    pub(in crate::ui) fn focus_later(&self) {
        let focus = self.clone();
        glib::idle_add_local_once(move || {
            if !focus.focus() {
                tracing::debug!("active content did not take focus");
            }
        });
    }

    pub(in crate::ui) fn focus_later_if_unset(&self, window: &adw::ApplicationWindow) {
        let focus = self.clone();
        let window = window.downgrade();
        glib::idle_add_local_once(move || {
            let Some(window) = window.upgrade() else {
                return;
            };
            if window.is_active()
                && gtk4::prelude::GtkWindowExt::focus(&window).is_none()
                && !focus.focus()
            {
                tracing::debug!("startup content did not take focus");
            }
        });
    }
}

fn focus_widget_or_descendant(widget: &gtk4::Widget) -> bool {
    widget.grab_focus() || widget.child_focus(gtk4::DirectionType::TabForward)
}
