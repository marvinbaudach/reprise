//! The concerts table's columns.

use super::key::{ColumnKey, Pin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConcertColumn {
    Cover,
    Date,
    Artist,
    City,
    Venue,
    Distance,
    Tickets,
    Source,
}

const ALL: [ConcertColumn; 8] = [
    ConcertColumn::Cover,
    ConcertColumn::Artist,
    ConcertColumn::Date,
    ConcertColumn::City,
    ConcertColumn::Venue,
    ConcertColumn::Distance,
    ConcertColumn::Tickets,
    ConcertColumn::Source,
];

const DEFAULT_VISIBLE: [ConcertColumn; 5] = [
    ConcertColumn::Artist,
    ConcertColumn::Date,
    ConcertColumn::City,
    ConcertColumn::Distance,
    ConcertColumn::Tickets,
];

impl ColumnKey for ConcertColumn {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Date => "date",
            Self::Artist => "artist",
            Self::City => "city",
            Self::Venue => "venue",
            Self::Distance => "distance",
            Self::Tickets => "tickets",
            Self::Source => "source",
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
            Self::Cover => Some(Pin::Leading),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::{ColumnKey, Layout};

    #[test]
    fn concert_columns_round_trip_without_pinning_status_or_source() {
        for key in ConcertColumn::all() {
            assert_eq!(ConcertColumn::parse(key.as_str()), Some(*key));
        }
        assert_eq!(ConcertColumn::Tickets.pin(), None);
        assert_eq!(ConcertColumn::Source.pin(), None);
        assert_eq!(ConcertColumn::Date.pin(), None);
    }

    #[test]
    fn conc_17a_the_default_concert_layout_leads_with_the_cover() {
        let layout = Layout::<ConcertColumn>::default();
        assert_eq!(
            layout.order,
            vec![
                ConcertColumn::Cover,
                ConcertColumn::Artist,
                ConcertColumn::Date,
                ConcertColumn::City,
                ConcertColumn::Venue,
                ConcertColumn::Distance,
                ConcertColumn::Tickets,
                ConcertColumn::Source,
            ]
        );
        assert!(layout.visible.contains(&ConcertColumn::Cover));
        assert!(layout.visible.contains(&ConcertColumn::Artist));
        assert!(layout.visible.contains(&ConcertColumn::Date));
        assert!(layout.visible.contains(&ConcertColumn::City));
        assert!(layout.visible.contains(&ConcertColumn::Distance));
        assert!(layout.visible.contains(&ConcertColumn::Tickets));
        assert!(!layout.visible.contains(&ConcertColumn::Venue));
        assert!(!layout.visible.contains(&ConcertColumn::Source));
    }

    #[test]
    fn a_concert_layout_stored_before_the_cover_gains_it_at_the_leading_edge() {
        let layout = crate::columns::layout::parse::<ConcertColumn>(
            "artist,date,city,venue,distance,tickets,source;artist,date,city,distance,tickets",
        )
        .expect("the stored concert layout parses");

        assert_eq!(layout.order.first(), Some(&ConcertColumn::Cover));
        assert!(layout.visible.contains(&ConcertColumn::Cover));
    }
}
