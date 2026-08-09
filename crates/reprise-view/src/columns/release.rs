//! The releases table's columns.

use super::key::{ColumnKey, Pin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReleaseColumn {
    Cover,
    Date,
    Title,
    Artist,
    Type,
    Status,
    Buy,
}

const ALL: [ReleaseColumn; 7] = [
    ReleaseColumn::Cover,
    ReleaseColumn::Date,
    ReleaseColumn::Title,
    ReleaseColumn::Artist,
    ReleaseColumn::Type,
    ReleaseColumn::Status,
    ReleaseColumn::Buy,
];

const DEFAULT_VISIBLE: [ReleaseColumn; 4] = [
    ReleaseColumn::Date,
    ReleaseColumn::Title,
    ReleaseColumn::Artist,
    ReleaseColumn::Type,
];

impl ColumnKey for ReleaseColumn {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Date => "date",
            Self::Title => "title",
            Self::Artist => "artist",
            Self::Type => "type",
            Self::Status => "status",
            Self::Buy => "buy",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        ALL.iter().copied().find(|key| key.as_str() == value)
    }

    fn all() -> &'static [Self] {
        &ALL
    }

    fn default_visible() -> &'static [Self] {
        &DEFAULT_VISIBLE
    }

    /// Releases has no row context menu, so Status and Buy are the only access
    /// to hiding a release and to its purchase link. They are pinned for the
    /// same reason Cover is: hiding them would make a function unreachable.
    fn pin(self) -> Option<Pin> {
        match self {
            Self::Cover => Some(Pin::Leading),
            Self::Status | Self::Buy => Some(Pin::Trailing),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::{ColumnKey, Layout, Pin};

    #[test]
    fn release_columns_round_trip_and_pin_their_fixed_ones() {
        for key in ReleaseColumn::all() {
            assert_eq!(ReleaseColumn::parse(key.as_str()), Some(*key));
        }
        assert_eq!(ReleaseColumn::Cover.pin(), Some(Pin::Leading));
        assert_eq!(ReleaseColumn::Status.pin(), Some(Pin::Trailing));
        assert_eq!(ReleaseColumn::Buy.pin(), Some(Pin::Trailing));
        assert_eq!(ReleaseColumn::Date.pin(), None);
    }

    /// NR-25: the named text columns keep their order; the cover leads them.
    #[test]
    fn nr_25_the_default_release_layout_leads_with_the_cover() {
        let layout = Layout::<ReleaseColumn>::default();
        assert_eq!(
            layout.order,
            vec![
                ReleaseColumn::Cover,
                ReleaseColumn::Date,
                ReleaseColumn::Title,
                ReleaseColumn::Artist,
                ReleaseColumn::Type,
                ReleaseColumn::Status,
                ReleaseColumn::Buy,
            ]
        );
    }
}
