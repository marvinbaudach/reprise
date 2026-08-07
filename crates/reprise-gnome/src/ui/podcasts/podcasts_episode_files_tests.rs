use std::path::PathBuf;

use reprise_core::podcasts::{EpisodeRow, PodcastKind};

use super::*;

fn episode(id: i64, downloaded_path: Option<String>) -> EpisodeRow {
    EpisodeRow {
        id,
        subscription_id: 7,
        guid: format!("episode-{id}"),
        title: format!("Episode {id}"),
        show: "Show".into(),
        show_image_url: None,
        image_url: None,
        kind: PodcastKind::Rss,
        audio_url: format!("https://example.test/{id}.mp3"),
        page_url: None,
        published_at: None,
        duration_secs: None,
        downloaded_path,
        downloaded_bytes: None,
        played_at: None,
        position_ms: 0,
        first_seen_at: 1,
        is_new: false,
        media_category: None,
    }
}

#[test]
fn ctx_13_a_single_downloaded_episode_reveals_its_file() {
    let path = PathBuf::from("/downloads/show/episode.opus");

    assert_eq!(file_reveal(&[Some(path.clone())]), FileReveal::Reveal(path));
}

#[test]
fn ctx_13_b_an_episode_without_a_download_offers_nothing() {
    assert_eq!(file_reveal(&[None]), FileReveal::Hidden);
}

#[test]
fn ctx_13_an_empty_selection_offers_nothing() {
    assert_eq!(file_reveal(&[]), FileReveal::Hidden);
}

#[test]
fn ctx_13_c_a_selection_sharing_one_folder_opens_that_folder() {
    let folder = PathBuf::from("/downloads/show");

    assert_eq!(
        file_reveal(&[Some(folder.join("one.opus")), Some(folder.join("two.opus")),]),
        FileReveal::OpenFolder(folder)
    );
}

#[test]
fn ctx_13_d_a_selection_with_one_undownloaded_episode_offers_nothing() {
    assert_eq!(
        file_reveal(&[Some(PathBuf::from("/downloads/show/one.opus")), None,]),
        FileReveal::Hidden
    );
}

#[test]
fn ctx_13_e_a_selection_across_two_folders_offers_nothing() {
    assert_eq!(
        file_reveal(&[
            Some(PathBuf::from("/downloads/one/episode.opus")),
            Some(PathBuf::from("/downloads/two/episode.opus")),
        ]),
        FileReveal::Hidden
    );
}

#[test]
fn ctx_13_a_selection_with_a_parentless_path_offers_nothing() {
    assert_eq!(
        file_reveal(&[
            Some(PathBuf::from("one.opus")),
            Some(PathBuf::from("two.opus")),
        ]),
        FileReveal::Hidden
    );
}

#[test]
fn ctx_13_from_rows_discards_a_download_path_missing_from_disk() {
    let directory = tempfile::tempdir().unwrap();
    let present = directory.path().join("present.opus");
    std::fs::write(&present, b"audio").unwrap();
    let missing = directory.path().join("missing.opus");
    let rows = [
        episode(1, Some(present.to_string_lossy().into_owned())),
        episode(2, Some(missing.to_string_lossy().into_owned())),
    ];

    let paths = EpisodePaths::from_rows(&rows);

    assert_eq!(paths.lookup(&[1]), vec![Some(present)]);
    assert_eq!(paths.lookup(&[2]), vec![None]);
}

#[test]
fn ctx_13_from_rows_discards_an_empty_download_path() {
    let paths = EpisodePaths::from_rows(&[episode(1, Some(String::new()))]);

    assert_eq!(paths.lookup(&[1]), vec![None]);
}
