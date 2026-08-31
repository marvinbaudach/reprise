//! Coordinates the Library's two side panels at constrained window widths.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::info_panel::InfoPanel;
use crate::ui::now_playing_column::PANEL_WIDTH;
use crate::ui::sidebar_presentation::SIDEBAR_MIN_WIDTH;
use crate::ui::track_list::responsive_columns::FOLD_BREAKPOINT_WIDTH;

const CONSTRAINED_WIDTH: i32 = SIDEBAR_MIN_WIDTH as i32 + PANEL_WIDTH + FOLD_BREAKPOINT_WIDTH;
const UNDO_TIMEOUT_SECONDS: u32 = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PanelVisibility {
    library: bool,
    now_playing: bool,
}

#[derive(Clone, Copy)]
enum Panel {
    Library,
    NowPlaying,
}

impl Panel {
    fn is_open(self, visibility: PanelVisibility) -> bool {
        match self {
            Panel::Library => visibility.library,
            Panel::NowPlaying => visibility.now_playing,
        }
    }
}

/// The state a freshly opened panel forces on the other one while constrained.
///
/// The exclusion runs one way only. The library sidebar is a structural
/// column that no width and no other panel takes away, so opening it makes
/// the now-playing panel yield — while opening the now-playing panel leaves
/// the sidebar exactly as the user left it.
fn visibility_after_opening(panel: Panel, current: PanelVisibility) -> PanelVisibility {
    match panel {
        Panel::Library => PanelVisibility {
            library: true,
            now_playing: false,
        },
        Panel::NowPlaying => PanelVisibility {
            library: current.library,
            now_playing: true,
        },
    }
}

fn constrained_visibility(current: PanelVisibility) -> PanelVisibility {
    PanelVisibility {
        library: current.library,
        now_playing: false,
    }
}

#[derive(Debug, Default)]
struct ConstraintState {
    active: bool,
    snapshot: Option<PanelVisibility>,
    changed_by_user: bool,
    first_frame_done: bool,
}

impl ConstraintState {
    fn apply(&mut self, current: PanelVisibility) -> Option<PanelVisibility> {
        self.active = true;
        self.snapshot = Some(current);
        self.changed_by_user = false;
        let target = constrained_visibility(current);
        (target != current).then_some(target)
    }

    fn note_user_change(&mut self) {
        if self.active {
            self.changed_by_user = true;
        }
    }

    fn note_first_frame(&mut self) {
        self.first_frame_done = true;
    }

    /// Whether a collapse is worth telling the user about.
    ///
    /// A window that opens below the breakpoint closes its panels during the
    /// very first layout pass, before anything has been on screen. Announcing
    /// that is a message about a non-event: the user saw no panels close, and
    /// the Undo it offers restores a state they never chose. Every collapse
    /// after the first frame is the consequence of a real resize and is
    /// announced as before.
    fn announces_collapse(&self) -> bool {
        self.first_frame_done
    }

    fn should_enforce_constraint(&self) -> bool {
        self.active && !self.changed_by_user
    }

    fn visibility_change_target(
        &self,
        opened_panel: Panel,
        current: PanelVisibility,
    ) -> Option<PanelVisibility> {
        if !self.should_enforce_constraint() || !opened_panel.is_open(current) {
            return None;
        }
        let target = visibility_after_opening(opened_panel, current);
        (target != current).then_some(target)
    }

    fn undo(&mut self) -> Option<PanelVisibility> {
        if !self.active {
            return None;
        }
        self.note_user_change();
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

fn visibility(library: &adw::OverlaySplitView, now_playing: &Rc<InfoPanel>) -> PanelVisibility {
    PanelVisibility {
        library: library.shows_sidebar(),
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
        let weak_now_playing = Rc::downgrade(now_playing);
        library.connect_show_sidebar_notify(move |_| {
            if applying.get() {
                return;
            }
            let state = state.clone();
            let library = weak_library.clone();
            let now_playing = weak_now_playing.clone();
            let applying = applying.clone();
            gtk4::glib::idle_add_local_once(move || {
                let (Some(library), Some(now_playing)) = (library.upgrade(), now_playing.upgrade())
                else {
                    return;
                };
                // Re-read the live result in the idle callback. Multiple
                // queued notifications then converge on the final state, and
                // closes (including collapse-driven foreign writes) are
                // idempotent non-conflicts rather than user overrides.
                let target = state
                    .borrow()
                    .visibility_change_target(Panel::Library, visibility(&library, &now_playing));
                if let Some(target) = target {
                    set_transient_visibility(&applying, &library, &now_playing, target);
                }
            });
        });
    }
    {
        let state = state.clone();
        let applying = applying.clone();
        let weak_now_playing = Rc::downgrade(now_playing);
        let weak_library = library.downgrade();
        now_playing
            .split_view()
            .connect_show_sidebar_notify(move |_| {
                if applying.get() {
                    return;
                }
                let state = state.clone();
                let now_playing = weak_now_playing.clone();
                let library = weak_library.clone();
                let applying = applying.clone();
                gtk4::glib::idle_add_local_once(move || {
                    let (Some(now_playing), Some(library)) =
                        (now_playing.upgrade(), library.upgrade())
                    else {
                        return;
                    };
                    // As above, the live result makes stale idle callbacks
                    // harmless: only a panel that is still open can conflict
                    // with the enforced mutually-exclusive state.
                    let target = state.borrow().visibility_change_target(
                        Panel::NowPlaying,
                        visibility(&library, &now_playing),
                    );
                    if let Some(target) = target {
                        set_transient_visibility(&applying, &library, &now_playing, target);
                    }
                });
            });
    }
    {
        let state = state.clone();
        let applying = applying.clone();
        let library = library.clone();
        let now_playing = now_playing.clone();
        let overlay = overlay.downgrade();
        let active_toast = active_toast.clone();
        breakpoint.connect_apply(move |_| {
            let current = visibility(&library, &now_playing);
            let announces = state.borrow().announces_collapse();
            let Some(target) = state.borrow_mut().apply(current) else {
                return;
            };
            set_transient_visibility(&applying, &library, &now_playing, target);

            if !announces {
                return;
            }
            let Some(overlay) = overlay.upgrade() else {
                return;
            };
            if let Some(previous) = active_toast.borrow_mut().take() {
                previous.dismiss();
            }
            let toast = crate::ui::toasts::plain(&crate::ui::strings::text(
                crate::ui::strings::INFO_PANEL_CLOSED,
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

    // Layout runs on the frame clock (GDK_PRIORITY_REDRAW), which outranks the
    // default idle - so a breakpoint that fires because the window was born
    // narrow has already applied by the time this runs. From here on, every
    // collapse is one the user caused and gets its toast.
    {
        let state = state.clone();
        gtk4::glib::idle_add_local_once(move || state.borrow_mut().note_first_frame());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_user_change_receivers(source: &str) -> Vec<String> {
        let production = source
            .split_once("#[cfg(test)]")
            .map_or(source, |(production, _)| production);
        let compact: String = production
            .chars()
            .filter(|char| !char.is_whitespace())
            .collect();
        compact
            .split(';')
            .filter_map(|statement| {
                let (before_call, _) = statement.rsplit_once(".note_user_change()")?;
                before_call
                    .rsplit(['{', '}'])
                    .next()
                    .filter(|receiver| !receiver.is_empty())
                    .map(str::to_owned)
            })
            .collect()
    }

    #[test]
    fn style_7_constrained_window_closes_the_info_panel_and_keeps_the_library_sidebar() {
        let open = PanelVisibility {
            library: true,
            now_playing: true,
        };

        assert_eq!(
            constrained_visibility(open),
            PanelVisibility {
                library: true,
                now_playing: false,
            }
        );
    }

    #[test]
    fn style_7_undo_restores_the_exact_pre_snap_panel_state() {
        let mut state = ConstraintState::default();
        let before_snap = PanelVisibility {
            library: true,
            now_playing: true,
        };

        assert_eq!(
            state.apply(before_snap),
            Some(PanelVisibility {
                library: true,
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
            library: true,
            now_playing: true,
        };
        let mut untouched = ConstraintState::default();
        assert_eq!(
            untouched.apply(before_snap),
            Some(PanelVisibility {
                library: true,
                now_playing: false,
            })
        );
        assert_eq!(untouched.unapply(), Some(before_snap));

        let mut changed = ConstraintState::default();
        assert!(changed.apply(before_snap).is_some());
        changed.note_user_change();
        assert_eq!(changed.unapply(), None);
    }

    #[test]
    fn style_7_foreign_close_does_not_cancel_snapshot_restoration() {
        let before_snap = PanelVisibility {
            library: true,
            now_playing: true,
        };
        let mut state = ConstraintState::default();

        assert!(state.apply(before_snap).is_some());
        assert_eq!(
            state.visibility_change_target(
                Panel::Library,
                PanelVisibility {
                    library: false,
                    now_playing: true,
                }
            ),
            None,
            "a collapse-driven close does not conflict with the constraint"
        );
        assert_eq!(state.unapply(), Some(before_snap));
    }

    #[test]
    fn enforcement_latch_is_only_changed_by_undo() {
        // This protects the wiring invariant behind C1: foreign closes must
        // not latch a user override. A ConstraintState unit test cannot catch
        // a note_user_change call reintroduced in either notify handler.
        assert_eq!(
            note_user_change_receivers(include_str!("responsive_side_panels.rs")),
            ["self"]
        );
    }

    #[test]
    fn enforcement_latch_guard_recognizes_the_old_broken_wiring() {
        let broken = concat!(
            "library.connect_show_sidebar_notify(move |_| {\n",
            "    if library.shows_sidebar() {\n",
            "        enforce();\n",
            "    } else {\n",
            "        state.borrow_mut().note_user_",
            "change();\n",
            "    }\n",
            "});\n",
        );

        assert_eq!(note_user_change_receivers(broken), ["state.borrow_mut()"]);
    }

    #[test]
    fn style_7_constrained_entry_without_an_info_panel_close_is_a_non_event() {
        let mut state = ConstraintState::default();
        let library_only = PanelVisibility {
            library: true,
            now_playing: false,
        };

        assert_eq!(state.apply(library_only), None);
        assert_eq!(state.undo(), Some(library_only));
    }

    #[test]
    fn style_7_opening_the_library_sidebar_closes_the_info_panel_while_constrained() {
        let now_playing_only = PanelVisibility {
            library: false,
            now_playing: true,
        };

        assert_eq!(
            visibility_after_opening(Panel::Library, now_playing_only),
            PanelVisibility {
                library: true,
                now_playing: false,
            }
        );
    }

    #[test]
    fn the_info_panel_never_takes_the_library_sidebar_away() {
        for library in [true, false] {
            let current = PanelVisibility {
                library,
                now_playing: true,
            };

            assert_eq!(
                visibility_after_opening(Panel::NowPlaying, current),
                current,
                "opening the info panel changed the library sidebar"
            );
        }

        let mut state = ConstraintState::default();
        assert!(
            state
                .apply(PanelVisibility {
                    library: true,
                    now_playing: false,
                })
                .is_none(),
            "entering the constraint with the info panel closed is a non-event"
        );

        assert_eq!(
            state.visibility_change_target(
                Panel::NowPlaying,
                PanelVisibility {
                    library: true,
                    now_playing: true,
                }
            ),
            None,
            "a constrained window must keep both the sidebar column and the \
             info panel the user just opened"
        );
    }

    #[test]
    fn style_7_panel_and_table_thresholds_stay_coherent() {
        const {
            assert!(
                crate::ui::now_playing_column::INFO_PANEL_COLLAPSE_WIDTH
                    + (SIDEBAR_MIN_WIDTH as i32)
                    < CONSTRAINED_WIDTH,
                "the info panel breakpoint is measured in the content pane, so adding the \
                 pinned sidebar width must keep its effective window threshold below the \
                 window-level constraint"
            );
        }
        assert_eq!(
            CONSTRAINED_WIDTH,
            crate::ui::sidebar_presentation::SIDEBAR_MIN_WIDTH as i32
                + crate::ui::now_playing_column::PANEL_WIDTH
                + crate::ui::track_list::responsive_columns::FOLD_BREAKPOINT_WIDTH,
            "the window threshold must account for both flank widths before the table \
             reaches its fold width"
        );
    }

    #[test]
    fn style_7_default_window_is_not_born_below_its_own_breakpoint() {
        let default = reprise_core::library::session::SessionState::default();

        assert!(
            default.window_width > CONSTRAINED_WIDTH,
            "a fresh profile must not open at a width that closes both side \
             panels during the first layout pass: {} vs {CONSTRAINED_WIDTH}",
            default.window_width,
        );
    }

    #[test]
    fn style_7_a_collapse_before_the_first_frame_is_not_announced() {
        let open = PanelVisibility {
            library: true,
            now_playing: true,
        };

        let born_narrow = ConstraintState::default();
        assert!(
            !born_narrow.announces_collapse(),
            "a window that opens below the breakpoint closes panels the user \
             never saw - there is nothing to announce and nothing to undo"
        );

        let mut resized_by_user = ConstraintState::default();
        resized_by_user.note_first_frame();
        assert!(
            resized_by_user.announces_collapse(),
            "a collapse caused by a real resize keeps its toast"
        );
        assert_eq!(
            resized_by_user.apply(open),
            Some(PanelVisibility {
                library: true,
                now_playing: false,
            }),
            "the announcement gate must not change which panels close"
        );
    }
}
