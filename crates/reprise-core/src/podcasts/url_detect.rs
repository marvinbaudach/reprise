//! Podcast search and URL input classification.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    Search,
    YoutubeUrl,
    ProbableFeedUrl,
}

#[must_use]
pub fn detect(input: &str) -> InputKind {
    let Ok(url) = url::Url::parse(input.trim()) else {
        return InputKind::Search;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return InputKind::Search;
    }
    if is_youtube_url(&url) {
        InputKind::YoutubeUrl
    } else {
        InputKind::ProbableFeedUrl
    }
}

fn is_youtube_url(url: &url::Url) -> bool {
    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    if host == "youtu.be" {
        return true;
    }
    if !matches!(host.as_str(), "youtube.com" | "m.youtube.com") {
        return false;
    }
    let path = url.path();
    path.starts_with("/@")
        || path.starts_with("/channel/")
        || path.starts_with("/playlist")
        || url.query_pairs().any(|(key, _)| key == "list")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_search_youtube_and_probable_feed_inputs() {
        assert_eq!(detect("systems podcast"), InputKind::Search);
        assert_eq!(
            detect("https://www.youtube.com/@example"),
            InputKind::YoutubeUrl
        );
        assert_eq!(
            detect("https://youtube.com/playlist?list=PL123"),
            InputKind::YoutubeUrl
        );
        assert_eq!(
            detect("https://feeds.example.test/show.xml"),
            InputKind::ProbableFeedUrl
        );
        assert_eq!(detect("ftp://example.test/feed"), InputKind::Search);
    }
}
