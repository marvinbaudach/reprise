//! Fixtures shared by the YouTube pipeline test modules.

use std::path::Path;

use std::cell::RefCell;

use super::*;

pub(super) fn conn() -> Db {
    let conn = Db::open_in_memory().unwrap();
    // These tests exercise fetch/parse/store logic, not the NET-1a gate
    // itself (see the dedicated `net_1a_*` tests below), so YouTube starts
    // enabled here.
    crate::online_sources::set_enabled(&conn, true).unwrap();
    crate::modules::set_enabled(&conn, &crate::modules::YOUTUBE_MODULE, true).unwrap();
    conn
}

#[derive(Default)]
pub(super) struct NeverYoutube;

impl YoutubeFetcher for NeverYoutube {
    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        Err(PodcastError::YtDlpFailure {
            kind: crate::podcasts::ytdlp::YtDlpFailureKind::Other,
            stderr: "unexpected YouTube call".to_owned(),
        })
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        Err(PodcastError::YtDlpFailure {
            kind: crate::podcasts::ytdlp::YtDlpFailureKind::Other,
            stderr: "unexpected YouTube call".to_owned(),
        })
    }
}

pub(super) struct FakeFeedNeverCalled;

impl FeedFetcher for FakeFeedNeverCalled {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        panic!("a disabled subscription must not be fetched")
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("a disabled subscription must not be downloaded")
    }
}

pub(super) struct OfficialYoutubeFeed {
    pub(super) requested_urls: RefCell<Vec<String>>,
    pub(super) author: Option<&'static str>,
}

impl FeedFetcher for OfficialYoutubeFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        panic!("YouTube refresh must request the derived official feed URL");
    }

    fn fetch_url(
        &self,
        url: &str,
        _: Option<&str>,
        _: Option<&str>,
    ) -> Result<Response, PodcastError> {
        self.requested_urls.borrow_mut().push(url.to_owned());
        let author = self
            .author
            .map(|author| format!("<author><name>{author}</name></author>"))
            .unwrap_or_default();
        Ok(Response {
            body: format!(
                r#"<feed xmlns="http://www.w3.org/2005/Atom"
                          xmlns:yt="http://www.youtube.com/xml/schemas/2015">
              <title>Videos</title>{author}
              <entry><id>yt:video:newest</id><yt:videoId>newest</yt:videoId>
                <title>Newest</title><published>2026-07-28T08:00:00Z</published></entry>
              <entry><id>yt:video:older</id><yt:videoId>older</yt:videoId>
                <title>Older</title><published>2026-07-27T08:00:00Z</published></entry>
            </feed>"#
            ),
            etag: Some("\"youtube-v1\"".to_owned()),
            last_modified: None,
        })
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        panic!("refresh without auto-download must not download")
    }
}
