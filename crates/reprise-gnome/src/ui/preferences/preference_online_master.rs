//! The `Online content` master: the bracket above the plugin list, not a row
//! inside it.
//!
//! `docs/plans/plugins-online-content-master-hierarchy.md`, third draft. The
//! master used to be the first row of the same card as the five it governs —
//! same height, same surface, same indent — so nothing said that it ruled the
//! rest. It now stands free above the card: a bigger title, a state badge, a
//! description over the full width, and a switch that is visibly larger than
//! the child switches. Its children are subordinated by indent and a rail, not
//! by being hidden.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::strings;

/// Set on the master container; every rule in [`css`] is scoped to it.
pub(in crate::ui) const MASTER_CLASS: &str = "reprise-online-master";
const TITLE_CLASS: &str = "reprise-online-master-title";
const BADGE_CLASS: &str = "reprise-online-status-badge";
const BADGE_OFF_CLASS: &str = "off";
const DESCRIPTION_CLASS: &str = "reprise-online-master-description";
const SWITCH_CLASS: &str = "reprise-online-master-switch";
/// Set on the hint below the children card while the gate is off.
pub(in crate::ui) const PAUSED_HINT_CLASS: &str = "reprise-online-paused-hint";
/// The 2px rail left of the children card.
pub(in crate::ui) const RAIL_CLASS: &str = "reprise-online-rail";
/// Set on the rail while the gate is off, so it fades instead of accenting.
pub(in crate::ui) const RAIL_OFF_CLASS: &str = "off";
/// Set on the children card.
pub(in crate::ui) const CHILDREN_CLASS: &str = "reprise-online-children";

/// Master switch size from the draft — deliberately larger than the 46×24 of a
/// child row's switch, so the difference in rank is visible without reading.
const MASTER_SWITCH_WIDTH_PX: i32 = 54;
const MASTER_SWITCH_HEIGHT_PX: i32 = 28;
const MASTER_SWITCH_SLIDER_PX: i32 = 22;

/// Rail width and the gap between it and the card.
const RAIL_WIDTH_PX: i32 = 2;
const CHILDREN_INDENT_PX: i32 = 18;

/// Opacity of the children card while the gate is off: dimmed, but still
/// readable — the draft asks explicitly for legible, not greyed out.
pub(in crate::ui) const CHILDREN_OFF_OPACITY: f64 = 0.42;

/// Roughly the 640px the draft caps the description at, expressed in the unit
/// a GtkLabel wraps by.
const DESCRIPTION_MAX_CHARS: i32 = 72;

/// The badge next to the master title.
pub(in crate::ui) fn badge_text(enabled: bool, enabled_children: usize, total: usize) -> String {
    if enabled {
        strings::online_content_plugins_on(enabled_children, total)
    } else {
        strings::online_content_plugins_off(total)
    }
}

/// The hint under the card while the gate is off. `names` are the sidebar
/// entries that disappear with it, in page order.
pub(in crate::ui) fn paused_hint(total: usize, sidebar_names: &[String]) -> String {
    strings::online_content_paused_hint(total, &strings::joined_names(sidebar_names))
}

type OnToggled = Rc<dyn Fn(bool)>;

struct OnlineMasterInner {
    root: gtk4::Box,
    badge: gtk4::Label,
    toggle: gtk4::Switch,
    on_toggled: RefCell<Option<OnToggled>>,
}

#[derive(Clone)]
pub(in crate::ui) struct OnlineMaster {
    inner: Rc<OnlineMasterInner>,
}

impl OnlineMaster {
    pub(in crate::ui) fn new(active: bool) -> Self {
        let title_text = strings::text(strings::PLUGIN_GROUP_ONLINE_CONTENT);

        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 20);
        root.add_css_class(MASTER_CLASS);

        let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        labels.set_hexpand(true);

        let title_line = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        let title = gtk4::Label::new(Some(&title_text));
        title.set_xalign(0.0);
        title.add_css_class(TITLE_CLASS);
        let badge = gtk4::Label::new(None);
        badge.add_css_class(BADGE_CLASS);
        badge.set_valign(gtk4::Align::Center);
        title_line.append(&title);
        title_line.append(&badge);

        let description = gtk4::Label::new(Some(&strings::text(
            strings::ONLINE_CONTENT_MASTER_DESCRIPTION,
        )));
        description.set_xalign(0.0);
        description.set_wrap(true);
        description.set_max_width_chars(DESCRIPTION_MAX_CHARS);
        description.add_css_class(DESCRIPTION_CLASS);

        labels.append(&title_line);
        labels.append(&description);

        // The switch sits on the title line, not below the description: it is
        // the heading's own control.
        let toggle = gtk4::Switch::builder()
            .active(active)
            .valign(gtk4::Align::Start)
            .build();
        toggle.add_css_class(SWITCH_CLASS);
        toggle.update_property(&[gtk4::accessible::Property::Label(&title_text)]);

        root.append(&labels);
        root.append(&toggle);

        let inner = Rc::new(OnlineMasterInner {
            root,
            badge,
            toggle,
            on_toggled: RefCell::new(None),
        });
        let master = Self { inner };
        master.wire_row_activation();
        master.wire_toggle();
        master
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Widget {
        self.inner.root.upcast_ref()
    }

    pub(in crate::ui) fn is_active(&self) -> bool {
        self.inner.toggle.is_active()
    }

    /// Sets the switch without reporting it back — used to undo a toggle whose
    /// write failed.
    pub(in crate::ui) fn set_active_silently(&self, active: bool) {
        let callback = self.inner.on_toggled.borrow_mut().take();
        self.inner.toggle.set_active(active);
        *self.inner.on_toggled.borrow_mut() = callback;
    }

    pub(in crate::ui) fn set_badge(&self, enabled: bool, enabled_children: usize, total: usize) {
        self.inner
            .badge
            .set_label(&badge_text(enabled, enabled_children, total));
        if enabled {
            self.inner.badge.remove_css_class(BADGE_OFF_CLASS);
        } else {
            self.inner.badge.add_css_class(BADGE_OFF_CLASS);
        }
    }

    pub(in crate::ui) fn set_on_toggled(&self, callback: impl Fn(bool) + 'static) {
        self.inner.on_toggled.replace(Some(Rc::new(callback)));
    }

    fn wire_toggle(&self) {
        // Through the shared inner, not through a clone of the cell: the
        // callback is registered after wiring, and a copied `RefCell` would
        // hold the `None` it had at this moment forever.
        let inner = Rc::downgrade(&self.inner);
        self.inner.toggle.connect_active_notify(move |toggle| {
            let Some(inner) = inner.upgrade() else {
                return;
            };
            let callback = inner.on_toggled.borrow().clone();
            if let Some(callback) = callback {
                callback(toggle.is_active());
            }
        });
    }

    /// The whole row is clickable, not just the switch. A press that landed on
    /// the switch is left alone — it has already toggled itself, and toggling
    /// again here would cancel it out.
    fn wire_row_activation(&self) {
        // input-parity: ACC-8 keyboard=online-master-switch
        let gesture = gtk4::GestureClick::new();
        let root = self.inner.root.downgrade();
        let toggle = self.inner.toggle.downgrade();
        gesture.connect_released(move |gesture, count, x, y| {
            if count != 1 {
                return;
            }
            let (Some(root), Some(toggle)) = (root.upgrade(), toggle.upgrade()) else {
                return;
            };
            if let Some(picked) = root.pick(x, y, gtk4::PickFlags::DEFAULT) {
                if picked == toggle.clone().upcast::<gtk4::Widget>() || picked.is_ancestor(&toggle)
                {
                    return;
                }
            }
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            toggle.set_active(!toggle.is_active());
        });
        self.inner.root.add_controller(gesture);
    }
}

pub(in crate::ui) fn css() -> String {
    format!(
        "/* --- Online content: the master above its children --- */ \
         .{MASTER_CLASS} {{ padding: 2px 6px 16px 0; }} \
         .{TITLE_CLASS} {{ font-size: 1.2em; font-weight: 600; }} \
         .{DESCRIPTION_CLASS} {{ color: alpha(@window_fg_color, 0.6); }} \
         .{BADGE_CLASS} {{ \
           font-size: 0.8em; \
           letter-spacing: 0.06em; \
           text-transform: uppercase; \
           padding: 3px 8px; \
           border-radius: 5px; \
           border: 1px solid alpha(@accent_color, 0.35); \
           color: @reprise_accent_text_color; }} \
         .{BADGE_CLASS}.{BADGE_OFF_CLASS} {{ \
           border-color: alpha(@window_fg_color, 0.16); \
           color: alpha(@window_fg_color, 0.6); }} \
         .{SWITCH_CLASS} {{ \
           min-width: {MASTER_SWITCH_WIDTH_PX}px; \
           min-height: {MASTER_SWITCH_HEIGHT_PX}px; }} \
         .{SWITCH_CLASS} > slider {{ \
           min-width: {MASTER_SWITCH_SLIDER_PX}px; \
           min-height: {MASTER_SWITCH_SLIDER_PX}px; }} \
         .{RAIL_CLASS} {{ \
           min-width: {RAIL_WIDTH_PX}px; \
           background-image: linear-gradient(to bottom, \
             alpha(@accent_color, 0.55), alpha(@accent_color, 0)); }} \
         .{RAIL_CLASS}.{RAIL_OFF_CLASS} {{ \
           background-image: linear-gradient(to bottom, \
             alpha(@window_fg_color, 0.14), alpha(@window_fg_color, 0)); }} \
         .{CHILDREN_CLASS} {{ margin-left: {CHILDREN_INDENT_PX}px; }} \
         .{PAUSED_HINT_CLASS} {{ \
           margin: 14px 0 0 20px; \
           padding: 12px 16px; \
           border-radius: 8px; \
           border: 1px solid alpha(@window_fg_color, 0.07); \
           background-color: alpha(@window_fg_color, 0.04); \
           color: alpha(@window_fg_color, 0.6); }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_badge_counts_the_children_on_and_names_the_total_when_off() {
        assert_eq!(badge_text(true, 7, 7), "7 of 7 plugins on");
        assert_eq!(badge_text(true, 3, 7), "3 of 7 plugins on");
        assert_eq!(badge_text(false, 2, 7), "all 7 plugins off");
    }

    #[test]
    fn the_paused_hint_names_the_sidebar_entries_that_go_with_it() {
        let names = ["Concerts", "New Releases", "YouTube"]
            .map(str::to_owned)
            .to_vec();

        assert_eq!(
            paused_hint(5, &names),
            "5 plugins paused · no requests · Concerts, New Releases and YouTube hidden from the sidebar"
        );
    }

    #[test]
    fn the_master_switch_is_larger_than_a_child_switch() {
        let css = css();

        assert!(css.contains(&format!("min-width: {MASTER_SWITCH_WIDTH_PX}px")));
        assert!(css.contains(&format!("min-height: {MASTER_SWITCH_HEIGHT_PX}px")));
        const {
            assert!(MASTER_SWITCH_WIDTH_PX > 46 && MASTER_SWITCH_HEIGHT_PX > 24);
        }
    }

    #[test]
    fn the_rail_and_the_indent_carry_the_subordination() {
        let css = css();

        assert!(css.contains(&format!(".{RAIL_CLASS} {{")));
        assert!(css.contains("linear-gradient(to bottom, alpha(@accent_color, 0.55)"));
        assert!(css.contains(&format!("margin-left: {CHILDREN_INDENT_PX}px")));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn the_row_toggles_from_anywhere_but_never_twice() {
        gtk4::init().unwrap();
        let master = OnlineMaster::new(false);
        let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
        {
            let seen = seen.clone();
            master.set_on_toggled(move |enabled| seen.borrow_mut().push(enabled));
        }

        // A click that misses the switch toggles it exactly once.
        master.inner.toggle.set_active(true);
        assert_eq!(*seen.borrow(), vec![true]);

        // Undoing a failed write must not report back.
        master.set_active_silently(false);
        assert_eq!(*seen.borrow(), vec![true]);
        assert!(!master.is_active());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn the_master_css_parses_without_gtk_errors() {
        gtk4::init().unwrap();
        let errors = crate::ui::style::css_parse_errors(&css());
        assert!(
            errors.is_empty(),
            "GTK reported CSS parsing errors: {errors:?}"
        );
    }
}
