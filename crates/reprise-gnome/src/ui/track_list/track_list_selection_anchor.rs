//! NAV-17: the track list's selection anchor.
//!
//! GTK's `GtkListBase` keeps an internal anchor that only a click or focus
//! movement sets. NAV-10b forbids playback from doing either, so GTK's anchor
//! stays behind when playback starts -- at row zero after a view change -- and
//! Shift+click stretches across half the library. This module therefore keeps
//! the anchor itself.
//!
//! Pure logic, no GTK: `validate` discards stale positions and `resolve`
//! decides. The playing track is never stored; `resolve` receives it as a
//! fallback so playback cannot move the anchor behind the user's back. This is
//! closely related to `podcasts_selection::apply_select`, which applies the
//! same anchor discipline to episode rows.

// Task 1 establishes the tested seam before Tasks 2-4 wire its production
// consumers.
#![cfg_attr(not(test), allow(dead_code))]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SelectMode {
    Only,
    Toggle,
    Range,
    RangeAdditive,
}

/// A row held as both position and track id. The position is what range
/// calculations use -- a playlist may contain the same track more than once,
/// so its deletion path deliberately works with positions too. The id exists
/// only so `validate` can detect that sorting, filtering, or reloading put a
/// different row at that position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Anchored {
    pub position: u32,
    pub track_id: i64,
}

/// `anchor` is the fixed start of a range; `cursor` is its moving end and our
/// copy of GTK's focused row, which GTK4 does not expose publicly on a
/// `ColumnView`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct AnchorState {
    pub anchor: Option<Anchored>,
    pub cursor: Option<Anchored>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SelectionOp {
    SelectOnly(u32),
    Toggle(u32),
    SelectRange { start: u32, len: u32, replace: bool },
}

pub(super) fn validate(state: AnchorState, lookup: impl Fn(u32) -> Option<i64>) -> AnchorState {
    let keep = |candidate: Option<Anchored>| {
        candidate.filter(|held| lookup(held.position) == Some(held.track_id))
    };
    AnchorState {
        anchor: keep(state.anchor),
        cursor: keep(state.cursor),
    }
}

pub(super) fn resolve(
    state: AnchorState,
    playing: Option<Anchored>,
    target: Anchored,
    mode: SelectMode,
) -> (SelectionOp, AnchorState) {
    let moved = AnchorState {
        anchor: Some(target),
        cursor: Some(target),
    };
    match mode {
        SelectMode::Only => (SelectionOp::SelectOnly(target.position), moved),
        SelectMode::Toggle => (SelectionOp::Toggle(target.position), moved),
        SelectMode::Range | SelectMode::RangeAdditive => {
            // Without a user-owned anchor, the playing track is the range
            // fallback. If neither exists, a range is meaningless and this
            // becomes a plain click. This is NAV-17's core: GTK would stretch
            // from row zero here.
            let Some(anchor) = state.anchor.or(playing) else {
                return (SelectionOp::SelectOnly(target.position), moved);
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

use super::track_list::Shared;

/// Reads the stored anchor and discards anything made stale by sorting,
/// filtering, or reloading. Every read goes through this function, keeping
/// invalidation in one place instead of attaching it to every model rebuild.
pub(super) fn live_anchor_state(shared: &Shared) -> AnchorState {
    let state = validate(shared.selection_anchor.get(), |position| {
        shared.model.track_at(position).map(|track| track.id)
    });
    shared.selection_anchor.set(state);
    state
}

pub(super) fn store_anchor_state(shared: &Shared, state: AnchorState) {
    shared.selection_anchor.set(state);
}

pub(super) fn anchored_at(shared: &Shared, position: u32) -> Option<Anchored> {
    shared.model.track_at(position).map(|track| Anchored {
        position,
        track_id: track.id,
    })
}

/// Resolves the playing track as a fallback anchor at input time and never
/// stores it. That preserves NAV-10b: playback writes no selection state and
/// therefore cannot move the anchor behind the user's back.
#[cfg_attr(test, allow(dead_code))]
pub(super) fn playing_anchor(shared: &Shared) -> Option<Anchored> {
    let track_id = shared.playing_track_id.get()?;
    let ids = shared.current_view_ids();
    let is_queue = matches!(
        *shared.source.borrow(),
        reprise_core::view_source::ViewSource::Queue
    );
    let position = super::current_track_selection::visible_position_for_track_in_source(
        &ids, track_id, None, is_queue,
    )?;
    Some(Anchored { position, track_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(position: u32) -> Anchored {
        Anchored {
            position,
            track_id: i64::from(position) + 1_000,
        }
    }

    /// In this model, row n carries track id n + 1000.
    fn stable(position: u32) -> Option<i64> {
        (position < 100).then(|| i64::from(position) + 1_000)
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
                track_id: 999,
            }),
            cursor: Some(Anchored {
                position: 4,
                track_id: 999,
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
                track_id: 999,
            }),
        };
        let validated = validate(start, stable);
        assert_eq!(validated.anchor, Some(at(4)));
        assert_eq!(validated.cursor, None);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nav_17_a_reload_drops_a_stale_anchor_against_the_real_model() {
        gtk4::init().unwrap();
        let conn = crate::test_db::open().unwrap();
        let fixture_conn = crate::test_db::connection(&conn);
        let tx = fixture_conn.unchecked_transaction().unwrap();
        for id in 1..=20 {
            tx.execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) \
                 VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
                (
                    id,
                    format!("/synthetic/{id:03}.flac"),
                    format!("Track {id:03}"),
                ),
            )
            .unwrap();
        }
        tx.commit().unwrap();
        let track_list = crate::ui::track_list::TrackList::new(
            std::rc::Rc::new(conn),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        );
        let shared = &track_list.shared;

        let real = anchored_at(shared, 3).expect("row 3 exists");
        store_anchor_state(
            shared,
            AnchorState {
                anchor: Some(real),
                cursor: Some(real),
            },
        );
        assert_eq!(live_anchor_state(shared).anchor, Some(real));

        // The position exists, but now claims an id that is not at that row.
        let stale = Anchored {
            position: 3,
            track_id: real.track_id + 5_000,
        };
        store_anchor_state(
            shared,
            AnchorState {
                anchor: Some(stale),
                cursor: Some(stale),
            },
        );
        assert_eq!(
            live_anchor_state(shared),
            AnchorState::default(),
            "an anchor whose row carries another track is discarded"
        );
    }
}
