//! One-time widget/state construction for `TrackList`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use super::browse_bar::BrowseBar;
use super::column_layout::{self, ColumnId};
use super::cover_download_worker::CoverDownloadRuntime;
use super::cover_loader::CoverLoader;
use super::import_errors_view::ImportErrorsView;
use super::issues::MissingFilesView;
use super::track_list_activation::wire_activate;
use super::track_list_context_menu;
use super::track_list_dnd_smoke;
use super::track_list_empty_state::build_status_page;
use super::track_list_model::TrackListModel;
use super::track_list_reload::reload;
use super::track_list_smoke::{
    arm_smoke_activate, arm_smoke_filter, arm_smoke_sort_column, arm_smoke_source,
};
use super::track_list_sort::{wire_sort_clicks, SortState};
use super::{
    notify_import_errors_mutated_and_reload, OnActivate, Shared, TrackList, STACK_PAGE_EMPTY,
    STACK_PAGE_IMPORT_ERRORS, STACK_PAGE_LIST, STACK_PAGE_MISSING,
};

pub(in crate::ui) fn build(
    conn: Rc<RefCell<Connection>>,
    on_activate: OnActivate,
    on_reload: impl Fn(&ViewSource, usize, &str, &BrowseFilter) + 'static,
    queue_ids_provider: impl Fn() -> super::queue_sections::QueueViewModel + 'static,
    cover_download: CoverDownloadRuntime,
) -> TrackList {
    let model = TrackListModel::new(conn.clone());
    let selection = gtk4::MultiSelection::new(Some(model.clone()));
    let column_view = gtk4::ColumnView::builder()
        .model(&selection)
        .show_row_separators(false)
        .show_column_separators(false)
        .build();
    super::track_list_header_style::mark(&column_view);

    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&column_view)
        .vexpand(true)
        .hexpand(true)
        .build();

    let empty_page = build_status_page();
    let empty_page_actions = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    empty_page_actions.set_halign(gtk4::Align::Center);
    let retry_library_button =
        gtk4::Button::with_label(&crate::ui::strings::text(crate::ui::strings::RETRY));
    retry_library_button.add_css_class("suggested-action");
    retry_library_button.set_visible(false);
    empty_page_actions.append(&retry_library_button);
    let show_all_button = gtk4::Button::new();
    show_all_button.add_css_class("pill");
    show_all_button.set_halign(gtk4::Align::Center);
    show_all_button.set_action_name(Some("win.clear-all-filters"));
    let import_errors_view = ImportErrorsView::new(conn.clone());
    let missing_files_view = MissingFilesView::new(conn.clone());
    let stack = super::track_list_layout::build_track_content_stack();
    let list_overlay = gtk4::Overlay::new();
    list_overlay.set_child(Some(&scrolled));
    stack.add_named(&empty_page, Some(STACK_PAGE_EMPTY));
    stack.add_named(&list_overlay, Some(STACK_PAGE_LIST));
    stack.add_named(import_errors_view.widget(), Some(STACK_PAGE_IMPORT_ERRORS));
    stack.add_named(missing_files_view.widget(), Some(STACK_PAGE_MISSING));
    stack.set_visible_child_name(STACK_PAGE_EMPTY);

    let browse_bar = BrowseBar::new(conn.clone());
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.append(browse_bar.widget());
    root.append(&stack);

    let cover_loader = CoverLoader::new(cover_download);
    let shared = Rc::new(Shared {
        model,
        selection: selection.clone(),
        column_view: column_view.clone(),
        playing_track_id: Cell::new(None),
        pending_row_activation: Cell::new(None),
        active_reorder_drag_from: Cell::new(None),
        view_state_memory: RefCell::new(std::collections::HashMap::new()),
        conn,
        cover_loader: cover_loader.clone(),
        browse_bar: browse_bar.clone(),
        browse_filter: RefCell::new(BrowseFilter::default()),
        stack,
        empty_page,
        empty_page_actions,
        retry_library_button: retry_library_button.clone(),
        library_root_unavailable: Cell::new(false),
        unavailable_library_root: RefCell::new(None),
        show_all_button,
        empty_scan_widget: RefCell::new(None),
        sort: RefCell::new(SortState::default()),
        restoring_view: Cell::new(false),
        filter: RefCell::new(String::new()),
        source: RefCell::new(ViewSource::default()),
        queue_ids_provider: Box::new(queue_ids_provider),
        queue_sections: RefCell::new(Vec::new()),
        on_activate,
        on_reload: Box::new(on_reload),
        toast_overlay: gtk4::glib::WeakRef::new(),
        window: gtk4::glib::WeakRef::new(),
        menu_actions: gtk4::gio::SimpleActionGroup::new(),
        on_queue_selected: RefCell::new(None),
        on_play_mix: RefCell::new(None),
        on_play_next_selected: RefCell::new(None),
        on_show_missing: RefCell::new(None),
        on_queue_activate: RefCell::new(None),
        on_queue_remove: RefCell::new(None),
        on_queue_move_to_top: RefCell::new(None),
        on_go_to_album: RefCell::new(None),
        on_go_to_artist: RefCell::new(None),
        on_show_missing_files: RefCell::new(None),
        on_playlist_mutated: RefCell::new(None),
        on_queue_reorder: RefCell::new(None),
        on_sidebar_playlist_drop: RefCell::new(None),
        on_sidebar_queue_drop: RefCell::new(None),
        import_errors_view,
        missing_files_view,
        on_rescan_library: RefCell::new(None),
        on_library_mutated: RefCell::new(None),
        on_scan_queue_purge_ids: RefCell::new(None),
        on_tags_mutated: RefCell::new(None),
        tag_write_gate: crate::ui::tag_write_gate::TagWriteGate::default(),
        on_import_errors_mutated: RefCell::new(None),
        player: RefCell::new(std::rc::Weak::new()),
    });

    {
        let shared_weak = Rc::downgrade(&shared);
        retry_library_button.connect_clicked(move |_| {
            let Some(shared) = shared_weak.upgrade() else {
                return;
            };
            let callback = shared.on_rescan_library.borrow().clone();
            if let Some(callback) = callback {
                callback();
            }
        });
    }

    {
        let shared_weak = Rc::downgrade(&shared);
        browse_bar.set_on_changed(move |filter| {
            let Some(shared) = shared_weak.upgrade() else {
                return;
            };
            *shared.browse_filter.borrow_mut() = filter;
            reload(&shared);
        });
    }
    {
        let shared_weak = Rc::downgrade(&shared);
        shared
            .import_errors_view
            .set_on_mutated(move || match shared_weak.upgrade() {
                Some(shared) => notify_import_errors_mutated_and_reload(&shared),
                None => tracing::warn!(
                    "import errors panel: mutated callback fired after track list was dropped"
                ),
            });
    }
    {
        let shared_weak = Rc::downgrade(&shared);
        shared.import_errors_view.set_on_edit_hint(move |path| {
            let Some(shared) = shared_weak.upgrade() else {
                return;
            };
            crate::ui::tag_edit_flow::begin_for_path(&shared, path);
        });
    }
    {
        let shared_weak = Rc::downgrade(&shared);
        shared.missing_files_view.set_on_mutated(move || {
            let Some(shared) = shared_weak.upgrade() else {
                return;
            };
            reload(&shared);
            let callback = shared.on_library_mutated.borrow().clone();
            if let Some(callback) = callback {
                callback(&[]);
            }
        });
    }
    {
        let shared_weak = Rc::downgrade(&shared);
        shared.missing_files_view.set_on_purged(move |ids| {
            let Some(shared) = shared_weak.upgrade() else {
                return;
            };
            let callback = shared.on_library_mutated.borrow().clone();
            if let Some(callback) = callback {
                callback(ids);
            }
        });
    }

    let built_columns = column_layout::build_columns(&column_view, &shared, &cover_loader);
    let title_column = built_columns.title;
    let artist_column = built_columns.artist;
    let column_registry = built_columns.registry;
    super::end_of_results::install(&shared, &list_overlay, &scrolled);
    let initial_sort_column = if column_registry.is_visible(ColumnId::Artist) {
        artist_column.clone()
    } else {
        *shared.sort.borrow_mut() = SortState {
            field: "title".into(),
            dir: "asc".into(),
        };
        title_column.clone()
    };

    wire_sort_clicks(&column_view, &shared);
    column_view.sort_by_column(Some(&initial_sort_column), gtk4::SortType::Ascending);
    wire_activate(&column_view, &shared);
    track_list_context_menu::wire_context_menu_actions(&column_view, &shared);

    reload(&shared);
    arm_smoke_activate(&shared);
    arm_smoke_filter(&shared);
    arm_smoke_source(&shared);
    arm_smoke_sort_column(&column_view, &title_column, &artist_column);
    super::track_list_menu_smoke::arm_smoke_menu_action(&shared);
    super::tag_edit_flow::arm_smoke(&shared);
    super::delete_tracks::arm_smoke(&shared);
    super::browse_bar::arm_smoke(&shared);
    track_list_dnd_smoke::arm_smoke_dnd(&shared);

    TrackList {
        shared,
        root,
        column_registry,
    }
}
