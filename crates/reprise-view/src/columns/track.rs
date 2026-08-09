//! Toolkit-independent track-column identity and persistence names.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnId {
    Cover,
    Title,
    TrackNumber,
    Artist,
    Album,
    Genre,
    Year,
    Added,
    Duration,
    Rating,
    PlayCount,
}

impl ColumnId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Title => "title",
            Self::TrackNumber => "track-number",
            Self::Artist => "artist",
            Self::Album => "album",
            Self::Genre => "genre",
            Self::Year => "year",
            Self::Added => "added",
            Self::Duration => "duration",
            Self::Rating => "rating",
            Self::PlayCount => "play-count",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "cover" => Some(Self::Cover),
            "title" => Some(Self::Title),
            "track-number" => Some(Self::TrackNumber),
            "artist" => Some(Self::Artist),
            "album" => Some(Self::Album),
            "genre" => Some(Self::Genre),
            "year" => Some(Self::Year),
            "added" => Some(Self::Added),
            "duration" => Some(Self::Duration),
            "rating" => Some(Self::Rating),
            "play-count" => Some(Self::PlayCount),
            _ => None,
        }
    }

    pub fn from_sort_field(field: &str) -> Option<Self> {
        match field {
            "title" => Some(Self::Title),
            "track_no" => Some(Self::TrackNumber),
            "artist" => Some(Self::Artist),
            "album" => Some(Self::Album),
            "genre" => Some(Self::Genre),
            "year" => Some(Self::Year),
            "added_at" => Some(Self::Added),
            "duration_ms" => Some(Self::Duration),
            "rating" => Some(Self::Rating),
            "play_count" => Some(Self::PlayCount),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ColumnId;

    const IDS: [(ColumnId, &str); 11] = [
        (ColumnId::Cover, "cover"),
        (ColumnId::Title, "title"),
        (ColumnId::TrackNumber, "track-number"),
        (ColumnId::Artist, "artist"),
        (ColumnId::Album, "album"),
        (ColumnId::Genre, "genre"),
        (ColumnId::Year, "year"),
        (ColumnId::Added, "added"),
        (ColumnId::Duration, "duration"),
        (ColumnId::Rating, "rating"),
        (ColumnId::PlayCount, "play-count"),
    ];

    #[test]
    fn persistence_names_round_trip() {
        for (id, name) in IDS {
            assert_eq!(id.as_str(), name);
            assert_eq!(ColumnId::parse(name), Some(id));
        }
        assert_eq!(ColumnId::parse("unknown"), None);
    }

    #[test]
    fn sort_fields_map_only_to_sortable_columns() {
        let sortable = [
            ("title", ColumnId::Title),
            ("track_no", ColumnId::TrackNumber),
            ("artist", ColumnId::Artist),
            ("album", ColumnId::Album),
            ("genre", ColumnId::Genre),
            ("year", ColumnId::Year),
            ("added_at", ColumnId::Added),
            ("duration_ms", ColumnId::Duration),
            ("rating", ColumnId::Rating),
            ("play_count", ColumnId::PlayCount),
        ];

        for (field, id) in sortable {
            assert_eq!(ColumnId::from_sort_field(field), Some(id));
        }
        assert_eq!(ColumnId::from_sort_field("cover"), None);
        assert_eq!(ColumnId::from_sort_field("unknown"), None);
    }
}
