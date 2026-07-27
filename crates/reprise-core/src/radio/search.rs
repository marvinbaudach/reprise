//! radio-browser station search.

use serde::Deserialize;

use super::RadioError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchOrder {
    #[default]
    Votes,
    Name,
    Clicks,
}

impl SearchOrder {
    #[must_use]
    pub const fn query_value(self) -> &'static str {
        match self {
            Self::Votes => "votes",
            Self::Name => "name",
            Self::Clicks => "clickcount",
        }
    }

    #[must_use]
    pub const fn setting_value(self) -> &'static str {
        match self {
            Self::Votes => "votes",
            Self::Name => "name",
            Self::Clicks => "clicks",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StationCandidate {
    pub uuid: String,
    pub name: String,
    pub url_resolved: String,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<i64>,
    pub country_code: Option<String>,
    pub genre: Option<String>,
    pub tags: Vec<String>,
    pub votes: i64,
    pub favicon_url: Option<String>,
}

#[derive(Deserialize)]
struct CandidateDocument {
    #[serde(default, rename = "stationuuid")]
    uuid: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    url_resolved: String,
    #[serde(default)]
    codec: String,
    bitrate: Option<i64>,
    #[serde(default, rename = "countrycode")]
    country_code: String,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    votes: i64,
    #[serde(default, rename = "favicon")]
    favicon_url: String,
}

pub fn search(terms: &str, order: SearchOrder) -> Result<Vec<StationCandidate>, RadioError> {
    if terms.trim().is_empty() {
        return Ok(Vec::new());
    }
    super::servers::try_servers(|server| {
        let body = super::http::get(&search_url(server, terms, order))?;
        parse_candidates(&body)
    })
}

pub fn find_by_url(stream_url: &str) -> Result<Option<StationCandidate>, RadioError> {
    if http_url(stream_url).is_none() {
        return Ok(None);
    }
    super::servers::try_servers(|server| {
        let body = super::http::get(&by_url_url(server, stream_url))?;
        Ok(parse_candidates(&body)?.into_iter().next())
    })
}

#[must_use]
pub fn search_url(server: &str, terms: &str, order: SearchOrder) -> String {
    let mut url = url::Url::parse(&format!(
        "{}/json/stations/search",
        server.trim_end_matches('/')
    ))
    .expect("radio-browser server URLs are normalized");
    url.query_pairs_mut()
        .append_pair("name", terms.trim())
        .append_pair("order", order.query_value())
        .append_pair("reverse", "true")
        .append_pair("limit", "50")
        .append_pair("hidebroken", "true");
    url.into()
}

#[must_use]
pub fn by_url_url(server: &str, stream_url: &str) -> String {
    let mut url = url::Url::parse(&format!(
        "{}/json/stations/byurl",
        server.trim_end_matches('/')
    ))
    .expect("radio-browser server URLs are normalized");
    url.query_pairs_mut().append_pair("url", stream_url.trim());
    url.into()
}

pub fn parse_candidates(json: &str) -> Result<Vec<StationCandidate>, RadioError> {
    let documents: Vec<CandidateDocument> =
        serde_json::from_str(json).map_err(|error| RadioError::Parse(error.to_string()))?;
    Ok(documents
        .into_iter()
        .filter_map(|document| {
            let uuid = non_empty(&document.uuid)?;
            let name = non_empty(&document.name)?;
            let url_resolved = http_url(&document.url_resolved)?;
            let tags = document
                .tags
                .split(',')
                .filter_map(non_empty)
                .collect::<Vec<_>>();
            Some(StationCandidate {
                uuid,
                name,
                url_resolved,
                codec: non_empty(&document.codec),
                bitrate_kbps: document.bitrate.filter(|bitrate| *bitrate > 0),
                country_code: non_empty(&document.country_code)
                    .map(|country| country.to_ascii_uppercase()),
                genre: tags.first().cloned(),
                tags,
                votes: document.votes.max(0),
                favicon_url: http_url(&document.favicon_url),
            })
        })
        .collect())
}

#[must_use]
pub fn format_candidate_details(candidate: &StationCandidate) -> String {
    let mut parts = Vec::new();
    if let Some(genre) = candidate.genre.as_deref() {
        parts.push(genre.to_owned());
    }
    if let Some(bitrate) = candidate.bitrate_kbps {
        parts.push(format!("{bitrate} kbit/s"));
    }
    if let Some(country) = candidate.country_code.as_deref() {
        parts.push(country.to_owned());
    }
    parts.push(format_votes(candidate.votes));
    parts.join(" · ")
}

fn format_votes(votes: i64) -> String {
    if votes >= 1_000 {
        let rounded = votes as f64 / 1_000.0;
        format!("{rounded:.1}k votes")
    } else {
        format!("{votes} votes")
    }
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn http_url(value: &str) -> Option<String> {
    let value = value.trim();
    url::Url::parse(value)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .map(|_| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_parser_maps_candidates_and_discards_unplayable_rows() {
        let candidates = parse_candidates(
            r#"[
              {"stationuuid":"abc","name":"Metal One","url_resolved":"https://one/live",
               "codec":"MP3","bitrate":320,"countrycode":"US","tags":"metal,rock",
               "votes":4200,"favicon":"https://one/icon.png"},
              {"stationuuid":"broken","name":"Broken","url_resolved":"","bitrate":0}
            ]"#,
        )
        .unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].uuid, "abc");
        assert_eq!(candidates[0].genre.as_deref(), Some("metal"));
        assert_eq!(
            format_candidate_details(&candidates[0]),
            "metal · 320 kbit/s · US · 4.2k votes"
        );
    }

    #[test]
    fn search_url_encodes_the_term_and_selected_order() {
        assert_eq!(
            search_url("https://de1.example", "deep house", SearchOrder::Clicks),
            "https://de1.example/json/stations/search?name=deep+house&order=clickcount&reverse=true&limit=50&hidebroken=true"
        );
    }

    #[test]
    fn by_url_lookup_encodes_the_stream_as_one_query_value() {
        assert_eq!(
            by_url_url("https://de1.example", "https://radio.example/live?a=1&b=2"),
            "https://de1.example/json/stations/byurl?url=https%3A%2F%2Fradio.example%2Flive%3Fa%3D1%26b%3D2"
        );
    }

    #[test]
    fn fixture_search_discovers_a_server_without_using_the_network() {
        let fixtures = tempfile::tempdir().unwrap();
        std::fs::write(
            fixtures.path().join("servers.json"),
            r#"[{"name":"fixture.radio-browser.test"}]"#,
        )
        .unwrap();
        std::fs::write(
            fixtures.path().join("search-metal.json"),
            r#"[{"stationuuid":"one","name":"Metal One",
                 "url_resolved":"https://radio.example/live","votes":10}]"#,
        )
        .unwrap();

        let candidates = super::super::http::with_fixture_dir(fixtures.path(), || {
            search("metal", SearchOrder::Votes)
        })
        .unwrap();

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "Metal One");
    }
}
