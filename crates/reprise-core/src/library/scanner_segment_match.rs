//! Which existing row each track of an edited sheet keeps.
//!
//! A row's id is what ratings, play counts, playlists and listens hang on, so
//! when a sheet changes, each of its tracks takes the row of the same song. The
//! position in the file is the weakest evidence for that: splitting one track in
//! two moves every later track one place on. So a track keeps, in this order:
//!
//! 1. the row with its start and its title,
//! 2. the row with its title, where exactly one row and one track have it,
//! 3. the row with its start,
//! 4. the row at its position.
//!
//! The title wins over the start when the two disagree: a sheet whose titles
//! trade places is a correction of which song is which, and a rating belongs to
//! the song. A track no row is left for gets a new one.

/// A row the file already has.
pub(super) struct KnownSegment {
    pub(super) id: i64,
    pub(super) index: i64,
    pub(super) start_ms: Option<i64>,
    pub(super) title: String,
}

/// A track the sheet now gives the file.
pub(super) struct WantedSegment<'a> {
    pub(super) index: i64,
    pub(super) start_ms: i64,
    pub(super) title: &'a str,
}

/// The id each of `wanted` keeps, in the order given; `None` for a new row.
pub(super) fn match_rows(known: &[KnownSegment], wanted: &[WantedSegment<'_>]) -> Vec<Option<i64>> {
    let mut matching = Matching {
        known,
        wanted,
        taken: vec![false; known.len()],
        matched: vec![None; wanted.len()],
    };
    matching.pass(|row, track, _| row.start_ms == Some(track.start_ms) && row.title == track.title);
    matching.pass(|row, track, free| row.title == track.title && free.title_is_unique(track.title));
    matching.pass(|row, track, _| row.start_ms == Some(track.start_ms));
    matching.pass(|row, track, _| row.index == track.index);
    matching
        .matched
        .into_iter()
        .map(|row| row.map(|row| known[row].id))
        .collect()
}

struct Matching<'a, 'b> {
    known: &'a [KnownSegment],
    wanted: &'a [WantedSegment<'b>],
    taken: Vec<bool>,
    /// For each wanted track, the index into `known` of the row it keeps.
    matched: Vec<Option<usize>>,
}

impl Matching<'_, '_> {
    /// Gives every track still without a row the first free row that `fits`.
    /// `fits` also sees the rows and tracks still free, as `Remaining`.
    fn pass(&mut self, fits: impl Fn(&KnownSegment, &WantedSegment<'_>, &Remaining<'_>) -> bool) {
        for slot in 0..self.wanted.len() {
            if self.matched[slot].is_some() {
                continue;
            }
            let remaining = Remaining {
                known: self.known,
                wanted: self.wanted,
                taken: &self.taken,
                matched: &self.matched,
            };
            let found = (0..self.known.len()).find(|row| {
                !self.taken[*row] && fits(&self.known[*row], &self.wanted[slot], &remaining)
            });
            if let Some(row) = found {
                self.taken[row] = true;
                self.matched[slot] = Some(row);
            }
        }
    }
}

/// The rows and tracks no earlier pass has paired.
struct Remaining<'a> {
    known: &'a [KnownSegment],
    wanted: &'a [WantedSegment<'a>],
    taken: &'a [bool],
    matched: &'a [Option<usize>],
}

impl Remaining<'_> {
    /// Whether exactly one free row and one free track carry `title`.
    fn title_is_unique(&self, title: &str) -> bool {
        let rows = self
            .known
            .iter()
            .zip(self.taken)
            .filter(|(row, taken)| !**taken && row.title == title)
            .count();
        let tracks = self
            .wanted
            .iter()
            .zip(self.matched)
            .filter(|(track, matched)| matched.is_none() && track.title == title)
            .count();
        rows == 1 && tracks == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(rows: &[(i64, i64, i64, &str)]) -> Vec<KnownSegment> {
        rows.iter()
            .map(|(id, index, start_ms, title)| KnownSegment {
                id: *id,
                index: *index,
                start_ms: Some(*start_ms),
                title: (*title).to_string(),
            })
            .collect()
    }

    fn wanted<'a>(tracks: &[(i64, i64, &'a str)]) -> Vec<WantedSegment<'a>> {
        tracks
            .iter()
            .map(|(index, start_ms, title)| WantedSegment {
                index: *index,
                start_ms: *start_ms,
                title,
            })
            .collect()
    }

    #[test]
    fn a_track_whose_start_was_corrected_keeps_its_row_by_title() {
        let rows = known(&[(10, 1, 0, "A"), (11, 2, 1_000, "B")]);

        let ids = match_rows(&rows, &wanted(&[(1, 0, "A"), (2, 1_200, "B")]));

        assert_eq!(ids, [Some(10), Some(11)]);
    }

    #[test]
    fn a_track_whose_title_was_corrected_keeps_its_row_by_start() {
        let rows = known(&[(10, 1, 0, "A"), (11, 2, 1_000, "B")]);

        let ids = match_rows(&rows, &wanted(&[(1, 0, "A"), (2, 1_000, "Bee")]));

        assert_eq!(ids, [Some(10), Some(11)]);
    }

    #[test]
    fn a_title_two_tracks_share_does_not_decide_between_them() {
        let rows = known(&[(10, 1, 0, "Untitled"), (11, 2, 1_000, "Untitled")]);

        let ids = match_rows(
            &rows,
            &wanted(&[
                (1, 0, "Untitled"),
                (2, 500, "Untitled"),
                (3, 1_000, "Untitled"),
            ]),
        );

        assert_eq!(ids, [Some(10), None, Some(11)]);
    }

    #[test]
    fn a_track_with_nothing_in_common_takes_the_row_at_its_position_or_none() {
        let rows = known(&[(10, 1, 0, "A"), (11, 2, 1_000, "B")]);

        let ids = match_rows(
            &rows,
            &wanted(&[(1, 300, "X"), (2, 900, "Y"), (3, 2_000, "Z")]),
        );

        assert_eq!(ids, [Some(10), Some(11), None]);
    }
}
