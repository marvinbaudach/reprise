use std::collections::{BTreeMap, HashMap};

use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

/// The metadata family being grouped by the stats screen.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GroupKind {
    Artist,
    AlbumArtist,
    Genre,
}

/// Runtime-only grouping key. This value is never written back to a tag.
pub fn normalize_group_key(raw: &str) -> String {
    let lowered = raw.trim().to_lowercase();
    let collapsed = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .collect()
}

/// One raw observation feeding the grouping fold.
#[derive(Clone, Copy, Debug)]
pub struct GroupInput<'a> {
    pub raw: &'a str,
    pub mbid: Option<&'a str>,
    pub plays: i64,
    pub ms: i64,
    pub last_played_at: i64,
}

/// A deterministic aggregate over equivalent raw metadata spellings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Group {
    pub label: String,
    pub key: String,
    pub plays: i64,
    pub ms: i64,
    pub variant_count: usize,
}

#[derive(Default)]
struct LabelStats {
    plays: i64,
    last_played_at: i64,
}

struct GroupAccumulator {
    key: String,
    plays: i64,
    ms: i64,
    labels: BTreeMap<String, LabelStats>,
}

impl GroupAccumulator {
    fn new(key: String) -> Self {
        Self {
            key,
            plays: 0,
            ms: 0,
            labels: BTreeMap::new(),
        }
    }

    fn add(&mut self, row: &GroupInput<'_>) {
        self.plays += row.plays;
        self.ms += row.ms;
        let label = self.labels.entry(row.raw.to_string()).or_default();
        label.plays += row.plays;
        label.last_played_at = label.last_played_at.max(row.last_played_at);
    }

    fn finish(self) -> Group {
        let variant_count = self.labels.len();
        let label = self
            .labels
            .into_iter()
            .min_by(|(left_raw, left), (right_raw, right)| {
                right
                    .plays
                    .cmp(&left.plays)
                    .then_with(|| right.last_played_at.cmp(&left.last_played_at))
                    .then_with(|| left_raw.cmp(right_raw))
            })
            .map(|(raw, _)| raw)
            .unwrap_or_default();
        Group {
            label,
            key: self.key,
            plays: self.plays,
            ms: self.ms,
            variant_count,
        }
    }
}

/// Folds raw rows into exact runtime groups and returns a total display order.
pub fn fold_groups(rows: &[GroupInput<'_>]) -> Vec<Group> {
    let mut accumulators = HashMap::<String, GroupAccumulator>::new();
    for row in rows {
        let key = match row.mbid.map(str::trim).filter(|mbid| !mbid.is_empty()) {
            Some(mbid) => format!("mbid:{mbid}"),
            None => format!("name:{}", normalize_group_key(row.raw)),
        };
        accumulators
            .entry(key.clone())
            .or_insert_with(|| GroupAccumulator::new(key))
            .add(row);
    }

    let mut groups = accumulators
        .into_values()
        .map(GroupAccumulator::finish)
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        right
            .ms
            .cmp(&left.ms)
            .then_with(|| right.plays.cmp(&left.plays))
            .then_with(|| left.label.cmp(&right.label))
            .then_with(|| left.key.cmp(&right.key))
    });
    groups
}

#[cfg(test)]
#[path = "group_key_tests.rs"]
mod tests;
