//! Main-window binding for the music table's shared header interactions.

use std::rc::Rc;

use gtk4::gio::prelude::*;
use libadwaita::prelude::*;

use crate::ui::track_list::TrackList;

pub(super) fn install(track_list: &Rc<TrackList>) {
    let model = crate::ui::column_layout::model(track_list);
    crate::ui::table_columns::header_popover::install_header_popover(
        track_list.column_view_widget(),
        &model,
    );
    crate::ui::table_columns::header_dnd::install_header_drag(
        track_list.column_view_widget(),
        &model,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn active_table(
    window: &libadwaita::ApplicationWindow,
    content_stack: &gtk4::Stack,
    content_navigation: &libadwaita::NavigationView,
    track_list: &Rc<TrackList>,
    concerts: &Rc<crate::ui::concerts::ConcertsView>,
    releases: &Rc<crate::ui::releases::ReleasesView>,
    radio: &Rc<crate::ui::radio::RadioView>,
) -> Rc<crate::ui::primary_menu::ActiveTable> {
    let active = Rc::new(crate::ui::primary_menu::ActiveTable::default());
    let music = crate::ui::column_layout::model(track_list);
    let concerts = concerts.column_model();
    let releases = releases.column_model();
    let radio = radio.column_model();
    let update: Rc<dyn Fn()> = {
        let window = window.downgrade();
        let content_stack = content_stack.downgrade();
        let content_navigation = content_navigation.downgrade();
        let active = Rc::downgrade(&active);
        Rc::new(move || {
            let (Some(window), Some(stack), Some(navigation), Some(active)) = (
                window.upgrade(),
                content_stack.upgrade(),
                content_navigation.upgrade(),
                active.upgrade(),
            ) else {
                return;
            };
            let page = stack.visible_child_name().unwrap_or_default();
            let root_visible = navigation
                .visible_page()
                .and_then(|page| page.tag())
                .as_deref()
                == Some(super::now_playing_wiring::LIBRARY_CONTENT_TAG);
            set_active(
                &window,
                &active,
                model_for_state(&page, root_visible, &music, &concerts, &releases, &radio),
            );
        })
    };
    {
        let update = update.clone();
        content_stack.connect_visible_child_name_notify(move |_| update());
    }
    {
        let update = update.clone();
        content_navigation.connect_visible_page_notify(move |_| update());
    }
    update();
    active
}

fn model_for_state(
    page: &str,
    navigation_root_visible: bool,
    music: &Rc<dyn crate::ui::table_columns::EditorModel>,
    concerts: &Rc<dyn crate::ui::table_columns::EditorModel>,
    releases: &Rc<dyn crate::ui::table_columns::EditorModel>,
    radio: &Rc<dyn crate::ui::table_columns::EditorModel>,
) -> Option<Rc<dyn crate::ui::table_columns::EditorModel>> {
    if !navigation_root_visible {
        return None;
    }
    model_for_page(page, music, concerts, releases, radio)
}

fn model_for_page(
    page: &str,
    music: &Rc<dyn crate::ui::table_columns::EditorModel>,
    concerts: &Rc<dyn crate::ui::table_columns::EditorModel>,
    releases: &Rc<dyn crate::ui::table_columns::EditorModel>,
    radio: &Rc<dyn crate::ui::table_columns::EditorModel>,
) -> Option<Rc<dyn crate::ui::table_columns::EditorModel>> {
    match page {
        "library" => Some(music.clone()),
        "concerts" => Some(concerts.clone()),
        "releases" => Some(releases.clone()),
        "radio" => Some(radio.clone()),
        _ => None,
    }
}

fn set_active(
    window: &libadwaita::ApplicationWindow,
    active: &crate::ui::primary_menu::ActiveTable,
    model: Option<Rc<dyn crate::ui::table_columns::EditorModel>>,
) {
    let enabled = model.is_some();
    active.set(model);
    if let Some(action) = window
        .lookup_action(crate::ui::primary_menu::ACTION_EDIT_COLUMN_LAYOUT)
        .and_downcast::<gtk4::gio::SimpleAction>()
    {
        action.set_enabled(enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;

    impl crate::ui::table_columns::EditorModel for Fake {
        fn title(&self) -> String {
            String::new()
        }

        fn columns(&self) -> Vec<crate::ui::table_columns::ColumnDescriptor> {
            Vec::new()
        }

        fn is_visible(&self, _id: &str) -> bool {
            false
        }

        fn set_visible(&self, _id: &str, _visible: bool) {}

        fn move_column(&self, _id: &str, _target: &str, _after: bool) {}

        fn reset(&self) {}
    }

    #[test]
    fn style_10_only_table_pages_resolve_an_editor_model() {
        let model: Rc<dyn crate::ui::table_columns::EditorModel> = Rc::new(Fake);
        for page in ["library", "concerts", "releases", "radio"] {
            assert!(model_for_page(page, &model, &model, &model, &model).is_some());
        }
        for page in [
            "stats",
            "podcasts",
            "youtube",
            "library-doctor",
            "device-sync",
        ] {
            assert!(model_for_page(page, &model, &model, &model, &model).is_none());
        }
        assert!(model_for_state("library", false, &model, &model, &model, &model).is_none());
    }
}
