//! Serialization for a table's user-adjusted column widths, persisted
//! independently of the order/visibility layout. Format is a comma-separated list of
//! `id:width` pairs, e.g. `artist:260,album:300`.
//!
//! Shared rather than GTK-local because the string it produces is a
//! *persisted* user setting: a GTK app and a Tauri desktop read the same
//! stored value, and two implementations of one format drift. A phone-sized
//! surface has no resizable columns and simply never calls this — that makes
//! it unused there, not surface-specific (waves plan §4 rule 12).

use crate::columns::ColumnKey;

/// Encodes the given widths as `id:width` pairs, sorted by id for a stable,
/// diff-friendly string. Non-positive widths are dropped.
pub fn serialize_widths<K: ColumnKey>(widths: &[(K, i32)]) -> String {
    let mut pairs: Vec<(&'static str, i32)> = widths
        .iter()
        .filter(|(_, width)| *width > 0)
        .map(|(id, width)| (id.as_str(), *width))
        .collect();
    pairs.sort_by_key(|(id, _)| *id);
    pairs
        .iter()
        .map(|(id, width)| format!("{id}:{width}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Parses an `id:width` list, skipping unknown ids and non-positive or
/// unparseable widths. The first value wins if an id repeats.
pub fn parse_widths<K: ColumnKey>(value: &str) -> Vec<(K, i32)> {
    let mut widths: Vec<(K, i32)> = Vec::new();
    for token in value.split(',') {
        let Some((id, width)) = token.trim().split_once(':') else {
            continue;
        };
        let Some(id) = K::parse(id.trim()) else {
            continue;
        };
        let Ok(width) = width.trim().parse::<i32>() else {
            continue;
        };
        if width <= 0 || widths.iter().any(|(seen, _)| *seen == id) {
            continue;
        }
        widths.push((id, width));
    }
    widths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::ColumnId;

    #[test]
    fn round_trips_sorted_pairs() {
        let widths = vec![(ColumnId::Album, 300), (ColumnId::Artist, 260)];
        let serialized = serialize_widths(&widths);
        assert_eq!(serialized, "album:300,artist:260");
        assert_eq!(
            parse_widths(&serialized),
            vec![(ColumnId::Album, 300), (ColumnId::Artist, 260)]
        );
    }

    #[test]
    fn serialize_drops_non_positive_widths() {
        let widths = vec![
            (ColumnId::Artist, 0),
            (ColumnId::Album, -5),
            (ColumnId::Year, 90),
        ];
        assert_eq!(serialize_widths(&widths), "year:90");
    }

    #[test]
    fn parse_skips_unknown_ids_and_bad_values() {
        assert_eq!(
            parse_widths("artist:260,banana:100,album:xyz,year:0,duration:100"),
            vec![(ColumnId::Artist, 260), (ColumnId::Duration, 100)]
        );
    }

    #[test]
    fn parse_tolerates_empty_and_whitespace() {
        assert_eq!(parse_widths::<ColumnId>(""), Vec::new());
        assert_eq!(parse_widths::<ColumnId>("  "), Vec::new());
        assert_eq!(parse_widths("artist:260"), vec![(ColumnId::Artist, 260)]);
    }

    #[test]
    fn parse_keeps_first_value_on_duplicate_id() {
        assert_eq!(
            parse_widths("artist:260,artist:999"),
            vec![(ColumnId::Artist, 260)]
        );
    }

    #[test]
    fn widths_round_trip_for_a_second_table() {
        use crate::columns::ReleaseColumn;
        let widths = vec![(ReleaseColumn::Artist, 260), (ReleaseColumn::Date, 160)];
        let serialized = serialize_widths(&widths);
        assert_eq!(serialized, "artist:260,date:160");
        assert_eq!(parse_widths::<ReleaseColumn>(&serialized), widths);
    }
}
