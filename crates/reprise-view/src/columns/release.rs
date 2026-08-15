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

const DEFAULT_VISIBLE: [ReleaseColumn; 6] = [
    ReleaseColumn::Date,
    ReleaseColumn::Title,
    ReleaseColumn::Artist,
    ReleaseColumn::Type,
    ReleaseColumn::Status,
    ReleaseColumn::Buy,
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

    /// Cover stays pinned because header drag-and-drop recognizes the unnamed
    /// leading column. Status and Buy are hideable and recoverable from the
    /// header popover.
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
    use crate::columns::{ColumnKey, Layout, Pin};

    #[test]
    fn only_the_cover_stays_pinned() {
        for key in ReleaseColumn::all() {
            assert_eq!(ReleaseColumn::parse(key.as_str()), Some(*key));
        }
        assert_eq!(ReleaseColumn::Cover.pin(), Some(Pin::Leading));
        assert_eq!(ReleaseColumn::Status.pin(), None);
        assert_eq!(ReleaseColumn::Buy.pin(), None);
        assert_eq!(ReleaseColumn::Date.pin(), None);
    }

    /// NR-33: the named text columns keep their order; the cover leads them.
    #[test]
    fn nr_33_the_default_release_layout_leads_with_the_cover() {
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
        assert!(layout.visible.contains(&ReleaseColumn::Status));
        assert!(layout.visible.contains(&ReleaseColumn::Buy));
    }

    #[test]
    fn a_layout_stored_before_the_unpinning_keeps_status_and_link_visible() {
        let layout = crate::columns::layout::parse::<ReleaseColumn>(
            "cover,date,title,artist,type,status,buy;date,title,artist,type,status,buy",
        )
        .expect("the stored release layout parses");

        assert!(layout.visible.contains(&ReleaseColumn::Status));
        assert!(layout.visible.contains(&ReleaseColumn::Buy));
    }

    #[test]
    fn a_layout_from_before_these_columns_existed_leaves_them_hidden() {
        let layout = crate::columns::layout::parse::<ReleaseColumn>("cover,date,title;date,title")
            .expect("the old release layout parses");

        assert!(layout.order.contains(&ReleaseColumn::Status));
        assert!(layout.order.contains(&ReleaseColumn::Buy));
        assert!(!layout.visible.contains(&ReleaseColumn::Status));
        assert!(!layout.visible.contains(&ReleaseColumn::Buy));
    }
}
