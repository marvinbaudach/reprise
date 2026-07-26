//! ICY response-header parsing.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IcyProbe {
    pub name: Option<String>,
    pub bitrate_kbps: Option<i64>,
    pub genre: Option<String>,
    pub content_type: Option<String>,
}

pub fn probe(url: &str) -> Result<IcyProbe, super::RadioError> {
    super::http::icy_headers(url).map(|headers| parse_icy_headers(&headers))
}

pub fn parse_icy_headers(headers: &[(String, String)]) -> IcyProbe {
    let value = |name: &str| {
        headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .and_then(|(_, value)| non_empty(value))
    };
    IcyProbe {
        name: value("icy-name"),
        bitrate_kbps: value("icy-br").and_then(|value| {
            value
                .split(',')
                .next()
                .and_then(|value| value.trim().parse().ok())
                .filter(|bitrate| *bitrate > 0)
        }),
        genre: value("icy-genre"),
        content_type: value("content-type").map(|value| {
            value
                .split_once(';')
                .map_or(value.as_str(), |(kind, _)| kind)
                .trim()
                .to_owned()
        }),
    }
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rad_4_icy_preview_parses_headers_case_insensitively() {
        let probe = parse_icy_headers(&[
            ("ICY-NAME".into(), " Radio Example ".into()),
            ("icy-br".into(), " 192 ".into()),
            ("icy-genre".into(), "Metal".into()),
            ("Content-Type".into(), "audio/mpeg; charset=utf-8".into()),
        ]);

        assert_eq!(probe.name.as_deref(), Some("Radio Example"));
        assert_eq!(probe.bitrate_kbps, Some(192));
        assert_eq!(probe.genre.as_deref(), Some("Metal"));
        assert_eq!(probe.content_type.as_deref(), Some("audio/mpeg"));
    }

    #[test]
    fn icy_preview_tolerates_blank_and_invalid_values() {
        let probe = parse_icy_headers(&[
            ("icy-name".into(), " ".into()),
            ("icy-br".into(), "fast".into()),
        ]);

        assert_eq!(probe, IcyProbe::default());
    }
}
