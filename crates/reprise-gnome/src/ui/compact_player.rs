//! Four compact playback layouts fed by the same `PlayerController::sync_*`
//! path as the library bar and Now Playing page.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::{gdk, gio, prelude::*};
use reprise_core::format::format_duration;
use reprise_core::library::settings::CompactLayout;
use reprise_core::playback::PlaybackState;
use reprise_core::queue::Repeat;

use super::compact_player_layouts::{self, LayoutMetrics, LayoutWidgets};
use super::compact_player_menu::{self, CompactMenu};
use super::compact_player_state::{normalized_position, volume_percent, CompactPresentation};
use super::cover_loader::CoverLoader;
use super::player_bar::{
    ICON_PAUSE, ICON_PLAY, ICON_REPEAT_ALL, ICON_REPEAT_ONE, REPEAT_OFF_CSS_CLASS,
};
use super::player_bar_seek::{
    should_apply_position_tick, should_clear_drag_guard_on_track_change,
    should_finish_observer_cancel, should_finish_observer_stop, should_self_heal,
    should_update_range,
};
use super::strings;

const ZERO_TIME: &str = "0:00";
type RestoreCallback = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

pub(super) struct CompactPlayer {
    stack: gtk4::Stack,
    views: Vec<LayoutWidgets>,
    menu: CompactMenu,
    layout: Rc<Cell<CompactLayout>>,
    presentation: Rc<RefCell<CompactPresentation>>,
    updating_scale: Rc<Cell<bool>>,
    dragging: Rc<Cell<bool>>,
    pointer_down: Rc<Cell<bool>>,
    seek_gestures: RefCell<Vec<gtk4::GestureClick>>,
    last_duration_ms: Cell<i64>,
    updating_shuffle: Rc<Cell<bool>>,
    updating_volume: Rc<Cell<bool>>,
    on_restore: RestoreCallback,
}

#[derive(Clone)]
pub(super) struct CompactPlayerHandle {
    stack: gtk4::Stack,
    layout: Rc<Cell<CompactLayout>>,
    layout_action: gio::SimpleAction,
}

impl CompactPlayerHandle {
    pub(super) fn widget(&self) -> &gtk4::Stack {
        &self.stack
    }

    pub(super) fn set_layout(&self, layout: CompactLayout) {
        self.layout.set(layout);
        self.stack
            .set_visible_child_name(compact_player_layouts::layout_token(layout));
        self.layout_action
            .set_state(&compact_player_menu::active_target(layout).to_variant());
    }
}

impl CompactPlayer {
    pub(super) fn new() -> Self {
        let stack = gtk4::Stack::new();
        stack.set_transition_type(gtk4::StackTransitionType::None);
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);
        let views: Vec<_> = [
            CompactLayout::Bar,
            CompactLayout::Cover,
            CompactLayout::Pill,
            CompactLayout::Card,
        ]
        .into_iter()
        .map(compact_player_layouts::build)
        .collect();
        for view in &views {
            stack.add_named(
                &view.root,
                Some(compact_player_layouts::layout_token(view.layout)),
            );
        }
        let menu = CompactMenu::build(CompactLayout::Bar);
        stack.insert_action_group("compact", Some(&menu.action_group));

        let compact = Self {
            stack,
            views,
            menu,
            layout: Rc::new(Cell::new(CompactLayout::Bar)),
            presentation: Rc::new(RefCell::new(CompactPresentation::default())),
            updating_scale: Rc::new(Cell::new(false)),
            dragging: Rc::new(Cell::new(false)),
            pointer_down: Rc::new(Cell::new(false)),
            seek_gestures: RefCell::new(Vec::new()),
            last_duration_ms: Cell::new(0),
            updating_shuffle: Rc::new(Cell::new(false)),
            updating_volume: Rc::new(Cell::new(false)),
            on_restore: Rc::new(RefCell::new(None)),
        };
        compact.wire_cover_mirror();
        compact.wire_menu_openers();
        compact.wire_restore_buttons();
        compact.set_repeat_indicator(Repeat::Off);
        compact.refresh_sensitivity();
        compact
    }

    pub(super) fn cover_image(&self) -> &gtk4::Image {
        &self.views[0].cover
    }

    pub(super) fn widget(&self) -> &gtk4::Stack {
        &self.stack
    }

    pub(super) fn handle(&self) -> CompactPlayerHandle {
        CompactPlayerHandle {
            stack: self.widget().clone(),
            layout: self.layout.clone(),
            layout_action: self.menu.layout_action(),
        }
    }

    pub(super) fn set_cover_placeholder(&self) {
        for view in &self.views {
            CoverLoader::set_placeholder(&view.cover);
        }
    }

    pub(super) fn layout(&self) -> CompactLayout {
        self.layout.get()
    }

    pub(super) fn set_layout(&self, layout: CompactLayout) {
        self.handle().set_layout(layout);
        let metrics = self.metrics();
        tracing::debug!(
            ?layout,
            width = metrics.width,
            height = metrics.height,
            "compact layout selected"
        );
    }

    pub(super) fn metrics(&self) -> LayoutMetrics {
        compact_player_layouts::metrics(self.layout())
    }

    pub(super) fn set_on_restore(&self, callback: Rc<dyn Fn()>) {
        *self.on_restore.borrow_mut() = Some(callback.clone());
        self.menu.set_on_restore(callback);
    }

    pub(super) fn set_on_layout(&self, callback: Rc<dyn Fn(CompactLayout)>) {
        self.menu.set_on_layout(callback);
    }

    pub(super) fn set_on_preferences(&self, callback: Rc<dyn Fn()>) {
        self.menu.set_on_preferences(callback);
    }

    pub(super) fn set_track(&self, title: &str, artist: &str, album: &str, year: Option<i32>) {
        if should_clear_drag_guard_on_track_change(
            self.pointer_down.get(),
            self.seek_gesture_is_active(),
        ) {
            self.dragging.set(false);
        }
        {
            let mut presentation = self.presentation.borrow_mut();
            presentation.title = title.to_string();
            presentation.artist = artist.to_string();
            presentation.album = album.to_string();
            presentation.year = year;
        }
        for view in &self.views {
            let detail_rows = compact_player_layouts::visible_detail_rows(view.layout, album, year);
            view.title.set_text(title);
            view.artist.set_text(artist);
            view.album.set_text(album);
            view.album
                .set_visible(detail_rows.contains(&compact_player_layouts::MetadataRow::Album));
            view.year
                .set_text(&year.map(|value| value.to_string()).unwrap_or_default());
            view.year
                .set_visible(detail_rows.contains(&compact_player_layouts::MetadataRow::Year));
        }
    }

    pub(super) fn clear_track(&self) {
        if should_clear_drag_guard_on_track_change(
            self.pointer_down.get(),
            self.seek_gesture_is_active(),
        ) {
            self.dragging.set(false);
        }
        self.presentation.borrow_mut().clear_track();
        for view in &self.views {
            view.title.set_text("");
            view.artist.set_text("");
            view.album.set_text("");
            view.year.set_text("");
            view.album.set_visible(false);
            view.year.set_visible(false);
            view.position.set_text(ZERO_TIME);
            view.duration.set_text(ZERO_TIME);
        }
        self.set_cover_placeholder();
    }

    pub(super) fn set_state(&self, state: PlaybackState) {
        self.presentation.borrow_mut().set_playback_state(state);
        let is_playing = state == PlaybackState::Playing;
        let action_label = strings::text(if is_playing {
            strings::PAUSE
        } else {
            strings::PLAY
        });
        for view in &self.views {
            view.play_pause
                .set_icon_name(if is_playing { ICON_PAUSE } else { ICON_PLAY });
            view.play_pause.set_tooltip_text(Some(&action_label));
            view.play_pause
                .update_property(&[gtk4::accessible::Property::Label(&action_label)]);
        }
        if state == PlaybackState::Stopped {
            self.pointer_down.set(false);
            self.dragging.set(false);
            self.set_position(0, 0);
        }
        self.refresh_sensitivity();
    }

    pub(super) fn set_position(&self, position_ms: i64, duration_ms: i64) {
        let (position_ms, duration_ms) = normalized_position(position_ms, duration_ms);
        {
            let mut presentation = self.presentation.borrow_mut();
            presentation.position_ms = position_ms;
            presentation.duration_ms = duration_ms;
        }
        if should_self_heal(
            self.dragging.get(),
            self.pointer_down.get(),
            self.seek_gesture_is_active(),
        ) {
            tracing::warn!("compact-player drag guard was stuck; self-healing");
            self.dragging.set(false);
        }
        let update_range = should_update_range(self.last_duration_ms.get(), duration_ms);
        self.updating_scale.set(true);
        for view in &self.views {
            if update_range {
                view.scale.set_range(0.0, duration_ms.max(1) as f64);
            }
            if should_apply_position_tick(self.dragging.get()) {
                view.scale.set_value(position_ms as f64);
            }
            view.position.set_text(&format_duration(position_ms));
            view.duration.set_text(&format_duration(duration_ms));
        }
        self.updating_scale.set(false);
        if update_range {
            self.last_duration_ms.set(duration_ms);
        }
    }

    pub(super) fn set_transport_enabled(&self, enabled: bool) {
        self.presentation.borrow_mut().transport_enabled = enabled;
        for view in &self.views {
            view.previous.set_sensitive(enabled);
            view.next.set_sensitive(enabled);
        }
        self.refresh_sensitivity();
    }

    pub(super) fn set_shuffle_indicator(&self, active: bool) {
        self.presentation.borrow_mut().shuffled = active;
        self.updating_shuffle.set(true);
        for view in &self.views {
            if let Some(shuffle) = &view.shuffle {
                shuffle.set_active(active);
            }
        }
        self.menu.set_shuffle(active);
        self.updating_shuffle.set(false);
    }

    pub(super) fn set_repeat_indicator(&self, repeat: Repeat) {
        self.presentation.borrow_mut().repeat = repeat;
        let (icon, off) = match repeat {
            Repeat::Off => (ICON_REPEAT_ALL, true),
            Repeat::All => (ICON_REPEAT_ALL, false),
            Repeat::One => (ICON_REPEAT_ONE, false),
        };
        for view in &self.views {
            if let Some(button) = &view.repeat {
                button.set_icon_name(icon);
                if off {
                    button.add_css_class(REPEAT_OFF_CSS_CLASS);
                } else {
                    button.remove_css_class(REPEAT_OFF_CSS_CLASS);
                }
            }
        }
        self.menu.set_repeat(repeat);
    }

    pub(super) fn set_volume_indicator(&self, volume: f64) {
        let volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.presentation.borrow_mut().volume_percent = volume_percent(volume);
        self.updating_volume.set(true);
        for view in &self.views {
            if let Some(button) = &view.volume {
                button.set_value(volume);
            }
        }
        self.menu.set_volume(volume);
        self.updating_volume.set(false);
    }

    pub(super) fn connect_play_pause(&self, callback: impl Fn() + 'static) {
        let callback = Rc::new(callback);
        for view in &self.views {
            let callback = callback.clone();
            view.play_pause.connect_clicked(move |_| callback());
        }
    }

    pub(super) fn connect_previous(&self, callback: impl Fn() + 'static) {
        connect_buttons(self.views.iter().map(|view| &view.previous), callback);
    }

    pub(super) fn connect_next(&self, callback: impl Fn() + 'static) {
        connect_buttons(self.views.iter().map(|view| &view.next), callback);
    }

    pub(super) fn connect_shuffle_toggled(&self, callback: impl Fn(bool) + 'static) {
        let callback = Rc::new(callback);
        for view in &self.views {
            let Some(shuffle) = &view.shuffle else {
                continue;
            };
            let callback = callback.clone();
            let updating = self.updating_shuffle.clone();
            shuffle.connect_toggled(move |button| {
                if !updating.get() {
                    callback(button.is_active());
                }
            });
        }
        self.menu.set_on_shuffle(callback);
    }

    pub(super) fn connect_repeat_clicked(&self, callback: impl Fn() + 'static) {
        let callback = Rc::new(callback);
        for view in &self.views {
            let Some(repeat) = &view.repeat else {
                continue;
            };
            let callback = callback.clone();
            repeat.connect_clicked(move |_| callback());
        }
        let presentation = self.presentation.clone();
        self.menu.set_on_repeat(Rc::new(move |desired| {
            let current = presentation.borrow().repeat;
            let steps = repeat_steps(current, desired);
            for _ in 0..steps {
                callback();
            }
        }));
    }

    pub(super) fn connect_volume_changed(&self, callback: impl Fn(f64) + 'static) {
        let callback = Rc::new(callback);
        for view in &self.views {
            let Some(volume) = &view.volume else {
                continue;
            };
            let callback = callback.clone();
            let updating = self.updating_volume.clone();
            volume.connect_value_changed(move |_, value| {
                if !updating.get() {
                    callback(value);
                }
            });
        }
        self.menu.set_on_volume(callback);
    }

    pub(super) fn connect_seek(&self, callback: impl Fn(i64) + 'static) {
        let callback = Rc::new(callback);
        let mut gestures = Vec::with_capacity(self.views.len());
        for view in &self.views {
            self.wire_seek(view, &callback, &mut gestures);
        }
        *self.seek_gestures.borrow_mut() = gestures;
    }

    fn wire_seek(
        &self,
        view: &LayoutWidgets,
        callback: &Rc<impl Fn(i64) + 'static>,
        gestures: &mut Vec<gtk4::GestureClick>,
    ) {
        let updating = self.updating_scale.clone();
        let dragging = self.dragging.clone();
        let changed = callback.clone();
        view.scale.connect_value_changed(move |scale| {
            if !updating.get() && !dragging.get() {
                changed(scale.value() as i64);
            }
        });

        let click = gtk4::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        click.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let press_dragging = self.dragging.clone();
        click.connect_pressed(move |_, _, _, _| press_dragging.set(true));

        let end_drag: Rc<dyn Fn()> = {
            let dragging = self.dragging.clone();
            let pointer_down = self.pointer_down.clone();
            let scale = view.scale.downgrade();
            let callback = callback.clone();
            Rc::new(move || {
                pointer_down.set(false);
                if !dragging.replace(false) {
                    return;
                }
                if let Some(scale) = scale.upgrade() {
                    callback(scale.value() as i64);
                }
            })
        };

        let raw = gtk4::EventControllerLegacy::new();
        raw.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let raw_pointer_down = self.pointer_down.clone();
        let raw_dragging = self.dragging.clone();
        let raw_end = end_drag.clone();
        raw.connect_event(move |_, event| {
            let primary = event
                .downcast_ref::<gdk::ButtonEvent>()
                .is_some_and(|button| button.button() == gdk::BUTTON_PRIMARY);
            match event.event_type() {
                gdk::EventType::ButtonPress if primary => {
                    raw_pointer_down.set(true);
                    raw_dragging.set(true);
                }
                gdk::EventType::TouchBegin => {
                    raw_pointer_down.set(true);
                    raw_dragging.set(true);
                }
                gdk::EventType::ButtonRelease if primary => raw_end(),
                gdk::EventType::TouchEnd | gdk::EventType::TouchCancel => raw_end(),
                _ => {}
            }
            gtk4::glib::Propagation::Proceed
        });
        view.scale.add_controller(raw);

        let released = end_drag.clone();
        click.connect_released(move |_, _, _, _| released());
        let cancel = end_drag.clone();
        let cancel_pointer_down = self.pointer_down.clone();
        click.connect_cancel(move |_, _| {
            if should_finish_observer_cancel(cancel_pointer_down.get()) {
                cancel();
            }
        });
        let unpaired = end_drag.clone();
        click.connect_unpaired_release(move |_, _, _, _, _| unpaired());
        let stopped = end_drag;
        let stopped_pointer_down = self.pointer_down.clone();
        click.connect_stopped(move |gesture| {
            if should_finish_observer_stop(stopped_pointer_down.get(), gesture.is_active()) {
                stopped();
            }
        });
        view.scale.add_controller(click.clone());
        gestures.push(click);
    }

    fn wire_cover_mirror(&self) {
        let secondaries: Vec<_> = self
            .views
            .iter()
            .skip(1)
            .map(|view| view.cover.clone())
            .collect();
        self.views[0]
            .cover
            .connect_notify_local(Some("paintable"), move |primary, _| {
                let paintable = primary.paintable();
                for image in &secondaries {
                    image.set_paintable(paintable.as_ref());
                }
            });
    }

    fn wire_menu_openers(&self) {
        for view in &self.views {
            let popover = self.menu.popover.clone();
            let anchor = view.menu.clone();
            view.menu.connect_clicked(move |_| {
                compact_player_menu::popup_at(&popover, anchor.upcast_ref(), None);
            });

            let click = gtk4::GestureClick::new();
            click.set_button(gdk::BUTTON_SECONDARY);
            let popover = self.menu.popover.clone();
            let anchor = view.root.clone();
            click.connect_pressed(move |_, _, x, y| {
                let interactive = anchor
                    .pick(x, y, gtk4::PickFlags::DEFAULT)
                    .is_some_and(|picked| is_interactive_descendant(&picked, &anchor));
                if !compact_player_menu::accepts_context_menu(interactive) {
                    return;
                }
                compact_player_menu::popup_at(&popover, &anchor, Some((x as i32, y as i32)));
            });
            view.root.add_controller(click);

            let keys = gtk4::EventControllerKey::new();
            let popover = self.menu.popover.clone();
            let anchor = view.menu.clone();
            keys.connect_key_pressed(move |_, key, _, modifiers| {
                if !compact_player_menu::is_context_menu_shortcut(key, modifiers) {
                    return gtk4::glib::Propagation::Proceed;
                }
                compact_player_menu::popup_at(&popover, anchor.upcast_ref(), None);
                gtk4::glib::Propagation::Stop
            });
            view.root.add_controller(keys);
        }
    }

    fn wire_restore_buttons(&self) {
        for view in &self.views {
            let callback = self.on_restore.clone();
            view.restore.connect_clicked(move |_| {
                let callback = callback.borrow().clone();
                if let Some(callback) = callback {
                    callback();
                }
            });
        }
    }

    fn seek_gesture_is_active(&self) -> bool {
        self.seek_gestures
            .borrow()
            .iter()
            .any(gtk4::prelude::GestureExt::is_active)
    }

    fn refresh_sensitivity(&self) {
        let presentation = self.presentation.borrow();
        let sensitive = super::player_bar_state::bar_should_be_sensitive(
            presentation.state,
            presentation.transport_enabled,
        );
        for view in &self.views {
            view.play_pause
                .set_sensitive(compact_player_layouts::control_is_sensitive(
                    compact_player_layouts::ControlRole::Playback,
                    sensitive,
                ));
            view.scale
                .set_sensitive(presentation.state != PlaybackState::Stopped);
            view.menu
                .set_sensitive(compact_player_layouts::control_is_sensitive(
                    compact_player_layouts::ControlRole::WindowAction,
                    sensitive,
                ));
            view.restore
                .set_sensitive(compact_player_layouts::control_is_sensitive(
                    compact_player_layouts::ControlRole::WindowAction,
                    sensitive,
                ));
        }
    }
}

fn connect_buttons<'a>(
    buttons: impl Iterator<Item = &'a gtk4::Button>,
    callback: impl Fn() + 'static,
) {
    let callback = Rc::new(callback);
    for button in buttons {
        let callback = callback.clone();
        button.connect_clicked(move |_| callback());
    }
}

fn repeat_steps(current: Repeat, desired: Repeat) -> usize {
    match (current, desired) {
        (a, b) if a == b => 0,
        (Repeat::Off, Repeat::All) | (Repeat::All, Repeat::One) | (Repeat::One, Repeat::Off) => 1,
        _ => 2,
    }
}

fn is_interactive_descendant(widget: &gtk4::Widget, root: &gtk4::Widget) -> bool {
    let mut current = Some(widget.clone());
    while let Some(widget) = current {
        if widget.is::<gtk4::Button>()
            || widget.is::<gtk4::Scale>()
            || widget.is::<gtk4::ScaleButton>()
        {
            return true;
        }
        if widget == *root {
            break;
        }
        current = widget.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_layout_contract(layout: CompactLayout) {
        if gtk4::init().is_err() {
            return;
        }
        let compact = CompactPlayer::new();
        compact.set_layout(layout);
        compact.set_track("Track", "Artist", "Album", Some(2026));
        let view = compact
            .views
            .iter()
            .find(|view| view.layout == layout)
            .unwrap();
        assert!(view.cover.parent().is_some());
        assert!(view.title.parent().is_some());
        assert!(view.artist.parent().is_some());
        assert_eq!(
            view.album.is_visible(),
            matches!(layout, CompactLayout::Cover | CompactLayout::Card)
        );
        assert_eq!(view.year.is_visible(), layout == CompactLayout::Card);
        if layout == CompactLayout::Card {
            assert_eq!(view.year.text(), "2026");
        }
        assert_eq!(view.previous.tooltip_text().as_deref(), Some("Previous"));
        assert_eq!(view.play_pause.tooltip_text().as_deref(), Some("Play"));
        assert_eq!(view.next.tooltip_text().as_deref(), Some("Next"));
        assert_eq!(
            view.scale.tooltip_text().as_deref(),
            Some("Playback position")
        );
        assert_eq!(
            view.menu.tooltip_text().as_deref(),
            Some("Compact player menu")
        );
        assert_eq!(
            view.restore.tooltip_text().as_deref(),
            Some("Return to Library")
        );
        let metrics = compact.metrics();
        let (_, stack_width, _, _) = compact.widget().measure(gtk4::Orientation::Horizontal, -1);
        let (_, stack_height, _, _) = compact
            .widget()
            .measure(gtk4::Orientation::Vertical, metrics.width);
        assert!(
            stack_width <= metrics.width,
            "active {layout:?} stack width {stack_width} > {}",
            metrics.width
        );
        assert!(
            stack_height <= metrics.height,
            "active {layout:?} stack height {stack_height} > {}",
            metrics.height
        );
        assert_eq!(
            tree_has(&view.root, &|widget| widget.is::<libadwaita::HeaderBar>()),
            metrics.separate_header
        );
        if !metrics.separate_header {
            assert!(tree_has(&view.root, &|widget| widget.is::<gtk4::WindowControls>()));
        }
        let (_, natural_width, _, _) = view.root.measure(gtk4::Orientation::Horizontal, -1);
        let (_, natural_height, _, _) = view
            .root
            .measure(gtk4::Orientation::Vertical, metrics.width);
        assert!(
            natural_width <= metrics.width,
            "{layout:?} width {natural_width} > {}",
            metrics.width
        );
        assert!(
            natural_height <= metrics.height,
            "{layout:?} height {natural_height} > {}",
            metrics.height
        );
        if metrics.direct_shuffle {
            assert_eq!(
                view.shuffle.as_ref().unwrap().tooltip_text().as_deref(),
                Some("Shuffle")
            );
            assert_eq!(
                view.repeat.as_ref().unwrap().tooltip_text().as_deref(),
                Some("Repeat")
            );
            assert_eq!(
                view.volume.as_ref().unwrap().tooltip_text().as_deref(),
                Some("Volume")
            );
        } else {
            assert!(view.shuffle.is_none());
            assert!(view.repeat.is_none());
            assert!(view.volume.is_none());
            assert!(compact.menu.action_group.has_action("shuffle"));
            assert!(compact.menu.action_group.has_action("repeat"));
            assert_eq!(
                compact.menu.volume.tooltip_text().as_deref(),
                Some("Volume")
            );
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn bar_layout_has_required_accessible_controls_and_fits() {
        assert_layout_contract(CompactLayout::Bar);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn cover_layout_has_required_accessible_controls_and_fits() {
        assert_layout_contract(CompactLayout::Cover);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn pill_layout_has_required_accessible_controls_and_fits() {
        assert_layout_contract(CompactLayout::Pill);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn card_layout_has_required_accessible_controls_and_fits() {
        assert_layout_contract(CompactLayout::Card);
    }

    fn tree_has(root: &gtk4::Widget, predicate: &dyn Fn(&gtk4::Widget) -> bool) -> bool {
        if predicate(root) {
            return true;
        }
        let mut child = root.first_child();
        while let Some(widget) = child {
            if tree_has(&widget, predicate) {
                return true;
            }
            child = widget.next_sibling();
        }
        false
    }
}
