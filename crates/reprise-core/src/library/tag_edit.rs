//! Selective classic-tag editing. The patch model makes “unchanged” an
//! explicit state so a multi-selection can never clobber per-track values.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MixedValue<T> {
    Uniform(T),
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditableTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<u32>,
    pub track_no: Option<u32>,
    pub genre: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditableTagSummary {
    pub title: MixedValue<String>,
    pub artist: MixedValue<String>,
    pub album: MixedValue<String>,
    pub album_artist: MixedValue<String>,
    pub year: MixedValue<Option<u32>>,
    pub track_no: MixedValue<Option<u32>>,
    pub genre: MixedValue<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagPatch {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<Option<u32>>,
    pub track_no: Option<Option<u32>>,
    pub genre: Option<String>,
}

impl TagPatch {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.artist.is_none()
            && self.album.is_none()
            && self.album_artist.is_none()
            && self.year.is_none()
            && self.track_no.is_none()
            && self.genre.is_none()
    }
}

pub fn summarize(tags: &[EditableTags]) -> Option<EditableTagSummary> {
    fn field<T: Clone + PartialEq>(
        tags: &[EditableTags],
        get: impl Fn(&EditableTags) -> &T,
    ) -> MixedValue<T> {
        let first = get(&tags[0]);
        if tags[1..].iter().all(|tag| get(tag) == first) {
            MixedValue::Uniform(first.clone())
        } else {
            MixedValue::Mixed
        }
    }

    tags.first()?;
    Some(EditableTagSummary {
        title: field(tags, |tag| &tag.title),
        artist: field(tags, |tag| &tag.artist),
        album: field(tags, |tag| &tag.album),
        album_artist: field(tags, |tag| &tag.album_artist),
        year: field(tags, |tag| &tag.year),
        track_no: field(tags, |tag| &tag.track_no),
        genre: field(tags, |tag| &tag.genre),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(title: &str, artist: &str) -> EditableTags {
        EditableTags {
            title: title.into(),
            artist: artist.into(),
            album: "Shared album".into(),
            album_artist: "Shared album artist".into(),
            year: Some(2026),
            track_no: Some(1),
            genre: "Rock".into(),
        }
    }

    #[test]
    fn summary_marks_only_differing_fields_mixed() {
        let summary = summarize(&[tags("First", "Artist"), tags("Second", "Artist")]).unwrap();
        assert_eq!(summary.title, MixedValue::Mixed);
        assert_eq!(summary.artist, MixedValue::Uniform("Artist".to_string()));
        assert_eq!(summary.year, MixedValue::Uniform(Some(2026)));
    }

    #[test]
    fn empty_selection_has_no_summary() {
        assert!(summarize(&[]).is_none());
    }

    #[test]
    fn untouched_patch_is_empty_but_clear_is_not() {
        assert!(TagPatch::default().is_empty());
        let patch = TagPatch {
            year: Some(None),
            ..TagPatch::default()
        };
        assert!(!patch.is_empty());
    }
}
