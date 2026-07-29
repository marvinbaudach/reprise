//! Bounded blocking HTTP boundary for podcast, channel, and station artwork.
//!
//! Callers must run this off the UI thread.

use std::io::Read;
use std::net::{IpAddr, ToSocketAddrs};
use std::time::Duration;

use super::PodcastError;

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;

pub fn fetch(url: &str) -> Result<Vec<u8>, PodcastError> {
    let url = validate_remote_url(url)?;
    validate_resolved_host(&url)?;
    let response = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(super::http::user_agent())
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .new_agent()
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

fn validate_resolved_host(url: &url::Url) -> Result<(), PodcastError> {
    let host = url
        .host()
        .ok_or_else(|| PodcastError::Parse("source artwork host is missing".into()))?;
    let url::Host::Domain(host) = host else {
        return Ok(());
    };
    let port = url
        .port_or_known_default()
        .ok_or_else(|| PodcastError::Parse("source artwork port is missing".into()))?;
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|error| PodcastError::Transport(error.to_string()))?
        .collect::<Vec<_>>();
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(PodcastError::Parse(
            "source artwork resolved to a non-public address".into(),
        ));
    }
    Ok(())
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
        ureq::Error::Timeout(_) => PodcastError::Timeout,
        other if other.to_string().to_ascii_lowercase().contains("timeout") => {
            PodcastError::Timeout
        }
        other => PodcastError::Transport(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
