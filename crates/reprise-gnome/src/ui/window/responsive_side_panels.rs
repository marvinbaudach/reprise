//! Coordinates the Library's two side panels at constrained window widths.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;

use crate::ui::info_panel::InfoPanel;

const CONSTRAINED_WIDTH: i32 = 1_400;
const UNDO_TIMEOUT_SECONDS: u32 = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PanelVisibility {
    library: bool,
    now_playing: bool,
}

impl PanelVisibility {
    fn any_open(self) -> bool {
        self.library || self.now_playing
    }
}

fn constrained_visibility(_current: PanelVisibility) -> PanelVisibility {
    PanelVisibility {
        library: false,
        now_playing: false,
    }
}

fn effective_library_visibility(
    shows_sidebar: bool,
    collapsed: bool,
    persisted_collapsed: bool,
) -> bool {
    if collapsed {
        !persisted_collapsed
    } else {
        shows_sidebar
    }
}

#[derive(Debug, Default)]
struct ConstraintState {
    active: bool,
    snapshot: Option<PanelVisibility>,
    changed_by_user: bool,
}

impl ConstraintState {
    fn apply(&mut self, current: PanelVisibility) -> Option<PanelVisibility> {
        self.active = true;
        self.snapshot = Some(current);
        self.changed_by_user = false;
        current.any_open().then(|| constrained_visibility(current))
    }

    fn note_user_change(&mut self) {
        if self.active {
            self.changed_by_user = true;
        }
    }

    fn should_enforce_constraint(&self) -> bool {
        self.active && !self.changed_by_user
    }

    fn undo(&mut self) -> Option<PanelVisibility> {
        if !self.active {
            return None;
        }
        self.changed_by_user = true;
        self.snapshot
    }

    fn unapply(&mut self) -> Option<PanelVisibility> {
        let restore = (self.active && !self.changed_by_user)
            .then_some(self.snapshot)
            .flatten();
        self.active = false;
        self.snapshot = None;
        self.changed_by_user = false;
        restore
    }
}

fn visibility(
    library: &adw::OverlaySplitView,
    now_playing: &Rc<InfoPanel>,
    conn: &Rc<Db>,
) -> PanelVisibility {
    let persisted_collapsed = reprise_core::library::settings::get_sidebar_collapsed(conn);
    PanelVisibility {
        library: effective_library_visibility(
            library.shows_sidebar(),
            library.is_collapsed(),
            persisted_collapsed,
        ),
        now_playing: now_playing.is_panel_visible(),
    }
}

fn set_transient_visibility(
    applying: &Cell<bool>,
    library: &adw::OverlaySplitView,
    now_playing: &Rc<InfoPanel>,
    visibility: PanelVisibility,
) {
    applying.set(true);
    library.set_show_sidebar(visibility.library);
    now_playing.set_transient_visibility(visibility.now_playing);
    applying.set(false);
}

pub(in crate::ui) fn install(
    window: &adw::ApplicationWindow,
    overlay: &adw::ToastOverlay,
    library: &adw::OverlaySplitView,
    now_playing: &Rc<InfoPanel>,
    conn: &Rc<Db>,
) {
    let condition = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        f64::from(CONSTRAINED_WIDTH),
        adw::LengthUnit::Px,
    );
    let breakpoint = adw::Breakpoint::new(condition);
    let state = Rc::new(RefCell::new(ConstraintState::default()));
    let applying = Rc::new(Cell::new(false));
    let active_toast = Rc::new(RefCell::new(None::<adw::Toast>));

    {
        let state = state.clone();
        let applying = applying.clone();
        let weak_library = library.downgrade();
        library.connect_show_sidebar_notify(move |_| {
            if applying.get() {
                return;
            }
            let state = state.clone();
            let library = weak_library.clone();
            gtk4::glib::idle_add_local_once(move || {
                let Some(library) = library.upgrade() else {
                    return;
                };
                if state.borrow().should_enforce_constraint() && library.shows_sidebar() {
                    state.borrow_mut().note_user_change();
                }
            });
        });
    }
    {
        let state = state.clone();
        let applying = applying.clone();
        let library = library.clone();
        library.connect_collapsed_notify(move |library| {
            if state.borrow().should_enforce_constraint() && library.shows_sidebar() {
                applying.set(true);
                library.set_show_sidebar(false);
                applying.set(false);
            }
        });
    }
    {
        let state = state.clone();
        let applying = applying.clone();
        let weak_now_playing = Rc::downgrade(now_playing);
        now_playing.widget().connect_show_sidebar_notify(move |_| {
            if applying.get() {
                return;
            }
            let state = state.clone();
            let now_playing = weak_now_playing.clone();
            gtk4::glib::idle_add_local_once(move || {
                let Some(now_playing) = now_playing.upgrade() else {
                    return;
                };
                if state.borrow().should_enforce_constraint() && now_playing.is_panel_visible() {
                    state.borrow_mut().note_user_change();
                }
            });
        });
    }
    {
        let state = state.clone();
        let applying = applying.clone();
        let now_playing = now_playing.clone();
        let split = now_playing.widget().clone();
        split.connect_collapsed_notify(move |split| {
            if state.borrow().should_enforce_constraint() && split.shows_sidebar() {
                applying.set(true);
                now_playing.set_transient_visibility(false);
                applying.set(false);
            }
        });
    }
    {
        let state = state.clone();
        let applying = applying.clone();
        let library = library.clone();
        let now_playing = now_playing.clone();
        let conn = conn.clone();
        let overlay = overlay.downgrade();
        let active_toast = active_toast.clone();
        breakpoint.connect_apply(move |_| {
            let current = visibility(&library, &now_playing, &conn);
            let Some(target) = state.borrow_mut().apply(current) else {
                return;
            };
            set_transient_visibility(&applying, &library, &now_playing, target);

            let Some(overlay) = overlay.upgrade() else {
                return;
            };
            if let Some(previous) = active_toast.borrow_mut().take() {
                previous.dismiss();
            }
            let toast = adw::Toast::new(&crate::ui::strings::text(
                crate::ui::strings::SIDE_PANELS_CLOSED,
            ));
            toast.set_button_label(Some(&crate::ui::strings::text(crate::ui::strings::UNDO)));
            toast.set_timeout(UNDO_TIMEOUT_SECONDS);
            toast.set_priority(adw::ToastPriority::High);
            {
                let state = state.clone();
                let applying = applying.clone();
                let library = library.clone();
                let now_playing = now_playing.clone();
                toast.connect_button_clicked(move |toast| {
                    toast.dismiss();
                    let Some(snapshot) = state.borrow_mut().undo() else {
                        return;
                    };
                    set_transient_visibility(&applying, &library, &now_playing, snapshot);
                });
            }
            overlay.add_toast(toast.clone());
            *active_toast.borrow_mut() = Some(toast);
        });
    }
    {
        let state = state.clone();
        let applying = applying.clone();
        let library = library.clone();
        let now_playing = now_playing.clone();
        let active_toast = active_toast.clone();
        breakpoint.connect_unapply(move |_| {
            if let Some(toast) = active_toast.borrow_mut().take() {
                toast.dismiss();
            }
            let Some(snapshot) = state.borrow_mut().unapply() else {
                return;
            };
            set_transient_visibility(&applying, &library, &now_playing, snapshot);
        });
    }

    window.add_breakpoint(breakpoint);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_7_constrained_window_closes_both_side_panels_as_one_transition() {
        let open = PanelVisibility {
            library: true,
            now_playing: true,
        };

        assert_eq!(
            constrained_visibility(open),
            PanelVisibility {
                library: false,
                now_playing: false,
            }
        );
    }

    #[test]
    fn style_7_undo_restores_the_exact_pre_snap_panel_state() {
        let mut state = ConstraintState::default();
        let before_snap = PanelVisibility {
            library: true,
            now_playing: false,
        };

        assert_eq!(
            state.apply(before_snap),
            Some(PanelVisibility {
                library: false,
                now_playing: false,
            })
        );
        assert_eq!(state.undo(), Some(before_snap));
        assert_eq!(
            state.unapply(),
            None,
            "widening must not replay the snapshot after Undo already restored it"
        );
    }

    #[test]
    fn style_7_widening_restores_pre_snap_state_unless_the_user_changed_it() {
        let before_snap = PanelVisibility {
            library: false,
            now_playing: true,
        };
        let mut untouched = ConstraintState::default();
        untouched.apply(before_snap);
        assert_eq!(untouched.unapply(), Some(before_snap));

        let mut changed = ConstraintState::default();
        changed.apply(before_snap);
        changed.note_user_change();
        assert_eq!(changed.unapply(), None);
    }

    #[test]
    fn style_7_initial_collapsed_layout_uses_the_persisted_wide_sidebar_state() {
        assert!(effective_library_visibility(false, true, false));
        assert!(!effective_library_visibility(false, true, true));
        assert!(!effective_library_visibility(false, false, false));
        assert!(effective_library_visibility(true, false, true));
    }
}
