//! Standard tags for a downloaded episode, so a phone player can name it
//! without Reprise (`POD-17`).
//!
//! Lofty rewrites Ogg and FLAC containers in memory, so tagging can
//! temporarily hold the complete episode payload in RAM. The write belongs
//! on the unpublished `.part` file because those rewrites truncate first.

use std::path::Path;

use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::TaggedFile;
use lofty::prelude::*;
use lofty::tag::{ItemKey, Tag};

use crate::device_sync::sanitize::MAX_COMPONENT_BYTES;

/// How long a single tag value may get. A feed owns every string in an
/// [`EpisodeTagSet`], and the write below is a truncate-then-rewrite of the
/// whole container, so a hostile or merely broken feed must not be able to
/// decide how much data goes through it. Deliberately the device path's
/// component cap (`MTP-47`): the tag and the file name a phone shows are two
/// views of the same episode, so they are bounded by one number rather than
/// by two that could drift.
const MAX_TAG_BYTES: usize = MAX_COMPONENT_BYTES;

/// The facts written into a downloaded episode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpisodeTagSet {
    pub title: String,
    /// The show — written to both Album and Album Artist.
    pub show: String,
    pub artist: String,
    /// `YYYY-MM-DD`, always resolved through [`episode_date`] so the file name
    /// and the tag can never carry different days.
    pub date: String,
}

/// Why a tag write did not happen — split by the only distinction that
/// matters to the download: whether the file on disk was touched.
#[derive(Debug, thiserror::Error)]
pub enum EpisodeTagError {
    #[error("could not open the download for tagging: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not read the download's container: {0}")]
    Unreadable(lofty::error::FileParseError),
    #[error("the download's container carries no writable tag")]
    NoWritableTag,
    /// [`lofty::prelude::TagExt::save_to_path`] failed. The Ogg and FLAC
    /// writers truncate the file before rewriting it, so this is the one
    /// failure that can leave a destroyed download behind.
    #[error("could not write the download's tags: {0}")]
    Write(lofty::error::FileEncodingError),
}

impl EpisodeTagError {
    /// `POD-13`: a fixed reason for logs — never the lofty prose, which can
    /// name a path.
    #[must_use]
    pub fn classify(&self) -> &'static str {
        match self {
            Self::Io(_) | Self::Unreadable(_) | Self::NoWritableTag => {
                "unsupported or unreadable audio container"
            }
            Self::Write(_) => "the download could not be rewritten with its tags",
        }
    }

    /// Whether the download may already be destroyed. Every other failure is
    /// decided while reading, with the file still exactly as it arrived.
    #[must_use]
    pub fn may_have_destroyed_the_file(&self) -> bool {
        matches!(self, Self::Write(_))
    }
}

/// `YYYY-MM-DD` in UTC from `published_at`, or from `first_seen_at` when no
/// date is known.
#[must_use]
pub fn episode_date(published_at: Option<i64>, first_seen_at: i64) -> String {
    let timestamp = published_at.unwrap_or(first_seen_at);
    chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0)
        .or_else(|| chrono::DateTime::<chrono::Utc>::from_timestamp(first_seen_at, 0))
        .map_or_else(
            || "0000-00-00".to_owned(),
            |date| date.format("%Y-%m-%d").to_string(),
        )
}

/// Writes Title/Album/Artist/Album Artist/Date into `path`, preserving every
/// other tag already there.
pub fn write_episode_tags(path: &Path, tags: &EpisodeTagSet) -> Result<(), EpisodeTagError> {
    let mut file = std::fs::File::open(path)?;
    // Content-based detection: extension-based readers refuse the `.part`
    // temporary that production deliberately hands us.
    let mut tagged = TaggedFile::read_from(&mut file, ParseOptions::new().read_properties(false))
        .map_err(EpisodeTagError::Unreadable)?;
    drop(file);
    if tagged.primary_tag().is_none() {
        tagged.insert_tag(Tag::new(tagged.primary_tag_type()));
    }
    let tag = tagged
        .primary_tag_mut()
        .ok_or(EpisodeTagError::NoWritableTag)?;
    set_episode_tags(tag, tags);
    // Everything above this line only read. From here the file may change.
    tag.save_to_path(path, WriteOptions::default())
        .map_err(EpisodeTagError::Write)?;
    Ok(())
}

/// The cap sits here rather than at the one caller that builds an
/// [`EpisodeTagSet`] today, so no future caller can reach the tag write
/// around it.
fn set_episode_tags(tag: &mut Tag, tags: &EpisodeTagSet) {
    tag.set_title(capped(&tags.title));
    tag.set_album(capped(&tags.show));
    tag.set_artist(capped(&tags.artist));
    tag.insert_text(ItemKey::AlbumArtist, capped(&tags.show));
    tag.insert_text(ItemKey::RecordingDate, capped(&tags.date));
}

/// [`MAX_TAG_BYTES`] on a character boundary — the same helper the device
/// path caps its components with, so a title cannot be shortened one way in
/// the file name and another way in the tag.
fn capped(value: &str) -> String {
    crate::device_sync::sanitize::truncate_utf8(value, MAX_TAG_BYTES)
}

/// [`write_episode_tags`] with its two failure classes told apart (`POD-17`).
///
/// A container Reprise cannot read or cannot tag is logged and dropped: that
/// is decided before anything is written, the download is untouched, and it
/// must still become a usable file. A failed *write* is returned instead:
/// the file may already be truncated, and the only honest thing left is to
/// fail the whole download, so the caller deletes the temporary and the
/// episode stays downloadable. Publishing it would record the size of the
/// wreckage as the episode's size — a number nothing downstream can ever
/// disagree with, on a file that can never be downloaded again.
///
/// `POD-13`: only the classified reason reaches the log line, never the
/// lofty prose or the path.
pub fn tag_download(
    path: &Path,
    tags: &EpisodeTagSet,
    episode_id: i64,
) -> Result<(), EpisodeTagError> {
    let Err(error) = write_episode_tags(path, tags) else {
        return Ok(());
    };
    tracing::warn!(
        episode_id,
        reason = error.classify(),
        "podcast download could not be tagged"
    );
    if error.may_have_destroyed_the_file() {
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use lofty::prelude::*;

    use super::{episode_date, write_episode_tags, EpisodeTagSet, MAX_TAG_BYTES};

    fn audio_fixture(dir: &Path, name: &str) -> PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        let destination = dir.join(name);
        std::fs::copy(source, &destination).unwrap();
        destination
    }

    fn example_tags() -> EpisodeTagSet {
        EpisodeTagSet {
            title: "Episode title".to_owned(),
            show: "The Show".to_owned(),
            artist: "The Author".to_owned(),
            date: "2026-07-28".to_owned(),
        }
    }

    /// Midnight UTC on 2026-07-28. Every timestamp below is expressed
    /// relative to it, so the fixture states its own intent.
    const UTC_MIDNIGHT: i64 = 1_785_196_800;

    /// The day must be the UTC day wherever this runs, so the fixture sits
    /// minutes away from UTC midnight and asserts from **both** sides: a
    /// west-of-UTC machine would push the first case back a day, an
    /// east-of-UTC machine would pull the second case forward. A timestamp
    /// in the middle of the UTC day (08:00, say) proves nothing — it names
    /// the same calendar day in every zone this project is ever run in, so
    /// a `Local` formatter would pass it unnoticed.
    #[test]
    fn pod_17_the_episode_date_is_the_publication_day_in_utc() {
        assert_eq!(episode_date(Some(UTC_MIDNIGHT + 60), 1), "2026-07-28");
        assert_eq!(episode_date(Some(UTC_MIDNIGHT - 60), 1), "2026-07-27");
    }

    #[test]
    fn pod_17_an_episode_without_a_publication_date_falls_back_to_the_day_it_was_first_seen() {
        assert_eq!(episode_date(None, 1_785_225_600), "2026-07-28");
        assert_eq!(episode_date(None, 0), "1970-01-01");
    }

    #[test]
    fn pod_17_a_download_carries_its_title_show_artist_and_date() {
        let directory = tempfile::tempdir().unwrap();
        let path = audio_fixture(directory.path(), "episode.flac");

        write_episode_tags(&path, &example_tags()).unwrap();

        let tagged = lofty::read_from_path(&path).unwrap();
        let tag = tagged.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("Episode title"));
        assert_eq!(tag.album().as_deref(), Some("The Show"));
        assert_eq!(tag.artist().as_deref(), Some("The Author"));
        assert_eq!(
            tag.get_string(lofty::tag::ItemKey::AlbumArtist),
            Some("The Show")
        );
        assert_eq!(
            tag.get_string(lofty::tag::ItemKey::RecordingDate),
            Some("2026-07-28")
        );
    }

    #[test]
    fn pod_17_tags_are_written_from_the_containers_bytes_not_its_file_name() {
        let directory = tempfile::tempdir().unwrap();

        for name in ["episode.part", "episode.audio"] {
            let path = audio_fixture(directory.path(), name);
            write_episode_tags(&path, &example_tags()).unwrap();

            let mut file = std::fs::File::open(&path).unwrap();
            let tagged = lofty::file::TaggedFile::read_from(
                &mut file,
                lofty::config::ParseOptions::new().read_properties(false),
            )
            .unwrap();
            assert_eq!(
                tagged.primary_tag().and_then(Accessor::title).as_deref(),
                Some("Episode title")
            );
        }
    }

    #[test]
    fn pod_17_a_container_reprise_cannot_tag_is_reported_and_left_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("episode.part");
        let original = b"not an audio container";
        std::fs::write(&path, original).unwrap();

        let error = write_episode_tags(&path, &example_tags()).unwrap_err();

        assert_eq!(
            error.classify(),
            "unsupported or unreadable audio container"
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    /// The feed writes these strings, and the tag write is a truncate-then-
    /// rewrite of the whole container: an uncapped title would push an
    /// arbitrary amount of feed-supplied data through the most fragile step
    /// of the download. The device path already caps its components
    /// (`MTP-47`); the tag must not be the one place a feed's length still
    /// goes through unchecked.
    #[test]
    fn pod_17_a_feeds_endless_title_is_capped_before_it_reaches_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = audio_fixture(directory.path(), "episode.flac");
        // One ASCII byte then two-byte characters, so the cap falls in the
        // middle of a character and a byte-wise cut would corrupt it.
        let endless = format!("a{}", "ä".repeat(4_000));
        let tags = EpisodeTagSet {
            title: endless.clone(),
            show: endless.clone(),
            artist: endless,
            date: "2026-07-28".to_owned(),
        };

        write_episode_tags(&path, &tags).unwrap();

        let tagged = lofty::read_from_path(&path).unwrap();
        let tag = tagged.primary_tag().unwrap();
        let written = [
            tag.title().map(std::borrow::Cow::into_owned),
            tag.album().map(std::borrow::Cow::into_owned),
            tag.artist().map(std::borrow::Cow::into_owned),
            tag.get_string(lofty::tag::ItemKey::AlbumArtist)
                .map(str::to_owned),
        ];
        for value in written {
            let value = value.expect("every capped tag is still written");
            assert!(
                value.starts_with("aä"),
                "a long value is shortened, never dropped: {value}"
            );
            assert_eq!(
                value.len(),
                MAX_TAG_BYTES - 1,
                "the cap backs off to the character boundary below it"
            );
        }
    }

    #[test]
    fn pod_17_existing_tags_survive_a_tag_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = audio_fixture(directory.path(), "episode.flac");
        let mut tagged = lofty::read_from_path(&path).unwrap();
        if tagged.primary_tag().is_none() {
            tagged.insert_tag(lofty::tag::Tag::new(tagged.primary_tag_type()));
        }
        let tag = tagged.primary_tag_mut().unwrap();
        tag.set_genre("Documentary".to_owned());
        tag.save_to_path(&path, lofty::config::WriteOptions::default())
            .unwrap();

        write_episode_tags(&path, &example_tags()).unwrap();

        let tagged = lofty::read_from_path(&path).unwrap();
        assert_eq!(
            tagged.primary_tag().and_then(Accessor::genre).as_deref(),
            Some("Documentary")
        );
    }
}
