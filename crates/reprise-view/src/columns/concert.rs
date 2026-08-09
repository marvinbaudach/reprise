//! The concerts table's columns.

use super::key::{ColumnKey, Pin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConcertColumn {
    Date,
    Artist,
    City,
    Venue,
    Distance,
    Tickets,
}

const ALL: [ConcertColumn; 6] = [
    ConcertColumn::Date,
    ConcertColumn::Artist,
    ConcertColumn::City,
    ConcertColumn::Venue,
    ConcertColumn::Distance,
    ConcertColumn::Tickets,
];

const DEFAULT_VISIBLE: [ConcertColumn; 5] = [
    ConcertColumn::Date,
    ConcertColumn::Artist,
    ConcertColumn::City,
    ConcertColumn::Venue,
    ConcertColumn::Distance,
];

impl ColumnKey for ConcertColumn {
    fn as_str(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Artist => "artist",
            Self::City => "city",
            Self::Venue => "venue",
            Self::Distance => "distance",
            Self::Tickets => "tickets",
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
            Self::Tickets => Some(Pin::Trailing),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::{ColumnKey, Layout, Pin};

    #[test]
    fn concert_columns_round_trip_and_pin_the_ticket_action() {
        for key in ConcertColumn::all() {
            assert_eq!(ConcertColumn::parse(key.as_str()), Some(*key));
        }
        assert_eq!(ConcertColumn::Tickets.pin(), Some(Pin::Trailing));
        assert_eq!(ConcertColumn::Date.pin(), None);
    }

    #[test]
    fn the_default_concert_layout_keeps_tickets_trailing() {
        let layout = Layout::<ConcertColumn>::default();
        assert_eq!(
            layout.order,
            vec![
                ConcertColumn::Date,
                ConcertColumn::Artist,
                ConcertColumn::City,
                ConcertColumn::Venue,
                ConcertColumn::Distance,
                ConcertColumn::Tickets,
            ]
        );
    }
}
