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

#[derive(Debug, thiserror::Error)]
pub enum EpisodeTagError {
    #[error("could not open the download for tagging: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not tag the download: {0}")]
    Lofty(#[from] lofty::error::LoftyError),
    #[error("the download's container carries no writable tag")]
    NoWritableTag,
}

impl EpisodeTagError {
    /// `POD-13`: a fixed reason for logs — never the lofty prose, which can
    /// name a path.
    #[must_use]
    pub fn classify(&self) -> &'static str {
        "unsupported or unreadable audio container"
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
    let mut tagged = TaggedFile::read_from(&mut file, ParseOptions::new().read_properties(false))?;
    drop(file);
    if tagged.primary_tag().is_none() {
        tagged.insert_tag(Tag::new(tagged.primary_tag_type()));
    }
    let tag = tagged
        .primary_tag_mut()
        .ok_or(EpisodeTagError::NoWritableTag)?;
    set_episode_tags(tag, tags);
    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

fn set_episode_tags(tag: &mut Tag, tags: &EpisodeTagSet) {
    tag.set_title(tags.title.clone());
    tag.set_album(tags.show.clone());
    tag.set_artist(tags.artist.clone());
    tag.insert_text(ItemKey::AlbumArtist, tags.show.clone());
    tag.insert_text(ItemKey::RecordingDate, tags.date.clone());
}

/// [`write_episode_tags`], with a failure logged and dropped: a container
/// Reprise cannot tag must still become a usable download.
pub fn tag_best_effort(path: &Path, tags: &EpisodeTagSet, episode_id: i64) {
    if let Err(error) = write_episode_tags(path, tags) {
        tracing::warn!(
            episode_id,
            reason = error.classify(),
            "podcast download could not be tagged"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use lofty::prelude::*;

    use super::{episode_date, write_episode_tags, EpisodeTagSet};

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

    #[test]
    fn pod_17_the_episode_date_is_the_publication_day_in_utc() {
        assert_eq!(episode_date(Some(1_785_225_600), 1), "2026-07-28");
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
