#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum SelectMode {
    // Kept in the resolver's complete operation vocabulary; GTK performs
    // non-Shift selection directly while the capture seam only observes it.
    #[allow(dead_code)]
    Only,
    #[allow(dead_code)]
    Toggle,
    Range,
    RangeAdditive,
}

/// A row held as both position and id. The position is what range
/// calculations use -- a playlist may contain the same track more than once,
/// so its deletion path deliberately works with positions too. The id exists
/// only so `validate` can detect that sorting, filtering, or reloading put a
/// different row at that position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) struct Anchored<Id> {
    pub position: u32,
    pub id: Id,
}

/// `anchor` is the fixed start of a range; `cursor` is its moving end and our
/// copy of GTK's focused row, which GTK4 does not expose publicly on a
/// `ColumnView`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ui) struct AnchorState<Id> {
    pub anchor: Option<Anchored<Id>>,
    pub cursor: Option<Anchored<Id>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum SelectionOp {
    SelectOnly(u32),
    Toggle(u32),
    SelectRange { start: u32, len: u32, replace: bool },
}

pub(in crate::ui) fn validate<Id: PartialEq>(
    state: AnchorState<Id>,
    lookup: impl Fn(u32) -> Option<Id>,
) -> AnchorState<Id> {
    let keep = |candidate: Option<Anchored<Id>>| {
        candidate.filter(|held| lookup(held.position).as_ref() == Some(&held.id))
    };
    AnchorState {
        anchor: keep(state.anchor),
        cursor: keep(state.cursor),
    }
}

/// Resolves a selection input from the user-owned anchor. The track list can
/// pass its playing row as `fallback`; tables without playback pass `None`.
pub(in crate::ui) fn resolve<Id: Clone>(
    state: AnchorState<Id>,
    fallback: Option<Anchored<Id>>,
    target: Anchored<Id>,
    mode: SelectMode,
) -> (SelectionOp, AnchorState<Id>) {
    match mode {
        SelectMode::Only | SelectMode::Toggle => {
            let op = if mode == SelectMode::Only {
                SelectionOp::SelectOnly(target.position)
            } else {
                SelectionOp::Toggle(target.position)
            };
            let moved = AnchorState {
                anchor: Some(target.clone()),
                cursor: Some(target),
            };
            (op, moved)
        }
        SelectMode::Range | SelectMode::RangeAdditive => {
            // Without a user-owned anchor, the fallback is the range anchor.
            // If neither exists, a range is meaningless and this becomes a
            // plain click. This is NAV-17's core: GTK would stretch from row
            // zero here.
            let Some(anchor) = state.anchor.or(fallback) else {
                let position = target.position;
                let moved = AnchorState {
                    anchor: Some(target.clone()),
                    cursor: Some(target),
                };
                return (SelectionOp::SelectOnly(position), moved);
            };
            let (start, end) = if anchor.position <= target.position {
                (anchor.position, target.position)
            } else {
                (target.position, anchor.position)
            };
            let op = SelectionOp::SelectRange {
                start,
                len: end - start + 1,
                replace: matches!(mode, SelectMode::Range),
            };
            // A range never moves the anchor. The next input starts there
            // again rather than extending from the previous result.
            (
                op,
                AnchorState {
                    anchor: Some(anchor),
                    cursor: Some(target),
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct CountedId(std::rc::Rc<std::cell::Cell<usize>>);

    impl Clone for CountedId {
        fn clone(&self) -> Self {
            self.0.set(self.0.get() + 1);
            Self(self.0.clone())
        }
    }

    fn at(position: u32) -> Anchored<i64> {
        Anchored {
            position,
            id: i64::from(position) + 1_000,
        }
    }

    /// In this model, row n carries track id n + 1000.
    fn stable(position: u32) -> Option<i64> {
        (position < 100).then(|| i64::from(position) + 1_000)
    }

    #[test]
    fn a_range_with_an_anchor_does_not_clone_the_non_copy_target_id() {
        let target_clones = std::rc::Rc::new(std::cell::Cell::new(0));
        let state = AnchorState {
            anchor: Some(Anchored {
                position: 2,
                id: CountedId(std::rc::Rc::new(std::cell::Cell::new(0))),
            }),
            cursor: None,
        };
        let target = Anchored {
            position: 7,
            id: CountedId(target_clones.clone()),
        };

        let _ = resolve(state, None, target, SelectMode::Range);

        assert_eq!(target_clones.get(), 0);
    }

    #[test]
    fn nav_17_a_plain_click_sets_both_anchor_and_cursor() {
        let (op, state) = resolve(AnchorState::default(), None, at(7), SelectMode::Only);
        assert_eq!(op, SelectionOp::SelectOnly(7));
        assert_eq!(state.anchor, Some(at(7)));
        assert_eq!(state.cursor, Some(at(7)));
    }

    #[test]
    fn nav_17_a_toggle_moves_both_too() {
        let start = AnchorState {
            anchor: Some(at(3)),
            cursor: Some(at(3)),
        };
        let (op, state) = resolve(start, None, at(9), SelectMode::Toggle);
        assert_eq!(op, SelectionOp::Toggle(9));
        assert_eq!(state.anchor, Some(at(9)));
        assert_eq!(state.cursor, Some(at(9)));
    }

    #[test]
    fn nav_17_a_range_never_moves_the_anchor() {
        let start = AnchorState {
            anchor: Some(at(4)),
            cursor: Some(at(4)),
        };
        let (op, state) = resolve(start, None, at(8), SelectMode::Range);
        assert_eq!(
            op,
            SelectionOp::SelectRange {
                start: 4,
                len: 5,
                replace: true
            }
        );
        assert_eq!(state.anchor, Some(at(4)), "the anchor stays fixed");
        assert_eq!(state.cursor, Some(at(8)), "only the cursor follows");

        // A second range starts from the anchor again rather than extending
        // from the previous result.
        let (op, _) = resolve(state, None, at(2), SelectMode::Range);
        assert_eq!(
            op,
            SelectionOp::SelectRange {
                start: 2,
                len: 3,
                replace: true
            }
        );
    }

    #[test]
    fn nav_17_a_range_without_any_anchor_selects_a_single_row() {
        let (op, state) = resolve(AnchorState::default(), None, at(42), SelectMode::Range);
        assert_eq!(
            op,
            SelectionOp::SelectOnly(42),
            "do not stretch from row zero"
        );
        assert_eq!(
            state.anchor,
            Some(at(42)),
            "the click establishes the anchor"
        );
        assert_eq!(state.cursor, Some(at(42)));
    }

    #[test]
    fn nav_17_a_range_without_an_anchor_starts_at_the_playing_row() {
        let (op, state) = resolve(
            AnchorState::default(),
            Some(at(5)),
            at(9),
            SelectMode::Range,
        );
        assert_eq!(
            op,
            SelectionOp::SelectRange {
                start: 5,
                len: 5,
                replace: true
            }
        );
        assert_eq!(
            state.anchor,
            Some(at(5)),
            "the playing row becomes the anchor"
        );
        assert_eq!(state.cursor, Some(at(9)));
    }

    #[test]
    fn nav_17_an_own_anchor_beats_the_playing_row() {
        let start = AnchorState {
            anchor: Some(at(20)),
            cursor: Some(at(20)),
        };
        let (op, _) = resolve(start, Some(at(5)), at(22), SelectMode::Range);
        assert_eq!(
            op,
            SelectionOp::SelectRange {
                start: 20,
                len: 3,
                replace: true
            }
        );
    }

    #[test]
    fn nav_17_a_backwards_range_is_ordered() {
        let start = AnchorState {
            anchor: Some(at(30)),
            cursor: Some(at(30)),
        };
        let (op, _) = resolve(start, None, at(25), SelectMode::Range);
        assert_eq!(
            op,
            SelectionOp::SelectRange {
                start: 25,
                len: 6,
                replace: true
            }
        );
    }

    #[test]
    fn nav_17_an_additive_range_keeps_the_rest_of_the_selection() {
        let start = AnchorState {
            anchor: Some(at(4)),
            cursor: Some(at(4)),
        };
        let (op, _) = resolve(start, None, at(6), SelectMode::RangeAdditive);
        assert_eq!(
            op,
            SelectionOp::SelectRange {
                start: 4,
                len: 3,
                replace: false
            }
        );
    }

    #[test]
    fn nav_17_a_stale_anchor_is_dropped_and_the_playing_row_takes_over() {
        // Sorting changed row 4 to a different track.
        let start = AnchorState {
            anchor: Some(Anchored {
                position: 4,
                id: 999,
            }),
            cursor: Some(Anchored {
                position: 4,
                id: 999,
            }),
        };
        let validated = validate(start, stable);
        assert_eq!(validated, AnchorState::default());

        let (op, _) = resolve(validated, Some(at(5)), at(9), SelectMode::Range);
        assert_eq!(
            op,
            SelectionOp::SelectRange {
                start: 5,
                len: 5,
                replace: true
            }
        );
    }

    #[test]
    fn nav_17_an_anchor_past_the_end_is_dropped() {
        let start = AnchorState {
            anchor: Some(at(500)),
            cursor: Some(at(500)),
        };
        assert_eq!(validate(start, stable), AnchorState::default());
    }

    #[test]
    fn nav_17_a_live_anchor_survives_validation() {
        let start = AnchorState {
            anchor: Some(at(4)),
            cursor: Some(at(8)),
        };
        assert_eq!(validate(start, stable), start);
    }

    #[test]
    fn nav_17_validation_drops_each_half_on_its_own() {
        let start = AnchorState {
            anchor: Some(at(4)),
            cursor: Some(Anchored {
                position: 8,
                id: 999,
            }),
        };
        let validated = validate(start, stable);
        assert_eq!(validated.anchor, Some(at(4)));
        assert_eq!(validated.cursor, None);
    }
}
