//! What a table's column identity has to provide.

use std::hash::Hash;

/// A column the user may not move or hide, and where it sits.
///
/// Two kinds exist in Reprise: a leading artwork column that opens the row,
/// and a trailing action column that is the only access to an action on
/// surfaces without a row context menu. Both stay visible, keep their
/// position, and never appear in the column editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pin {
    Leading,
    Trailing,
}

/// The identity of one table's columns.
///
/// `as_str` is a *persisted* name: changing one silently discards every
/// stored layout that mentions it.
pub trait ColumnKey: Copy + Eq + Hash + Sized + 'static {
    fn as_str(self) -> &'static str;
    fn parse(value: &str) -> Option<Self>;
    /// Every column of this table, in the built-in default order.
    fn all() -> &'static [Self];
    /// The columns a fresh layout shows. Pins are visible regardless of
    /// whether they are listed here.
    fn default_visible() -> &'static [Self];
    fn pin(self) -> Option<Pin>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    enum Probe {
        Lead,
        Free,
        Trail,
    }

    impl ColumnKey for Probe {
        fn as_str(self) -> &'static str {
            match self {
                Self::Lead => "lead",
                Self::Free => "free",
                Self::Trail => "trail",
            }
        }
        fn parse(value: &str) -> Option<Self> {
            match value {
                "lead" => Some(Self::Lead),
                "free" => Some(Self::Free),
                "trail" => Some(Self::Trail),
                _ => None,
            }
        }
        fn all() -> &'static [Self] {
            &[Self::Lead, Self::Free, Self::Trail]
        }
        fn default_visible() -> &'static [Self] {
            &[Self::Free]
        }
        fn pin(self) -> Option<Pin> {
            match self {
                Self::Lead => Some(Pin::Leading),
                Self::Trail => Some(Pin::Trailing),
                Self::Free => None,
            }
        }
    }

    #[test]
    fn every_key_round_trips_through_its_persisted_name() {
        for key in Probe::all() {
            assert_eq!(Probe::parse(key.as_str()), Some(*key));
        }
        assert_eq!(Probe::parse("unknown"), None);
    }
}
