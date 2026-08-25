use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings::{self, PlayerBarPosition};

use super::preference_layout_preview::{self, LayoutPreview, LayoutPreviewState};
use super::preference_visual_strings as visual_strings;
use super::strings;
use super::{action_row, PreferencesContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibraryWindowControl {
    Sidebar,
    BrowseBar,
    InfoPanel,
    StatusLine,
}

fn library_window_controls() -> [LibraryWindowControl; 4] {
    [
        LibraryWindowControl::Sidebar,
        LibraryWindowControl::BrowseBar,
        LibraryWindowControl::InfoPanel,
        LibraryWindowControl::StatusLine,
    ]
}

fn control_title(control: LibraryWindowControl) -> String {
    let message = match control {
        LibraryWindowControl::Sidebar => visual_strings::NAVIGATION_SIDEBAR,
        LibraryWindowControl::BrowseBar => visual_strings::FILTER_BAR,
        LibraryWindowControl::InfoPanel => visual_strings::DETAILS_SIDEBAR,
        LibraryWindowControl::StatusLine => visual_strings::STATUS_BAR,
    };
    visual_strings::text(message)
}

/// Where the region sits in the window — the subtitle says it, so the switch
/// row and the preview above it describe the same thing.
fn control_subtitle(control: LibraryWindowControl) -> String {
    let message = match control {
        LibraryWindowControl::Sidebar => visual_strings::NAVIGATION_SIDEBAR_EDGE,
        LibraryWindowControl::BrowseBar => visual_strings::FILTER_BAR_EDGE,
        LibraryWindowControl::InfoPanel => visual_strings::DETAILS_SIDEBAR_EDGE,
        LibraryWindowControl::StatusLine => visual_strings::STATUS_BAR_EDGE,
    };
    visual_strings::text(message)
}

fn control_save_failure(control: LibraryWindowControl) -> &'static str {
    match control {
        LibraryWindowControl::Sidebar => visual_strings::SIDEBAR_VISIBILITY_SAVE_FAILED,
        LibraryWindowControl::BrowseBar => visual_strings::FILTER_VISIBILITY_SAVE_FAILED,
        LibraryWindowControl::InfoPanel => visual_strings::INFORMATION_VISIBILITY_SAVE_FAILED,
        LibraryWindowControl::StatusLine => visual_strings::STATUS_VISIBILITY_SAVE_FAILED,
    }
}

fn control_visible(state: LayoutPreviewState, control: LibraryWindowControl) -> bool {
    match control {
        LibraryWindowControl::Sidebar => state.sidebar,
        LibraryWindowControl::BrowseBar => state.browse,
        LibraryWindowControl::InfoPanel => state.info,
        LibraryWindowControl::StatusLine => state.status,
    }
}

fn with_control(
    state: LayoutPreviewState,
    control: LibraryWindowControl,
    visible: bool,
) -> LayoutPreviewState {
    match control {
        LibraryWindowControl::Sidebar => LayoutPreviewState {
            sidebar: visible,
            ..state
        },
        LibraryWindowControl::BrowseBar => LayoutPreviewState {
            browse: visible,
            ..state
        },
        LibraryWindowControl::InfoPanel => LayoutPreviewState {
            info: visible,
            ..state
        },
        LibraryWindowControl::StatusLine => LayoutPreviewState {
            status: visible,
            ..state
        },
    }
}

/// Every layout write ends in the same store, so both appliers name it once.
type SaveResult = Result<(), rusqlite::Error>;

fn apply_window_control(
    context: &PreferencesContext,
    control: LibraryWindowControl,
    active: bool,
) -> SaveResult {
    {
        let conn = &context.conn;
        match control {
            LibraryWindowControl::Sidebar => settings::set_sidebar_visible(conn, active),
            LibraryWindowControl::BrowseBar => settings::set_browse_visible(conn, active),
            LibraryWindowControl::InfoPanel => settings::set_info_panel_visible(conn, active),
            LibraryWindowControl::StatusLine => settings::set_status_visible(conn, active),
        }
    }?;
    match control {
        LibraryWindowControl::Sidebar => super::window_navigation::apply_sidebar_visibility(
            &context.split_view,
            &context.sidebar_page,
            active,
        ),
        LibraryWindowControl::BrowseBar => context.track_list.set_browse_visible(active),
        LibraryWindowControl::InfoPanel => {
            context.info_panel.apply_persisted_visibility(active);
        }
        LibraryWindowControl::StatusLine => {
            context.status_bar.set_enabled(active);
            if active {
                context.track_list.reload();
            }
        }
    }
    Ok(())
}

fn apply_bar_position(context: &PreferencesContext, position: PlayerBarPosition) -> SaveResult {
    {
        let conn = &context.conn;
        settings::set_player_bar_position(conn, position)
    }?;
    context.library_player_bar.set_position(position);
    Ok(())
}

/// One saveable step. Splitting the request into steps keeps the rollback
/// rule — whatever fails keeps its previous value — testable without a
/// database behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutChange {
    Bar(PlayerBarPosition),
    Region(LibraryWindowControl, bool),
}

fn pending_changes(
    previous: LayoutPreviewState,
    requested: LayoutPreviewState,
) -> Vec<LayoutChange> {
    let bar = (requested.bar != previous.bar).then_some(LayoutChange::Bar(requested.bar));
    let regions = library_window_controls()
        .into_iter()
        .filter_map(move |control| {
            let wanted = control_visible(requested, control);
            (wanted != control_visible(previous, control))
                .then_some(LayoutChange::Region(control, wanted))
        });
    bar.into_iter().chain(regions).collect()
}

/// Applies every pending step through `save`. A step `save` rejects keeps its
/// previous value, so preview and switches can never drift apart from what is
/// stored.
fn commit_changes(
    previous: LayoutPreviewState,
    requested: LayoutPreviewState,
    save: &dyn Fn(LayoutChange) -> bool,
) -> LayoutPreviewState {
    pending_changes(previous, requested)
        .into_iter()
        .fold(previous, |committed, change| {
            if !save(change) {
                return committed;
            }
            match change {
                LayoutChange::Bar(position) => LayoutPreviewState {
                    bar: position,
                    ..committed
                },
                LayoutChange::Region(control, visible) => with_control(committed, control, visible),
            }
        })
}

/// The one place a layout change reaches the database and the live window.
fn commit(
    context: &PreferencesContext,
    previous: LayoutPreviewState,
    requested: LayoutPreviewState,
) -> LayoutPreviewState {
    commit_changes(previous, requested, &|change| match change {
        LayoutChange::Bar(position) => match apply_bar_position(context, position) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "could not save player bar position");
                context.track_list.toast(&visual_strings::text(
                    visual_strings::PLAYER_BAR_POSITION_SAVE_FAILED,
                ));
                false
            }
        },
        LayoutChange::Region(control, visible) => {
            match apply_window_control(context, control, visible) {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(%error, ?control, "could not save library window control");
                    context
                        .track_list
                        .toast(&visual_strings::text(control_save_failure(control)));
                    false
                }
            }
        }
    })
}

fn state_from_settings(context: &PreferencesContext) -> LayoutPreviewState {
    let conn = &context.conn;
    LayoutPreviewState {
        bar: settings::get_player_bar_position(conn),
        sidebar: settings::get_sidebar_visible(conn),
        browse: settings::get_browse_visible(conn),
        info: settings::get_info_panel_visible(conn),
        status: settings::get_status_visible(conn),
    }
}

fn visible_columns_subtitle(context: &PreferencesContext) -> String {
    let layout = context.track_list.current_column_layout();
    layout
        .order
        .into_iter()
        .filter(|id| layout.visible.contains(id))
        .map(super::column_layout::column_label)
        .collect::<Vec<_>>()
        .join(" · ")
}

fn bar_position_from_index(index: u32) -> PlayerBarPosition {
    if index == 0 {
        PlayerBarPosition::Top
    } else {
        PlayerBarPosition::Bottom
    }
}

fn bar_position_index(value: PlayerBarPosition) -> u32 {
    match value {
        PlayerBarPosition::Top => 0,
        PlayerBarPosition::Bottom => 1,
    }
}

fn toggle_group(labels: &[(&str, Option<&str>)], active: u32) -> adw::ToggleGroup {
    let group = adw::ToggleGroup::new();
    group.set_valign(gtk4::Align::Center);
    for (label, icon) in labels {
        let toggle = adw::Toggle::new();
        toggle.set_label(Some(label));
        if let Some(icon) = icon {
            toggle.set_icon_name(Some(icon));
        }
        group.add(toggle);
    }
    group.set_active(active);
    group
}

fn toggle_row(title: &str, group: &adw::ToggleGroup) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    row.add_suffix(group);
    row
}

/// A request for a new layout state, from the preview or from a switch.
type LayoutRequest = Rc<dyn Fn(LayoutPreviewState)>;
/// The preview needs the request handler, and the handler needs the preview:
/// the dispatcher breaks that circle without leaking either.
type LayoutDispatch = Rc<RefCell<Option<LayoutRequest>>>;

fn dispatcher(dispatch: &LayoutDispatch) -> LayoutRequest {
    let dispatch = dispatch.clone();
    Rc::new(move |requested| {
        let handler = dispatch.borrow().clone();
        if let Some(handler) = handler {
            handler(requested);
        }
    })
}

/// Everything on the page that shows layout state. `sync` is the only writer,
/// so a rejected save leaves no half-applied widget behind. The strong handle
/// lives in [`PreferencesContext`], replaced on every dialog open; the request
/// handler holds a weak one, so the page's widgets die with the dialog instead
/// of surviving in a reference cycle.
pub(in crate::ui) struct LayoutControls {
    preview: Rc<LayoutPreview>,
    bar: adw::ToggleGroup,
    rows: Vec<(LibraryWindowControl, adw::SwitchRow)>,
    syncing: Rc<Cell<bool>>,
}

impl LayoutControls {
    fn sync(&self, state: LayoutPreviewState) {
        self.syncing.set(true);
        self.preview.render(state);
        self.bar.set_active(bar_position_index(state.bar));
        for (control, row) in &self.rows {
            row.set_active(control_visible(state, *control));
        }
        self.syncing.set(false);
    }
}

pub(in crate::ui) fn build(context: &Rc<PreferencesContext>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title(strings::text(strings::PREFERENCES_LAYOUT))
        .icon_name("view-grid-symbolic")
        .build();

    let state = Rc::new(Cell::new(state_from_settings(context)));
    let syncing = Rc::new(Cell::new(false));
    let dispatch: LayoutDispatch = Rc::new(RefCell::new(None));
    let preview = LayoutPreview::new(dispatcher(&dispatch));

    let preview_group = adw::PreferencesGroup::builder()
        .title(visual_strings::text(visual_strings::WINDOW_LAYOUT))
        .description(visual_strings::text(visual_strings::LAYOUT_PREVIEW_HINT))
        .build();
    preview_group.add(&preview.root);
    page.add(&preview_group);

    let regions_group = adw::PreferencesGroup::builder()
        .title(visual_strings::text(visual_strings::WINDOW_REGIONS))
        .build();
    let bar = toggle_group(
        &[
            (
                &strings::text(strings::POSITION_TOP),
                Some("go-up-symbolic"),
            ),
            (
                &strings::text(strings::POSITION_BOTTOM),
                Some("go-down-symbolic"),
            ),
        ],
        bar_position_index(state.get().bar),
    );
    regions_group.add(&toggle_row(
        &strings::text(strings::PLAYER_BAR_POSITION),
        &bar,
    ));

    let rows = library_window_controls()
        .into_iter()
        .map(|control| {
            let row = adw::SwitchRow::builder()
                .title(control_title(control))
                .subtitle(control_subtitle(control))
                .active(control_visible(state.get(), control))
                .build();
            regions_group.add(&row);
            (control, row)
        })
        .collect::<Vec<_>>();

    page.add(&regions_group);

    let controls = Rc::new(LayoutControls {
        preview: preview.clone(),
        bar: bar.clone(),
        rows: rows.clone(),
        syncing: syncing.clone(),
    });
    controls.sync(state.get());

    context
        .layout_controls
        .borrow_mut()
        .replace(controls.clone());
    {
        let weak = Rc::downgrade(context);
        let state = state.clone();
        let controls = Rc::downgrade(&controls);
        *dispatch.borrow_mut() = Some(Rc::new(move |requested| {
            let (Some(context), Some(controls)) = (weak.upgrade(), controls.upgrade()) else {
                return;
            };
            let committed = commit(&context, state.get(), requested);
            state.set(committed);
            controls.sync(committed);
        }));
    }

    let request = dispatcher(&dispatch);

    for (control, row) in &rows {
        let control = *control;
        let state = state.clone();
        let syncing = syncing.clone();
        let request = request.clone();
        row.connect_active_notify(move |row| {
            if syncing.get() {
                return;
            }
            request(with_control(state.get(), control, row.is_active()));
        });
    }
    {
        let state = state.clone();
        let syncing = syncing.clone();
        let request = request.clone();
        bar.connect_active_notify(move |group| {
            if syncing.get() {
                return;
            }
            request(LayoutPreviewState {
                bar: bar_position_from_index(group.active()),
                ..state.get()
            });
        });
    }
    let columns_group = adw::PreferencesGroup::builder()
        .title(visual_strings::text(visual_strings::COLUMNS))
        .build();
    let weak = Rc::downgrade(context);
    let columns = action_row(
        strings::EDIT_COLUMN_LAYOUT,
        Rc::new(move || {
            if let Some(context) = weak.upgrade() {
                context.open_column_layout_editor();
            }
        }),
    );
    columns.set_subtitle(&visible_columns_subtitle(context));
    columns_group.add(&columns);
    page.add(&columns_group);

    let restore_group = adw::PreferencesGroup::new();
    let restore = gtk4::Button::builder()
        .label(visual_strings::text(
            visual_strings::RESTORE_LAYOUT_DEFAULTS,
        ))
        .halign(gtk4::Align::Start)
        .build();
    {
        let request = request.clone();
        restore.connect_clicked(move |_| {
            request(LayoutPreviewState::defaults());
        });
    }
    restore_group.add(&restore);
    page.add(&restore_group);
    page
}

pub(in crate::ui) fn css() -> String {
    preference_layout_preview::css()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_on() -> LayoutPreviewState {
        LayoutPreviewState {
            bar: PlayerBarPosition::Bottom,
            sidebar: true,
            browse: true,
            info: true,
            status: true,
        }
    }

    #[test]
    fn library_window_controls_cover_every_visible_region_once() {
        assert_eq!(
            library_window_controls(),
            [
                LibraryWindowControl::Sidebar,
                LibraryWindowControl::BrowseBar,
                LibraryWindowControl::InfoPanel,
                LibraryWindowControl::StatusLine,
            ]
        );
    }

    #[test]
    fn every_control_reads_and_writes_its_own_region_only() {
        for control in library_window_controls() {
            let hidden = with_control(all_on(), control, false);

            assert!(!control_visible(hidden, control));
            assert_eq!(
                library_window_controls()
                    .into_iter()
                    .filter(|other| !control_visible(hidden, *other))
                    .count(),
                1,
                "{control:?} must not touch the other regions"
            );
        }
    }

    #[test]
    fn every_region_row_names_its_edge() {
        for control in library_window_controls() {
            assert!(!control_title(control).is_empty());
            assert!(!control_subtitle(control).is_empty());
        }
        assert_eq!(
            control_title(LibraryWindowControl::Sidebar),
            "Navigation Sidebar"
        );
        assert_eq!(
            control_subtitle(LibraryWindowControl::StatusLine),
            "Below the track list"
        );
    }

    #[test]
    fn a_request_only_saves_what_actually_changed() {
        let requested = LayoutPreviewState {
            bar: PlayerBarPosition::Top,
            status: false,
            ..all_on()
        };

        assert_eq!(
            pending_changes(all_on(), requested),
            vec![
                LayoutChange::Bar(PlayerBarPosition::Top),
                LayoutChange::Region(LibraryWindowControl::StatusLine, false),
            ]
        );
        assert!(pending_changes(all_on(), all_on()).is_empty());
    }

    #[test]
    fn set_16_a_rejected_save_keeps_the_previous_state() {
        let requested = LayoutPreviewState {
            bar: PlayerBarPosition::Top,
            sidebar: false,
            ..all_on()
        };

        assert_eq!(
            commit_changes(all_on(), requested, &|_| false),
            all_on(),
            "a refused save must leave every region where it was"
        );
        assert_eq!(commit_changes(all_on(), requested, &|_| true), requested);
        assert_eq!(
            commit_changes(all_on(), requested, &|change| matches!(
                change,
                LayoutChange::Bar(_)
            )),
            LayoutPreviewState {
                bar: PlayerBarPosition::Top,
                ..all_on()
            },
            "only the step that failed rolls back"
        );
    }

    #[test]
    fn player_bar_toggles_round_trip_top_then_bottom() {
        for (index, value) in [PlayerBarPosition::Top, PlayerBarPosition::Bottom]
            .into_iter()
            .enumerate()
        {
            assert_eq!(bar_position_index(value), index as u32);
            assert_eq!(bar_position_from_index(index as u32), value);
        }
    }
}
