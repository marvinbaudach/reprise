//! Selection arithmetic shared by the app's multi-select tables.
//!
//! Widget-free on purpose: the anchor rule (NAV-17) and the pointer/key
//! modifier reading are the same question in the track list and in the
//! releases table, and a rule with two homes drifts. Everything that touches
//! a `SelectionModel` stays in the table that owns it.

mod anchor;
mod input;

pub(in crate::ui) use anchor::{resolve, validate, AnchorState, Anchored, SelectMode, SelectionOp};
pub(in crate::ui) use input::{key_intent, pointer_mode, KeyIntent};
