//! Accessible sort control for the music browse bar.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::column_layout::{self, ColumnId};
use crate::ui::{filter_bar_layout, strings};

const ACTION_GROUP: &str = "sort";
const FIELD_ACTION: &str = "field";
const DIRECTION_ACTION: &str = "direction";
const FIELD_DETAILED_ACTION: &str = "sort.field";
const DIRECTION_DETAILED_ACTION: &str = "sort.direction";
const ASCENDING: &str = "asc";
const DESCENDING: &str = "desc";

const SORT_FIELDS: [(ColumnId, &str); 10] = [
    (ColumnId::Title, "title"),
    (ColumnId::TrackNumber, "track_no"),
    (ColumnId::Artist, "artist"),
    (ColumnId::Album, "album"),
    (ColumnId::Genre, "genre"),
    (ColumnId::Year, "year"),
    (ColumnId::Added, "added_at"),
    (ColumnId::Duration, "duration_ms"),
    (ColumnId::Rating, "rating"),
    (ColumnId::PlayCount, "play_count"),
];

type OnChanged = Rc<dyn Fn(String, String)>;
type OnOpen = Rc<dyn Fn()>;

pub(in crate::ui) struct BrowseSortMenu {
    button: gtk4::MenuButton,
    field_action: gio::SimpleAction,
    direction_action: gio::SimpleAction,
    on_changed: Rc<RefCell<Option<OnChanged>>>,
    on_open: Rc<RefCell<Option<OnOpen>>>,
}

impl BrowseSortMenu {
    pub(in crate::ui) fn new() -> Rc<Self> {
        let field_action = gio::SimpleAction::new_stateful(
            FIELD_ACTION,
            Some(glib::VariantTy::STRING),
            &"artist".to_variant(),
        );
        let direction_action = gio::SimpleAction::new_stateful(
            DIRECTION_ACTION,
            Some(glib::VariantTy::STRING),
            &ASCENDING.to_variant(),
        );
        let on_changed = Rc::new(RefCell::new(None::<OnChanged>));
        let on_open = Rc::new(RefCell::new(None::<OnOpen>));
        wire_field_action(&field_action, &direction_action, &on_changed);
        wire_direction_action(&field_action, &direction_action, &on_changed);

        let actions = gio::SimpleActionGroup::new();
        actions.add_action(&field_action);
        actions.add_action(&direction_action);

        let button = gtk4::MenuButton::new();
        button.set_child(Some(&gtk4::Label::new(Some(&strings::text(strings::SORT)))));
        button.set_menu_model(Some(&menu_model()));
        button.insert_action_group(ACTION_GROUP, Some(&actions));
        // a11y-semantics: role=button name=sort-tracks state=focusable action=click
        button.set_focusable(true);
        button.add_css_class("pill");
        filter_bar_layout::style_add_filter(&button);
        button.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::SORT_TRACKS,
        ))]);
        {
            let on_open = on_open.clone();
            button.connect_active_notify(move |button| {
                if !button.is_active() {
                    return;
                }
                let callback = on_open.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            });
        }

        Rc::new(Self {
            button,
            field_action,
            direction_action,
            on_changed,
            on_open,
        })
    }

    pub(in crate::ui) fn button(&self) -> &gtk4::MenuButton {
        &self.button
    }

    pub(in crate::ui) fn set_on_changed(&self, callback: impl Fn(String, String) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_open(&self, callback: impl Fn() + 'static) {
        *self.on_open.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn sync(&self, field: &str, direction: &str) {
        self.field_action.set_state(&field.to_variant());
        self.direction_action
            .set_state(&normalized_direction(direction).to_variant());
        self.direction_action.set_enabled(is_sort_field(field));
    }

    #[cfg(test)]
    pub(in crate::ui) fn activate_field(&self, field: &str) {
        self.button
            .activate_action(FIELD_DETAILED_ACTION, Some(&field.to_variant()))
            .expect("the sort field menu action is installed on the real button");
    }

    #[cfg(test)]
    pub(in crate::ui) fn activate_direction(&self, direction: &str) {
        self.button
            .activate_action(DIRECTION_DETAILED_ACTION, Some(&direction.to_variant()))
            .expect("the sort direction menu action is installed on the real button");
    }

    #[cfg(test)]
    pub(in crate::ui) fn state(&self) -> (String, String) {
        (
            string_state(&self.field_action).unwrap(),
            string_state(&self.direction_action).unwrap(),
        )
    }
}

fn wire_field_action(
    field_action: &gio::SimpleAction,
    direction_action: &gio::SimpleAction,
    on_changed: &Rc<RefCell<Option<OnChanged>>>,
) {
    let direction_action = direction_action.clone();
    let on_changed = on_changed.clone();
    field_action.connect_change_state(move |action, value| {
        let Some(field) = value.and_then(glib::Variant::get::<String>) else {
            return;
        };
        if !is_sort_field(&field) {
            return;
        }
        action.set_state(&field.to_variant());
        direction_action.set_enabled(true);
        let direction = string_state(&direction_action).unwrap_or_else(|| ASCENDING.into());
        let callback = on_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(field, direction);
        }
    });
}

fn wire_direction_action(
    field_action: &gio::SimpleAction,
    direction_action: &gio::SimpleAction,
    on_changed: &Rc<RefCell<Option<OnChanged>>>,
) {
    let field_action = field_action.clone();
    let on_changed = on_changed.clone();
    direction_action.connect_change_state(move |action, value| {
        let Some(direction) = value.and_then(glib::Variant::get::<String>) else {
            return;
        };
        if !matches!(direction.as_str(), ASCENDING | DESCENDING) {
            return;
        }
        let Some(field) = string_state(&field_action).filter(|field| is_sort_field(field)) else {
            return;
        };
        action.set_state(&direction.to_variant());
        let callback = on_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(field, direction);
        }
    });
}

fn menu_model() -> gio::Menu {
    let menu = gio::Menu::new();
    let fields = gio::Menu::new();
    for (id, field) in SORT_FIELDS {
        append_targeted(
            &fields,
            &column_layout::column_label(id),
            FIELD_DETAILED_ACTION,
            field,
        );
    }
    menu.append_section(Some(&strings::text(strings::SORT_BY)), &fields);

    let directions = gio::Menu::new();
    append_targeted(
        &directions,
        &strings::text(strings::SORT_ASCENDING),
        DIRECTION_DETAILED_ACTION,
        ASCENDING,
    );
    append_targeted(
        &directions,
        &strings::text(strings::SORT_DESCENDING),
        DIRECTION_DETAILED_ACTION,
        DESCENDING,
    );
    menu.append_section(Some(&strings::text(strings::SORT_DIRECTION)), &directions);
    menu
}

fn append_targeted(menu: &gio::Menu, label: &str, action: &str, target: &str) {
    let item = gio::MenuItem::new(Some(label), None);
    item.set_action_and_target_value(Some(action), Some(&target.to_variant()));
    menu.append_item(&item);
}

fn normalized_direction(direction: &str) -> &'static str {
    if direction == DESCENDING {
        DESCENDING
    } else {
        ASCENDING
    }
}

fn is_sort_field(field: &str) -> bool {
    SORT_FIELDS.iter().any(|(_, candidate)| *candidate == field)
}

fn string_state(action: &gio::SimpleAction) -> Option<String> {
    action.state().and_then(|state| state.get::<String>())
}
