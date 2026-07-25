//! Loopback-only seams for isolated scrobbling acceptance tests.

use reprise_core::scrobbling::{LastFmClient, ListenBrainzClient, MetadataError};

const LISTENBRAINZ_API_ROOT_ENV: &str = "REPRISE_SMOKE_LISTENBRAINZ_API_ROOT";
const LASTFM_API_ROOT_ENV: &str = "REPRISE_SMOKE_LASTFM_API_ROOT";
const LASTFM_AUTH_ROOT_ENV: &str = "REPRISE_SMOKE_LASTFM_AUTH_ROOT";

pub(in crate::ui) fn is_loopback_http_root(value: &str) -> bool {
    ["http://127.0.0.1:", "http://[::1]:"]
        .into_iter()
        .find_map(|prefix| value.strip_prefix(prefix))
        .and_then(|remainder| remainder.split('/').next())
        .is_some_and(|port| port.parse::<u16>().is_ok())
}

fn loopback_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|root| cfg!(debug_assertions) && is_loopback_http_root(root))
}

pub(in crate::ui) fn listenbrainz_api_root() -> Option<String> {
    loopback_env(LISTENBRAINZ_API_ROOT_ENV)
}

pub(in crate::ui) fn lastfm_api_root() -> Option<String> {
    loopback_env(LASTFM_API_ROOT_ENV)
}

fn lastfm_auth_root() -> Option<String> {
    loopback_env(LASTFM_AUTH_ROOT_ENV)
}

pub(in crate::ui) fn listenbrainz_client() -> ListenBrainzClient {
    listenbrainz_api_root().map_or_else(ListenBrainzClient::new, |root| {
        ListenBrainzClient::with_api_root(&root)
    })
}

pub(in crate::ui) fn lastfm_client(
    api_key: &str,
    shared_secret: &str,
) -> Result<LastFmClient, MetadataError> {
    match (lastfm_api_root(), lastfm_auth_root()) {
        (Some(api_root), Some(auth_root)) => {
            LastFmClient::with_roots(&api_root, &auth_root, api_key, shared_secret)
        }
        _ => LastFmClient::new(api_key, shared_secret),
    }
}

pub(in crate::ui) fn bypass_lastfm_browser_launch(url: &str) -> bool {
    lastfm_auth_root().is_some_and(|root| authorization_url_matches_root(url, &root))
}

fn authorization_url_matches_root(url: &str, root: &str) -> bool {
    if !is_loopback_http_root(root) {
        return false;
    }
    let root = format!("{}/", root.trim_end_matches('/'));
    url.starts_with(&format!("{root}?"))
}

#[cfg(test)]
mod tests {
    use super::{authorization_url_matches_root, is_loopback_http_root};

    #[test]
    fn smoke_api_override_accepts_only_explicit_loopback_http_ports() {
        assert!(is_loopback_http_root("http://127.0.0.1:8123"));
        assert!(is_loopback_http_root("http://[::1]:8123/api"));
        assert!(!is_loopback_http_root("https://api.listenbrainz.org"));
        assert!(!is_loopback_http_root("http://127.0.0.1"));
        assert!(!is_loopback_http_root("http://example.test:8123"));
    }

    #[test]
    fn production_and_non_port_urls_cannot_be_smoke_targets() {
        assert!(!is_loopback_http_root("https://ws.audioscrobbler.com/2.0/"));
        assert!(!is_loopback_http_root("http://[::1]/2.0/"));
        assert!(is_loopback_http_root("http://127.0.0.1:9876/2.0/"));
    }

    #[test]
    fn browser_bypass_requires_the_configured_loopback_authorization_root() {
        assert!(authorization_url_matches_root(
            "http://127.0.0.1:9876/auth/?api_key=key&token=token",
            "http://127.0.0.1:9876/auth"
        ));
        assert!(!authorization_url_matches_root(
            "https://www.last.fm/api/auth/?api_key=key&token=token",
            "http://127.0.0.1:9876/auth"
        ));
        assert!(!authorization_url_matches_root(
            "http://127.0.0.1:9876/auth/?api_key=key&token=token",
            "https://www.last.fm/api/auth"
        ));
    }
}
