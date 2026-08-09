//! The radio table's columns.

use super::key::{ColumnKey, Pin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RadioColumn {
    Artwork,
    State,
    Station,
    Genre,
    Bitrate,
    Country,
    NowPlaying,
}

const ALL: [RadioColumn; 7] = [
    RadioColumn::Artwork,
    RadioColumn::State,
    RadioColumn::Station,
    RadioColumn::Genre,
    RadioColumn::Bitrate,
    RadioColumn::Country,
    RadioColumn::NowPlaying,
];

const DEFAULT_VISIBLE: [RadioColumn; 5] = [
    RadioColumn::Station,
    RadioColumn::Genre,
    RadioColumn::Bitrate,
    RadioColumn::Country,
    RadioColumn::NowPlaying,
];

impl ColumnKey for RadioColumn {
    fn as_str(self) -> &'static str {
        match self {
            Self::Artwork => "artwork",
            Self::State => "state",
            Self::Station => "station",
            Self::Genre => "genre",
            Self::Bitrate => "bitrate",
            Self::Country => "country",
            Self::NowPlaying => "now-playing",
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

    fn pin(self) -> Option<Pin> {
        match self {
            Self::Artwork | Self::State => Some(Pin::Leading),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::{ColumnKey, Layout, Pin};

    #[test]
    fn radio_columns_round_trip_and_pin_their_leading_cells() {
        for key in RadioColumn::all() {
            assert_eq!(RadioColumn::parse(key.as_str()), Some(*key));
        }
        assert_eq!(RadioColumn::Artwork.pin(), Some(Pin::Leading));
        assert_eq!(RadioColumn::State.pin(), Some(Pin::Leading));
        assert_eq!(RadioColumn::Station.pin(), None);
    }

    #[test]
    fn the_default_radio_layout_keeps_both_leading_cells_first() {
        let layout = Layout::<RadioColumn>::default();
        assert_eq!(
            layout.order,
            vec![
                RadioColumn::Artwork,
                RadioColumn::State,
                RadioColumn::Station,
                RadioColumn::Genre,
                RadioColumn::Bitrate,
                RadioColumn::Country,
                RadioColumn::NowPlaying,
            ]
        );
    }
}
