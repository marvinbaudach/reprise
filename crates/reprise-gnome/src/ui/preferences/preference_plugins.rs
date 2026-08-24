use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::modules::ModuleDescriptor;

use super::preference_online_master::{self as master_chrome, OnlineMaster};
use super::preference_plugin_chrome as chrome;
use super::{strings, PreferencesContext};

pub(in crate::ui) const TARGET_CLASS: &str = "reprise-plugin-target";
pub(in crate::ui) const ONLINE_LYRICS_TARGETS: &[&str] = &["online_lyrics"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PluginGroup {
    Local,
    Online,
    Connected,
}

const LOCAL_PLUGIN_IDS: &[&str] = &["song_visuals"];
const ONLINE_PLUGIN_IDS: &[&str] = &[
    "artwork",
    "online_lyrics",
    "concerts",
    "new_releases",
    "youtube",
    "podcasts",
    "radio",
];
const CONNECTED_PLUGIN_IDS: &[&str] = &["scrobbling"];

/// The online plugins that own a sidebar entry — the ones the master's own
/// description promises to hide, and the ones the paused hint names.
const SIDEBAR_PLUGIN_IDS: &[&str] = &["concerts", "new_releases", "youtube", "podcasts", "radio"];

fn plugin_ids_for_group(group: PluginGroup) -> &'static [&'static str] {
    match group {
        PluginGroup::Local => LOCAL_PLUGIN_IDS,
        PluginGroup::Online => ONLINE_PLUGIN_IDS,
        PluginGroup::Connected => CONNECTED_PLUGIN_IDS,
    }
}

fn plugin_uses_expander(id: &str) -> bool {
    matches!(
        id,
        "youtube" | "podcasts" | "radio" | "new_releases" | "concerts"
    )
}

/// What the online section shows for a given gate state.
///
/// The second draft collapsed the whole list behind a "Show the N sources"
/// disclosure while the gate was off. The third draft keeps every row in place
/// and dims the card instead: the list must stay exactly as compact — and as
/// long — as it was, so that toggling the gate cannot move anything under the
/// reader's eyes.
#[derive(Clone, Debug, PartialEq)]
struct OnlineSectionState {
    badge: String,
    /// Dimmed but readable, never greyed out past legibility.
    card_opacity: f64,
    /// Off is off: the rows are shown, not operated.
    card_interactive: bool,
    hint: Option<String>,
}

fn online_section_state(
    enabled: bool,
    enabled_children: usize,
    total: usize,
    sidebar_names: &[String],
) -> OnlineSectionState {
    OnlineSectionState {
        badge: master_chrome::badge_text(enabled, enabled_children, total),
        card_opacity: if enabled {
            1.0
        } else {
            master_chrome::CHILDREN_OFF_OPACITY
        },
        card_interactive: enabled,
        hint: (!enabled).then(|| master_chrome::paused_hint(total, sidebar_names)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CollapsedGroupState {
    rows_visible: bool,
    rows_sensitive: bool,
    disclosure_visible: bool,
    disclosure_label: String,
}

fn connected_group_state(
    master_enabled: bool,
    revealed_while_disabled: bool,
) -> CollapsedGroupState {
    CollapsedGroupState {
        rows_visible: master_enabled || revealed_while_disabled,
        rows_sensitive: master_enabled,
        disclosure_visible: !master_enabled && !revealed_while_disabled,
        disclosure_label: strings::text(strings::SCROBBLING_NEEDS_ONLINE_SOURCES),
    }
}

fn apply_collapsed_group(
    rows: &[gtk4::Widget],
    disclosure: &adw::ActionRow,
    state: &CollapsedGroupState,
) {
    disclosure.set_title(&state.disclosure_label);
    disclosure.set_visible(state.disclosure_visible);
    for row in rows {
        row.set_visible(state.rows_visible);
        row.set_sensitive(state.rows_sensitive);
    }
}

fn descriptor(id: &str) -> &'static ModuleDescriptor {
    reprise_core::modules::ALL_MODULES
        .iter()
        .copied()
        .find(|descriptor| descriptor.id == id)
        .unwrap_or_else(|| panic!("unknown Plugins capability: {id}"))
}

fn sidebar_plugin_names() -> Vec<String> {
    SIDEBAR_PLUGIN_IDS
        .iter()
        .map(|id| plugin_title(descriptor(id)))
        .collect()
}

pub(in crate::ui) fn highlight_duration() -> std::time::Duration {
    std::time::Duration::from_millis(u64::from(crate::ui::motion::AMBIENT_MS))
}

pub(in crate::ui) fn css() -> String {
    format!(
        ".{TARGET_CLASS}, .{} {{ \
           background-color: alpha(@accent_bg_color, 0.22); \
           box-shadow: inset 3px 0 @accent_color; \
           transition: background-color {}ms {}, box-shadow {}ms {}; }} \
         .{} {{ background-color: alpha(@card_bg_color, 0.55); }} {} {}",
        super::preference_location::LOCATION_TARGET_CLASS,
        crate::ui::motion::MICRO_MS,
        crate::ui::motion::MICRO_CSS_EASING,
        crate::ui::motion::MICRO_MS,
        crate::ui::motion::MICRO_CSS_EASING,
        super::preference_concerts::LOCATION_REFERENCE_CLASS,
        chrome::css(),
        master_chrome::css(),
    )
}

pub(in crate::ui) fn plugin_applies_live(descriptor: &ModuleDescriptor) -> bool {
    descriptor.applies_live
}

pub(in crate::ui) fn plugin_title(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "listenbrainz" => strings::LISTENBRAINZ,
        "lastfm" => strings::LASTFM,
        "new_releases" => strings::NEW_RELEASES,
        "concerts" => strings::CONCERTS,
        "youtube" => strings::YOUTUBE,
        "podcasts" => strings::PODCASTS,
        "radio" => strings::RADIO,
        "library_doctor" => strings::LIBRARY_DOCTOR,
        "artwork" => strings::ARTWORK,
        "online_lyrics" => strings::ONLINE_LYRICS,
        "song_visuals" => strings::SONG_VISUALS,
        _ => return descriptor.name.to_string(),
    };
    strings::text(message)
}

pub(in crate::ui) fn plugin_description(descriptor: &ModuleDescriptor) -> String {
    let message = match descriptor.id {
        "listenbrainz" => strings::PLUGIN_LISTENBRAINZ_DESCRIPTION,
        "lastfm" => strings::PLUGIN_LASTFM_DESCRIPTION,
        "new_releases" => strings::NEW_RELEASES_DESCRIPTION,
        "concerts" => strings::CONCERTS_DESCRIPTION,
        "youtube" => strings::ONLINE_SOURCES_YOUTUBE_SUBTITLE,
        "podcasts" => strings::ONLINE_SOURCES_PODCASTS_SUBTITLE,
        "radio" => strings::ONLINE_SOURCES_RADIO_SUBTITLE,
        "artwork" => strings::ARTWORK_DESCRIPTION,
        "online_lyrics" => strings::ONLINE_LYRICS_DESCRIPTION,
        "song_visuals" => strings::SONG_VISUALS_DESCRIPTION,
        _ => return descriptor.description.to_string(),
    };
    strings::text(message)
}

fn register_plugin_row(
    context: &PreferencesContext,
    descriptor: &'static ModuleDescriptor,
    row: &impl IsA<gtk4::Widget>,
) {
    let target = glib::WeakRef::new();
    target.set(Some(row.upcast_ref::<gtk4::Widget>()));
    context
        .plugin_rows
        .borrow_mut()
        .insert(descriptor.id, target);
}

/// One online plugin's row, reduced to what the section has to drive: what it
/// persists, and how to show a state without persisting it.
struct OnlineChild {
    descriptor: &'static ModuleDescriptor,
    /// The plugin's own, persisted flag — untouched by the master.
    stored: Rc<Cell<bool>>,
    /// Shows a state on the switch without reporting it anywhere.
    show: Rc<dyn Fn(bool)>,
}

/// A guarded switch write: `show` may drive the widget freely without the
/// notify handler mistaking it for a click.
fn guarded_show(syncing: &Rc<Cell<bool>>, set_active: impl Fn(bool) + 'static) -> Rc<dyn Fn(bool)> {
    let syncing = syncing.clone();
    Rc::new(move |active| {
        syncing.set(true);
        set_active(active);
        syncing.set(false);
    })
}

fn switch_alignment_placeholder() -> gtk4::Image {
    chrome::switch_alignment_placeholder()
}

fn aligned_switch_row(title: &str, subtitle: &str, active: bool) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder()
        .title(title)
        .subtitle(subtitle)
        .use_markup(false)
        .active(active)
        .build();
    row.add_suffix(&switch_alignment_placeholder());
    row
}

fn simple_plugin_row(
    context: &Rc<PreferencesContext>,
    descriptor: &'static ModuleDescriptor,
    on_child_changed: &Rc<dyn Fn()>,
) -> (adw::SwitchRow, OnlineChild) {
    let description = plugin_description(descriptor);
    let subtitle = if plugin_applies_live(descriptor) {
        description
    } else {
        format!(
            "{} · {}",
            description,
            strings::text(strings::RESTART_REQUIRED)
        )
    };
    let active = reprise_core::modules::is_enabled(&context.conn, descriptor)
        .unwrap_or(descriptor.default_enabled);
    let row = aligned_switch_row(&plugin_title(descriptor), &subtitle, active);
    register_plugin_row(context, descriptor, &row);

    let syncing = Rc::new(Cell::new(false));
    let stored = Rc::new(Cell::new(active));
    let show = guarded_show(&syncing, {
        let row = row.downgrade();
        move |active| {
            if let Some(row) = row.upgrade() {
                row.set_active(active);
            }
        }
    });
    {
        let weak = Rc::downgrade(context);
        let syncing = syncing.clone();
        let stored = stored.clone();
        let on_child_changed = on_child_changed.clone();
        row.connect_active_notify(move |row| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            if syncing.get() {
                return;
            }
            let active = row.is_active();
            syncing.set(true);
            let result = context.set_module_enabled(descriptor, active, "plugin toggled");
            syncing.set(false);
            match result {
                Ok(()) => {
                    stored.set(active);
                    on_child_changed();
                }
                Err(error) => {
                    tracing::warn!(%error, module = descriptor.id, "could not save plugin state");
                    syncing.set(true);
                    row.set_active(!active);
                    syncing.set(false);
                }
            }
        });
    }
    (
        row,
        OnlineChild {
            descriptor,
            stored,
            show,
        },
    )
}

/// An expandable plugin row.
///
/// The switch is a suffix widget, **not** libadwaita's `enable-expansion`.
/// `adw_expander_row_set_enable_expansion` forces `expanded` to the same value,
/// which is exactly how switching the gate on used to unfold every plugin's
/// settings at once. With the switch decoupled, visibility and function are two
/// separate axes: the chevron and the title area open a row, the switch runs it,
/// and neither ever moves the other.
fn settings_plugin_row(
    context: &Rc<PreferencesContext>,
    descriptor: &'static ModuleDescriptor,
    on_child_changed: &Rc<dyn Fn()>,
) -> (adw::ExpanderRow, OnlineChild) {
    let active = reprise_core::modules::is_enabled(&context.conn, descriptor)
        .unwrap_or(descriptor.default_enabled);
    let row = adw::ExpanderRow::builder()
        .title(plugin_title(descriptor))
        .subtitle(plugin_description(descriptor))
        .build();
    chrome::mark_expander(&row);
    let toggle = gtk4::Switch::builder()
        .active(active)
        .valign(gtk4::Align::Center)
        .build();
    toggle.update_property(&[gtk4::accessible::Property::Label(&plugin_title(descriptor))]);
    row.add_suffix(&toggle);

    let set_children_sensitive: Rc<dyn Fn(bool)> = match descriptor.id {
        "youtube" => {
            let rows = super::preference_youtube::build(&context.conn, active);
            rows.add_to(&row);
            Rc::new(move |enabled| rows.set_sensitive(enabled))
        }
        "podcasts" => {
            let rows = super::preference_podcasts::build(&context.conn, active);
            rows.add_to(&row);
            Rc::new(move |enabled| rows.set_sensitive(enabled))
        }
        "radio" => {
            let rows = super::preference_radio::build(&context.conn, active);
            rows.add_to(&row);
            Rc::new(move |enabled| rows.set_sensitive(enabled))
        }
        "new_releases" => {
            let rows =
                super::preference_new_releases::build(&context.conn, &context.artist_news, active);
            rows.add_to(&row);
            Rc::new(move |enabled| rows.set_sensitive(enabled))
        }
        "concerts" => {
            let weak = Rc::downgrade(context);
            let on_location: Rc<dyn Fn()> = Rc::new(move || {
                if let Some(context) = weak.upgrade() {
                    context.present_location_settings();
                }
            });
            let rows = super::preference_concerts::build(
                &context.conn,
                &context.concerts,
                &context.location_broadcast,
                &on_location,
                active,
            );
            rows.add_to(&row);
            Rc::new(move |enabled| rows.set_sensitive(enabled))
        }
        id => panic!("capability {id} has no Plugins child-row builder"),
    };
    register_plugin_row(context, descriptor, &row);

    let syncing = Rc::new(Cell::new(false));
    let stored = Rc::new(Cell::new(active));
    let show = guarded_show(&syncing, {
        let toggle = toggle.downgrade();
        move |active| {
            if let Some(toggle) = toggle.upgrade() {
                toggle.set_active(active);
            }
        }
    });
    {
        let weak = Rc::downgrade(context);
        let syncing = syncing.clone();
        let stored = stored.clone();
        let on_child_changed = on_child_changed.clone();
        let set_children_sensitive = set_children_sensitive.clone();
        toggle.connect_active_notify(move |toggle| {
            let Some(context) = weak.upgrade() else {
                return;
            };
            if syncing.get() {
                return;
            }
            let active = toggle.is_active();
            syncing.set(true);
            let result = context.set_module_enabled(descriptor, active, "plugin toggled");
            syncing.set(false);
            match result {
                Ok(()) => {
                    stored.set(active);
                    set_children_sensitive(active);
                    on_child_changed();
                }
                Err(error) => {
                    tracing::warn!(%error, module = descriptor.id, "could not save plugin state");
                    syncing.set(true);
                    toggle.set_active(!active);
                    syncing.set(false);
                }
            }
        });
    }
    subscribe_runtime_state(context, descriptor, &show);
    (
        row,
        OnlineChild {
            descriptor,
            stored,
            show,
        },
    )
}

/// Some runtimes publish their own enabled state (it is recomputed against the
/// global gate). Mirroring it onto the switch keeps the row honest without ever
/// writing anything back — and without touching the row's expansion.
fn subscribe_runtime_state(
    context: &Rc<PreferencesContext>,
    descriptor: &'static ModuleDescriptor,
    show: &Rc<dyn Fn(bool)>,
) {
    let alive = Rc::downgrade(show);
    match descriptor.id {
        "new_releases" => {
            let show = show.clone();
            context.artist_news.subscribe_enabled(
                move || alive.strong_count() > 0,
                move |enabled| show(enabled),
            );
        }
        "concerts" => {
            let show = show.clone();
            context.concerts.subscribe_enabled(
                move || alive.strong_count() > 0,
                move |enabled| show(enabled),
            );
        }
        _ => {}
    }
}

/// The online section's widgets, kept together so one place applies one state.
///
/// Owned by [`PreferencesContext`], not by the widgets: the master's own
/// callback has to reach the section, and holding it strongly from there would
/// tie the section to a widget that the section owns.
pub(in crate::ui) struct OnlineSection {
    conn: Rc<reprise_core::db::Db>,
    master: OnlineMaster,
    card: adw::PreferencesGroup,
    rail: gtk4::Box,
    hint: gtk4::Label,
    children: Rc<Vec<OnlineChild>>,
}

impl OnlineSection {
    /// Re-reads every child's persisted flag. The store is the authority, not
    /// the widgets: a first enable seeds modules that were never decided, and a
    /// sidebar menu can write a flag while this page is open — either way the
    /// badge has to count what is actually stored.
    fn reload_stored(&self) {
        for child in self.children.iter() {
            let stored = reprise_core::modules::is_enabled(&self.conn, child.descriptor)
                .unwrap_or(child.descriptor.default_enabled);
            child.stored.set(stored);
        }
    }

    fn apply(&self, enabled: bool) {
        self.reload_stored();
        let enabled_children = self
            .children
            .iter()
            .filter(|child| child.stored.get())
            .count();
        let state = online_section_state(
            enabled,
            enabled_children,
            self.children.len(),
            &sidebar_plugin_names(),
        );
        self.master
            .set_badge(enabled, enabled_children, self.children.len());
        self.card.set_opacity(state.card_opacity);
        // Not `sensitive`: that greys the card past legibility, and the draft
        // asks for the rows to stay readable. Blocking targeting and focus stops
        // them being operated while the gate is off, which is all that is meant.
        self.card.set_can_target(state.card_interactive);
        self.card.set_can_focus(state.card_interactive);
        if enabled {
            self.rail.remove_css_class(master_chrome::RAIL_OFF_CLASS);
        } else {
            self.rail.add_css_class(master_chrome::RAIL_OFF_CLASS);
        }
        match &state.hint {
            Some(text) => {
                self.hint.set_label(text);
                self.hint.set_visible(true);
            }
            None => self.hint.set_visible(false),
        }
        // Every child shows the state it *effectively* has. Nothing is written:
        // the stored flags are the user's and the master never overwrites them.
        for child in self.children.iter() {
            (child.show)(enabled && child.stored.get());
        }
    }
}

impl PreferencesContext {
    pub(in crate::ui) fn plugins_page(self: &Rc<Self>) -> adw::PreferencesPage {
        // TODO(package-B): remove these superseded catalog entries from the
        // shared string module it owns.
        let _legacy_group_catalog_entries = (
            strings::LOCAL_FEATURES,
            strings::ONLINE_CONTENT,
            strings::CONNECTED_SERVICES,
        );
        let page = adw::PreferencesPage::builder()
            .title(strings::text(strings::PREFERENCES_PLUGINS))
            .icon_name("application-x-addon-symbolic")
            .build();
        page.add_css_class(chrome::PLUGINS_PAGE_CLASS);

        let local_group = adw::PreferencesGroup::builder()
            .title(strings::text(strings::PLUGIN_GROUP_LOCAL))
            .build();
        let no_op: Rc<dyn Fn()> = Rc::new(|| {});
        for id in plugin_ids_for_group(PluginGroup::Local) {
            let descriptor = descriptor(id);
            if plugin_uses_expander(id) {
                let (row, _) = settings_plugin_row(self, descriptor, &no_op);
                local_group.add(&row);
            } else {
                let (row, _) = simple_plugin_row(self, descriptor, &no_op);
                local_group.add(&row);
            }
        }

        let global_enabled = reprise_core::online_sources::is_enabled(&self.conn).unwrap_or(true);
        let master = OnlineMaster::new(global_enabled);
        // The master leaves the card list and becomes the bracket over it: its
        // own group carries no rows and no title, only the heading widget.
        let master_group = adw::PreferencesGroup::new();
        master_group.add(master.widget());

        // The children: one card, five (here: seven) rows, indented behind a
        // rail. The indent plus the rail say who obeys whom.
        let children_group = adw::PreferencesGroup::new();
        let bracket = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let rail = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        rail.add_css_class(master_chrome::RAIL_CLASS);
        rail.set_valign(gtk4::Align::Fill);
        rail.set_vexpand(true);
        let card = adw::PreferencesGroup::new();
        card.add_css_class(master_chrome::CHILDREN_CLASS);
        card.set_hexpand(true);
        bracket.append(&rail);
        bracket.append(&card);
        children_group.add(&bracket);

        let hint = gtk4::Label::new(None);
        hint.set_xalign(0.0);
        hint.set_wrap(true);
        hint.add_css_class(master_chrome::PAUSED_HINT_CLASS);
        hint.set_visible(false);
        children_group.add(&hint);

        let on_child_changed: Rc<dyn Fn()> = {
            let weak = Rc::downgrade(self);
            Rc::new(move || {
                let Some(context) = weak.upgrade() else {
                    return;
                };
                let section = context.online_section.borrow().clone();
                if let Some(section) = section {
                    section.apply(section.master.is_active());
                }
            })
        };

        let mut children = Vec::with_capacity(ONLINE_PLUGIN_IDS.len());
        for id in plugin_ids_for_group(PluginGroup::Online) {
            let descriptor = descriptor(id);
            let child = if plugin_uses_expander(id) {
                let (row, child) = settings_plugin_row(self, descriptor, &on_child_changed);
                card.add(&row);
                child
            } else {
                let (row, child) = simple_plugin_row(self, descriptor, &on_child_changed);
                card.add(&row);
                child
            };
            children.push(child);
        }

        let connected_group = adw::PreferencesGroup::builder()
            .title(strings::text(strings::PLUGIN_GROUP_CONNECTED_SERVICES))
            .build();
        let connected_disclosure = adw::ActionRow::builder()
            .title(strings::text(strings::SCROBBLING_NEEDS_ONLINE_SOURCES))
            .activatable(true)
            .build();
        connected_disclosure.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
        connected_group.add(&connected_disclosure);
        let connected_rows = plugin_ids_for_group(PluginGroup::Connected)
            .iter()
            .map(|id| match *id {
                "scrobbling" => {
                    let row = super::preference_scrobbling::build(self);
                    row.upcast::<gtk4::Widget>()
                }
                id => panic!("connected capability {id} has no Plugins row builder"),
            })
            .collect::<Vec<_>>();
        for row in &connected_rows {
            connected_group.add(row);
        }
        let connected_rows = Rc::new(connected_rows);
        apply_collapsed_group(
            &connected_rows,
            &connected_disclosure,
            &connected_group_state(global_enabled, false),
        );
        {
            let rows = connected_rows.clone();
            let disclosure = connected_disclosure.clone();
            connected_disclosure.connect_activated(move |_| {
                apply_collapsed_group(&rows, &disclosure, &connected_group_state(false, true));
            });
        }

        let online = Rc::new(OnlineSection {
            conn: self.conn.clone(),
            master: master.clone(),
            card,
            rail,
            hint,
            children: Rc::new(children),
        });
        self.online_section.replace(Some(online.clone()));
        online.apply(global_enabled);

        {
            let weak = Rc::downgrade(self);
            let connected_rows = connected_rows.clone();
            let connected_disclosure = connected_disclosure.clone();
            master.set_on_toggled(move |enabled| {
                let Some(context) = weak.upgrade() else {
                    return;
                };
                let Some(online) = context.online_section.borrow().clone() else {
                    return;
                };
                if let Err(error) = context.set_online_sources_enabled(enabled) {
                    tracing::warn!(%error, "could not save the online-sources gate");
                    online.master.set_active_silently(!enabled);
                    return;
                }
                // Nothing here reaches an expansion state, and no row changes
                // its visibility: the list keeps exactly the height it had, so
                // whatever was in view stays in view.
                online.apply(enabled);
                apply_collapsed_group(
                    &connected_rows,
                    &connected_disclosure,
                    &connected_group_state(enabled, false),
                );
            });
        }

        page.add(&local_group);
        page.add(&master_group);
        page.add(&children_group);
        page.add(&connected_group);
        page
    }

    pub(in crate::ui) fn highlight_pending_plugin_rows(&self) {
        let targets = std::mem::take(&mut *self.pending_plugin_targets.borrow_mut());
        let rows = targets
            .iter()
            .filter_map(|target| {
                self.plugin_rows
                    .borrow()
                    .get(target)
                    .and_then(glib::WeakRef::upgrade)
            })
            .collect::<Vec<_>>();
        if let Some(first) = rows.first() {
            first.grab_focus();
        }
        for row in rows {
            // A deep link is a navigation, not a switch: this is the one place
            // allowed to open a row, and it opens only the row it was sent to.
            if let Ok(expander) = row.clone().downcast::<adw::ExpanderRow>() {
                expander.set_expanded(true);
            }
            row.add_css_class(TARGET_CLASS);
            let row = row.downgrade();
            glib::timeout_add_local_once(highlight_duration(), move || {
                if let Some(row) = row.upgrade() {
                    row.remove_css_class(TARGET_CLASS);
                }
            });
        }
    }
}

#[cfg(test)]
#[path = "preference_plugins_tests.rs"]
mod tests;
