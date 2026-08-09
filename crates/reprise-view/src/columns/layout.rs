//! Column order and visibility, and the string they are stored as.
//!
//! The persisted shape is `order;visible`: two comma-separated id lists, the
//! second a subset of the first. Unchanged from the music library's original
//! format, so no stored layout needs migrating.

use std::collections::HashSet;

use super::key::{ColumnKey, Pin};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout<K: ColumnKey> {
    pub order: Vec<K>,
    pub visible: HashSet<K>,
}

impl<K: ColumnKey> Default for Layout<K> {
    fn default() -> Self {
        normalize(
            K::all().to_vec(),
            K::default_visible().iter().copied().collect(),
        )
    }
}

/// Brings any order and visibility set into the one shape the table renders:
/// leading pins, then the free columns in the user's order, then trailing
/// pins, with every pin visible and every unmentioned column appended.
pub fn normalize<K: ColumnKey>(order: Vec<K>, mut visible: HashSet<K>) -> Layout<K> {
    for key in K::all() {
        if key.pin().is_some() {
            visible.insert(*key);
        }
    }
    let mut normalized: Vec<K> = K::all()
        .iter()
        .copied()
        .filter(|key| key.pin() == Some(Pin::Leading))
        .collect();
    for key in order.into_iter().chain(K::all().iter().copied()) {
        if key.pin().is_none() && !normalized.contains(&key) {
            normalized.push(key);
        }
    }
    normalized.extend(
        K::all()
            .iter()
            .copied()
            .filter(|key| key.pin() == Some(Pin::Trailing)),
    );
    Layout {
        order: normalized,
        visible,
    }
}

pub fn serialize<K: ColumnKey>(layout: &Layout<K>) -> String {
    let layout = normalize(layout.order.clone(), layout.visible.clone());
    let order = join(&layout.order);
    let visible: Vec<K> = layout
        .order
        .iter()
        .copied()
        .filter(|key| layout.visible.contains(key))
        .collect();
    format!("{order};{}", join(&visible))
}

pub fn parse<K: ColumnKey>(value: &str) -> Option<Layout<K>> {
    let (order, visible) = value.split_once(';')?;
    Some(normalize(
        parse_ids::<K>(order),
        parse_ids::<K>(visible).into_iter().collect(),
    ))
}

pub fn set_visible<K: ColumnKey>(layout: &Layout<K>, key: K, visible: bool) -> Layout<K> {
    let mut next = layout.clone();
    if visible || key.pin().is_some() {
        next.visible.insert(key);
    } else {
        next.visible.remove(&key);
    }
    normalize(next.order, next.visible)
}

pub fn move_before<K: ColumnKey>(layout: &Layout<K>, key: K, target: K) -> Layout<K> {
    move_relative(layout, key, target, false)
}

pub fn move_after<K: ColumnKey>(layout: &Layout<K>, key: K, target: K) -> Layout<K> {
    move_relative(layout, key, target, true)
}

fn move_relative<K: ColumnKey>(layout: &Layout<K>, key: K, target: K, after: bool) -> Layout<K> {
    if key == target || key.pin().is_some() {
        return layout.clone();
    }
    let mut order = layout.order.clone();
    let Some(source) = order.iter().position(|candidate| *candidate == key) else {
        return layout.clone();
    };
    order.remove(source);
    let Some(index) = order.iter().position(|candidate| *candidate == target) else {
        return layout.clone();
    };
    order.insert(index + usize::from(after), key);
    normalize(order, layout.visible.clone())
}

fn join<K: ColumnKey>(keys: &[K]) -> String {
    keys.iter()
        .map(|key| key.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

/// Unknown ids are skipped rather than failing the whole parse: a layout
/// written by a newer build must not cost an older one its other columns.
/// A repeated id keeps its first occurrence.
fn parse_ids<K: ColumnKey>(value: &str) -> Vec<K> {
    let mut seen: Vec<K> = Vec::new();
    for token in value.split(',') {
        if let Some(key) = K::parse(token.trim()) {
            if !seen.contains(&key) {
                seen.push(key);
            }
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::probe::Probe;

    #[test]
    fn normalize_places_pins_around_the_free_band() {
        let layout = normalize(
            vec![Probe::Trail, Probe::Free, Probe::Lead],
            [Probe::Free].into_iter().collect(),
        );
        assert_eq!(layout.order, vec![Probe::Lead, Probe::Free, Probe::Trail]);
    }

    #[test]
    fn normalize_forces_every_pin_visible() {
        let layout = normalize(Probe::all().to_vec(), std::collections::HashSet::new());
        assert!(layout.visible.contains(&Probe::Lead));
        assert!(layout.visible.contains(&Probe::Trail));
        assert!(!layout.visible.contains(&Probe::Free));
    }

    #[test]
    fn normalize_appends_a_column_the_stored_value_never_mentioned() {
        // A column added in a later release must not become unreachable.
        let layout = normalize(vec![Probe::Lead], [Probe::Lead].into_iter().collect());
        assert!(layout.order.contains(&Probe::Free));
    }

    #[test]
    fn a_layout_round_trips_through_its_persisted_string() {
        let layout = set_visible(&Layout::<Probe>::default(), Probe::Free, false);
        let serialized = serialize(&layout);
        assert_eq!(parse::<Probe>(&serialized), Some(layout));
    }

    #[test]
    fn parse_skips_an_unknown_id_without_losing_the_rest() {
        let layout = parse::<Probe>("lead,gone,free,trail;free").expect("parses");
        assert_eq!(layout.order, vec![Probe::Lead, Probe::Free, Probe::Trail]);
        assert!(layout.visible.contains(&Probe::Free));
    }

    #[test]
    fn a_pin_can_be_neither_hidden_nor_moved() {
        let hidden = set_visible(&Layout::<Probe>::default(), Probe::Lead, false);
        assert!(hidden.visible.contains(&Probe::Lead));
        let moved = move_after(&Layout::<Probe>::default(), Probe::Lead, Probe::Trail);
        assert_eq!(moved.order, Layout::<Probe>::default().order);
    }

    #[test]
    fn moving_a_free_column_reorders_only_the_free_band() {
        let layout = Layout::<Probe>::default();
        let moved = move_before(&layout, Probe::Free, Probe::Free);
        assert_eq!(moved.order, layout.order, "moving onto itself is a no-op");
    }
}
