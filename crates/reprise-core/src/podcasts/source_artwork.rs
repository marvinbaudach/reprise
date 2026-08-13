//! Bounded blocking HTTP boundary for podcast, channel, and station artwork.
//!
//! Callers must run this off the UI thread.

use std::fmt;
use std::io::Read;
use std::net::IpAddr;
use std::time::Duration;

use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

use super::PodcastError;

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug)]
struct NonPublicAddress;

impl fmt::Display for NonPublicAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("source artwork resolved to a non-public address")
    }
}

impl std::error::Error for NonPublicAddress {}

#[derive(Debug)]
struct PublicOnlyResolver<R> {
    inner: R,
}

impl<R: Resolver> Resolver for PublicOnlyResolver<R> {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let addresses = self.inner.resolve(uri, config, timeout)?;
        if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
            return Err(ureq::Error::Other(Box::new(NonPublicAddress)));
        }
        Ok(addresses)
    }
}

pub fn fetch(url: &str) -> Result<Vec<u8>, PodcastError> {
    let url = validate_remote_url(url)?;
    fetch_with_resolver(&url, DefaultResolver::default())
}

fn fetch_with_resolver(url: &url::Url, resolver: impl Resolver) -> Result<Vec<u8>, PodcastError> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(super::http::user_agent())
        .http_status_as_error(false)
        .max_redirects(0)
        .proxy(None)
        .build();
    let response = ureq::Agent::with_parts(
        config,
        DefaultConnector::default(),
        PublicOnlyResolver { inner: resolver },
    )
    .get(url.as_str())
    .call()
    .map_err(classify_transport)?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(PodcastError::HttpStatus(status));
    }
    read_bounded(response.into_body().into_reader())
}

fn validate_remote_url(value: &str) -> Result<url::Url, PodcastError> {
    let url = url::Url::parse(value).map_err(|error| PodcastError::Parse(error.to_string()))?;
    let Some(host) = url.host() else {
        return Err(PodcastError::Parse(
            "source artwork must use HTTP or HTTPS".into(),
        ));
    };
    let (local_name, unsafe_literal) = match host {
        url::Host::Domain(host) => {
            let host = host.to_ascii_lowercase();
            (
                host == "localhost"
                    || host.ends_with(".localhost")
                    || host.ends_with(".local")
                    || host.ends_with(".internal"),
                false,
            )
        }
        url::Host::Ipv4(ip) => (false, !is_public_ip(ip.into())),
        url::Host::Ipv6(ip) => (false, !is_public_ip(ip.into())),
    };
    if !matches!(url.scheme(), "http" | "https") || local_name || unsafe_literal {
        return Err(PodcastError::Parse(
            "source artwork must use a public HTTP or HTTPS host".into(),
        ));
    }
    Ok(url)
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [first, second, ..] = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_documentation()
                && !ip.is_multicast()
                && !ip.is_unspecified()
                && first != 0
                && !(first == 100 && (64..=127).contains(&second))
                && !(first == 198 && (18..=19).contains(&second))
        }
        IpAddr::V6(ip) => {
            let first = ip.segments()[0];
            !ip.is_loopback()
                && !ip.is_multicast()
                && !ip.is_unspecified()
                && first & 0xfe00 != 0xfc00
                && first & 0xffc0 != 0xfe80
                && !(ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8)
                && ip
                    .to_ipv4_mapped()
                    .is_none_or(|mapped| is_public_ip(mapped.into()))
        }
    }
}

fn read_bounded(mut reader: impl Read) -> Result<Vec<u8>, PodcastError> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take((MAX_IMAGE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| PodcastError::Body(error.to_string()))?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(PodcastError::Body(
            "source artwork exceeds the 4 MiB limit".into(),
        ));
    }
    Ok(bytes)
}

fn classify_transport(error: ureq::Error) -> PodcastError {
    match error {
        ureq::Error::Other(error) if error.is::<NonPublicAddress>() => {
            PodcastError::Parse("source artwork resolved to a non-public address".into())
        }
        ureq::Error::Timeout(_) => PodcastError::Timeout,
        other if other.to_string().to_ascii_lowercase().contains("timeout") => {
            PodcastError::Timeout
        }
        other => PodcastError::Transport(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;

    #[derive(Debug)]
    struct FixedResolver {
        address: SocketAddr,
        calls: Arc<AtomicUsize>,
    }

    impl ureq::unversioned::resolver::Resolver for FixedResolver {
        fn resolve(
            &self,
            _uri: &ureq::http::Uri,
            _config: &ureq::config::Config,
            _timeout: ureq::unversioned::transport::NextTimeout,
        ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut addresses = self.empty();
            addresses.push(self.address);
            Ok(addresses)
        }
    }

    #[test]
    fn source_artwork_rejects_local_and_oversized_inputs() {
        assert!(validate_remote_url("https://images.test/show.jpg").is_ok());
        assert!(validate_remote_url("file:///home/user/secret").is_err());
        assert!(validate_remote_url("http://127.0.0.1/private").is_err());
        assert!(validate_remote_url("http://[::1]/private").is_err());
        assert!(validate_remote_url("https://artwork.local/private").is_err());

        let oversized = std::io::repeat(1).take((MAX_IMAGE_BYTES + 1) as u64);
        assert!(read_bounded(oversized).is_err());
    }

    #[test]
    fn source_artwork_connect_refuses_a_private_resolver_answer() {
        let url = validate_remote_url("http://artwork.example.test/image.jpg").unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let resolver = FixedResolver {
            address: "127.0.0.1:80".parse().unwrap(),
            calls: calls.clone(),
        };

        let result = fetch_with_resolver(&url, resolver);

        assert!(matches!(
            result,
            Err(PodcastError::Parse(message))
                if message == "source artwork resolved to a non-public address"
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
