//! MusicBrainz, Wikidata and Commons URL/parsing steps for artist portraits.

use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

use crate::musicbrainz::FetchError;
use crate::net::client::{build_agent, AgentPolicy};
use crate::net::rate::{wait_for_slot, RateLimitKey};

use super::sidecar::PortraitCredit;

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

static AGENT: OnceLock<ureq::Agent> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommonsImage {
    pub thumbnail_url: String,
    pub credit: PortraitCredit,
}

pub(crate) fn musicbrainz_relations_url(mbid: &str) -> String {
    format!(
        "https://musicbrainz.org/ws/2/artist/{}?inc=url-rels&fmt=json",
        crate::musicbrainz::urlencode(mbid.trim())
    )
}

pub(crate) fn parse_wikidata_qid(json: &str) -> Result<Option<String>, ()> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| ())?;
    let relations = value
        .get("relations")
        .and_then(serde_json::Value::as_array)
        .ok_or(())?;
    Ok(relations.iter().find_map(|relation| {
        let resource = relation.get("url")?.get("resource")?.as_str()?;
        let url = url::Url::parse(resource).ok()?;
        (url.scheme() == "https" && url.host_str() == Some("www.wikidata.org"))
            .then(|| url.path_segments()?.next_back().map(str::to_owned))
            .flatten()
            .filter(|qid| {
                qid.starts_with('Q') && qid[1..].bytes().all(|byte| byte.is_ascii_digit())
            })
    }))
}

pub(crate) fn wikidata_claims_url(qid: &str) -> String {
    format!(
        "https://www.wikidata.org/w/api.php?action=wbgetclaims&entity={}&property=P18&format=json",
        crate::musicbrainz::urlencode(qid.trim())
    )
}

pub(crate) fn parse_p18_filename(json: &str, qid: &str) -> Result<Option<String>, ()> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| ())?;
    let claims = value
        .get("claims")
        .and_then(serde_json::Value::as_object)
        .ok_or(())?;
    let Some(p18) = claims.get("P18").and_then(serde_json::Value::as_array) else {
        return Ok(None);
    };
    Ok(p18
        .iter()
        .find_map(|claim| {
            claim
                .get("mainsnak")?
                .get("datavalue")?
                .get("value")?
                .as_str()
                .map(str::to_owned)
        })
        .filter(|_| !qid.is_empty()))
}

pub(crate) fn commons_imageinfo_url(filename: &str) -> String {
    let title = format!("File:{}", filename.trim());
    format!(
        "https://commons.wikimedia.org/w/api.php?action=query&titles={}&prop=imageinfo&iiprop=url%7Cextmetadata&iiurlwidth=1000&format=json",
        crate::musicbrainz::urlencode(&title)
    )
}

pub(crate) fn parse_commons_imageinfo(json: &str) -> Result<Option<CommonsImage>, ()> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| ())?;
    let pages = value
        .get("query")
        .and_then(|query| query.get("pages"))
        .and_then(serde_json::Value::as_object)
        .ok_or(())?;
    let imageinfo = pages
        .values()
        .find_map(|page| page.get("imageinfo").and_then(serde_json::Value::as_array))
        .and_then(|items| items.first());
    let Some(info) = imageinfo else {
        return Ok(None);
    };
    let metadata = info
        .get("extmetadata")
        .and_then(serde_json::Value::as_object)
        .ok_or(())?;
    let field = |name: &str| {
        metadata
            .get(name)?
            .get("value")?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let Some(thumbnail_url) = info.get("thumburl").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    let Some(author) = field("Artist")
        .map(|value| strip_html(&value))
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let Some(license) = field("LicenseShortName") else {
        return Ok(None);
    };
    let Some(license_url) = field("LicenseUrl") else {
        return Ok(None);
    };
    let Some(file_page_url) = info
        .get("descriptionurl")
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(None);
    };
    Ok(Some(CommonsImage {
        thumbnail_url: thumbnail_url.to_owned(),
        credit: PortraitCredit {
            author,
            license,
            license_url,
            file_page_url: file_page_url.to_owned(),
        },
    }))
}

pub(crate) fn get(url: &str) -> Result<String, FetchError> {
    if !is_metadata_url(url) {
        return Err(FetchError::Transport);
    }
    let _ = wait_for_slot(RateLimitKey::Wikimedia, &mut || false);
    let response = agent()
        .get(url)
        .call()
        .map_err(|error| map_ureq_error(&error))?;
    read_bounded(response, MAX_RESPONSE_BYTES)
}

pub(crate) fn download_image(url: &str) -> Result<Vec<u8>, FetchError> {
    if !is_download_url(url) {
        return Err(FetchError::Transport);
    }
    let _ = wait_for_slot(RateLimitKey::Wikimedia, &mut || false);
    let response = agent()
        .get(url)
        .call()
        .map_err(|error| map_ureq_error(&error))?;
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| FetchError::Body)?;
    (bytes.len() as u64 <= MAX_IMAGE_BYTES)
        .then_some(bytes)
        .ok_or(FetchError::BodyTooLarge)
}

pub(crate) fn is_download_url(value: &str) -> bool {
    url::Url::parse(value)
        .is_ok_and(|url| url.scheme() == "https" && url.host_str() == Some("upload.wikimedia.org"))
}

fn is_metadata_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && matches!(
                url.host_str(),
                Some("www.wikidata.org" | "commons.wikimedia.org")
            )
    })
}

fn strip_html(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output.trim().to_owned()
}

fn agent() -> &'static ureq::Agent {
    AGENT.get_or_init(|| build_agent(AgentPolicy::strict(HTTP_TIMEOUT)))
}

fn read_bounded(
    response: ureq::http::Response<ureq::Body>,
    limit: u64,
) -> Result<String, FetchError> {
    let mut body = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(limit + 1)
        .read_to_end(&mut body)
        .map_err(|_| FetchError::Body)?;
    if body.len() as u64 > limit {
        return Err(FetchError::BodyTooLarge);
    }
    String::from_utf8(body).map_err(|_| FetchError::Body)
}

fn map_ureq_error(error: &ureq::Error) -> FetchError {
    match error {
        ureq::Error::StatusCode(status) => FetchError::HttpStatus(*status),
        ureq::Error::Timeout(_) => FetchError::Timeout,
        _ => FetchError::Transport,
    }
}
