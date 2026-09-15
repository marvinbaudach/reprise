//! Test-only handle publication for the fully composed window layout seam.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

pub(super) struct WindowLayoutTestHandles {
    pub(super) window: adw::ApplicationWindow,
    pub(super) split_view: adw::OverlaySplitView,
    pub(super) sidebar_page: adw::NavigationPage,
    pub(super) sidebar: Rc<crate::ui::sidebar::Sidebar>,
    pub(super) navigation_scroller: gtk4::ScrolledWindow,
    pub(super) activity_slot: gtk4::Box,
    pub(super) issues_listbox: gtk4::ListBox,
    pub(super) pinned_block: gtk4::Widget,
    pub(super) player_bar_shell: crate::ui::library_player_bar::LibraryPlayerBarShell,
    pub(super) player_bar: Option<gtk4::Widget>,
    pub(super) content_nav: adw::NavigationView,
    pub(super) column_view: gtk4::ColumnView,
    pub(super) track_scrolled: gtk4::ScrolledWindow,
}

thread_local! {
    static HANDLES: RefCell<Option<WindowLayoutTestHandles>> = const { RefCell::new(None) };
}

#[allow(clippy::too_many_arguments)]
pub(super) fn publish(
    window: &adw::ApplicationWindow,
    split_view: &adw::OverlaySplitView,
    sidebar_page: &adw::NavigationPage,
    sidebar: &Rc<crate::ui::sidebar::Sidebar>,
    player_bar_shell: &crate::ui::library_player_bar::LibraryPlayerBarShell,
    player_bar: Option<&gtk4::Widget>,
    content_nav: &adw::NavigationView,
    track_list: &Rc<crate::ui::track_list::TrackList>,
) {
    let pinned_block = sidebar
        .widget()
        .last_child()
        .expect("the sidebar ends with its pinned block");
    HANDLES.with(|handles| {
        handles.replace(Some(WindowLayoutTestHandles {
            window: window.clone(),
            split_view: split_view.clone(),
            sidebar_page: sidebar_page.clone(),
            sidebar: sidebar.clone(),
            navigation_scroller: sidebar.navigation_scroller_for_test(),
            activity_slot: sidebar.activity_slot_for_test(),
            issues_listbox: sidebar.shared.issues_listbox.clone(),
            pinned_block,
            player_bar_shell: player_bar_shell.clone(),
            player_bar: player_bar.cloned(),
            content_nav: content_nav.clone(),
            column_view: track_list.shared.column_view.clone(),
            track_scrolled: track_list.shared.scrolled.clone(),
        }));
    });
}

pub(super) fn take() -> Option<WindowLayoutTestHandles> {
    HANDLES.with(|handles| handles.borrow_mut().take())
}
