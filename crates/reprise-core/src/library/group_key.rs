use std::collections::{BTreeMap, BTreeSet, HashMap};

use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

/// The metadata family being grouped by the stats screen.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GroupKind {
    Artist,
    AlbumArtist,
    Genre,
}

/// Runtime-only grouping key. This value is never written back to a tag.
///
/// `to_lowercase` is Unicode lowercasing, deliberately not full case folding:
/// std has no `to_casefold`, and the only Latin-script difference that would
/// buy is "Straße" == "STRASSE" — a genuinely different spelling, not a casing
/// of the same one. STATS-9 states this limit rather than implying a fold the
/// code does not perform. Note that "STRAẞE" (U+1E9E) *does* fold, because
/// `to_lowercase` maps capital sharp s to `ß`.
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

/// The one place a raw metadata spelling becomes a group key.
///
/// Names fold first (STATS-9 stage 2), and an MBID is only consulted *inside*
/// the resulting name group, where it becomes that group's stable identity and
/// merges any other name group carrying the same identity. Resolving MBIDs per
/// raw spelling — as an earlier revision did — splits exactly what the name
/// fold just merged, because sparse MBIDs typically hang off a single spelling.
///
/// Build it once per domain and use it everywhere that domain is keyed:
/// aggregates, spotlight and the track lookup must never disagree.
#[derive(Clone, Debug, Default)]
pub struct KeyResolver {
    keys_by_name: HashMap<String, String>,
    names_by_mbid: HashMap<String, BTreeSet<String>>,
}

impl KeyResolver {
    pub fn build<'a>(rows: impl IntoIterator<Item = GroupInput<'a>>) -> Self {
        let mut plays_by_name_and_mbid = HashMap::<String, BTreeMap<String, i64>>::new();
        let mut names_by_mbid = HashMap::<String, BTreeSet<String>>::new();
        for row in rows {
            let name = normalize_group_key(row.raw);
            let by_mbid = plays_by_name_and_mbid.entry(name.clone()).or_default();
            let Some(mbid) = row.mbid.map(str::trim).filter(|mbid| !mbid.is_empty()) else {
                continue;
            };
            *by_mbid.entry(mbid.to_string()).or_default() += row.plays;
            names_by_mbid
                .entry(mbid.to_string())
                .or_default()
                .insert(name);
        }
        let keys_by_name = plays_by_name_and_mbid
            .into_iter()
            .map(|(name, by_mbid)| {
                let key = dominant_mbid(by_mbid)
                    .map_or_else(|| format!("name:{name}"), |mbid| format!("mbid:{mbid}"));
                (name, key)
            })
            .collect();
        Self {
            keys_by_name,
            names_by_mbid,
        }
    }

    /// The group key of a raw spelling. Spellings the resolver never saw fall
    /// back to their own name key, so a lookup can never return an empty key.
    pub fn key_for(&self, raw: &str) -> String {
        let name = normalize_group_key(raw);
        match self.keys_by_name.get(&name) {
            Some(key) => key.clone(),
            None => format!("name:{name}"),
        }
    }

    /// Normalized names covered by `key`, for callers that must recover a group
    /// from a key another domain resolved (a period-scoped snapshot key looked
    /// up against the whole catalog, say). Deliberately conservative: it widens
    /// an MBID key to every name that carries that MBID, but never chases the
    /// merge further, so a fallback can only ever return the asked-for act.
    pub fn names_for_key(&self, key: &str) -> BTreeSet<String> {
        let mut names = self
            .keys_by_name
            .iter()
            .filter(|(_, resolved)| resolved.as_str() == key)
            .map(|(name, _)| name.clone())
            .collect::<BTreeSet<_>>();
        if let Some(mbid) = key.strip_prefix("mbid:") {
            names.extend(self.names_by_mbid.get(mbid).into_iter().flatten().cloned());
        }
        if let Some(name) = key.strip_prefix("name:") {
            names.insert(name.to_string());
        }
        names
    }
}

/// Most-played MBID of one name group, tiebroken alphabetically so the key is
/// independent of row order.
fn dominant_mbid(by_mbid: BTreeMap<String, i64>) -> Option<String> {
    by_mbid
        .into_iter()
        .max_by(|(left_mbid, left_plays), (right_mbid, right_plays)| {
            left_plays
                .cmp(right_plays)
                .then_with(|| right_mbid.cmp(left_mbid))
        })
        .map(|(mbid, _)| mbid)
}

/// Folds raw rows into exact runtime groups and returns a total display order.
pub fn fold_groups(rows: &[GroupInput<'_>]) -> Vec<Group> {
    let resolver = KeyResolver::build(rows.iter().copied());
    let mut accumulators = HashMap::<String, GroupAccumulator>::new();
    for row in rows {
        let key = resolver.key_for(row.raw);
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
