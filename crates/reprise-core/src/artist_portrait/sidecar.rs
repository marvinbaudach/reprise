//! Provenance stored beside every cached artist portrait.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortraitSource {
    Deezer,
    Commons,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortraitCredit {
    pub author: String,
    pub license: String,
    pub license_url: String,
    pub file_page_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortraitRecord {
    pub source: PortraitSource,
    pub credit: Option<PortraitCredit>,
}

const MAX_CREDIT_TEXT_CHARS: usize = 256;
const MAX_CREDIT_URL_CHARS: usize = 2_048;

impl PortraitRecord {
    pub(crate) const fn deezer() -> Self {
        Self {
            source: PortraitSource::Deezer,
            credit: None,
        }
    }

    pub(crate) const fn commons(credit: PortraitCredit) -> Self {
        Self {
            source: PortraitSource::Commons,
            credit: Some(credit),
        }
    }
}

pub(crate) fn path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.json", super::cache::key_for(name)))
}

pub(crate) fn read(dir: &Path, name: &str) -> Option<PortraitRecord> {
    let bytes = std::fs::read(path(dir, name)).ok()?;
    let record: PortraitRecord = serde_json::from_slice(&bytes).ok()?;
    match (record.source, record.credit) {
        (PortraitSource::Deezer, None) => Some(PortraitRecord::deezer()),
        (PortraitSource::Commons, Some(credit)) => {
            validated_credit(credit).map(PortraitRecord::commons)
        }
        (PortraitSource::Deezer, Some(_)) | (PortraitSource::Commons, None) => None,
    }
}

pub(crate) fn encoded(record: &PortraitRecord) -> Option<Vec<u8>> {
    serde_json::to_vec(record).ok()
}

pub(crate) fn load_credit(dir: &Path, name: &str) -> Option<PortraitCredit> {
    read(dir, name)?.credit
}

pub(crate) fn validated_credit(mut credit: PortraitCredit) -> Option<PortraitCredit> {
    credit.author = bounded_text(&credit.author);
    credit.license = bounded_text(&credit.license);
    if credit.author.is_empty() || credit.license.is_empty() {
        return None;
    }
    credit.file_page_url = bounded_url(&credit.file_page_url);
    if !url_has_origin(&credit.file_page_url, "commons.wikimedia.org") {
        return None;
    }
    credit.license_url = bounded_url(&credit.license_url);
    if !credit.license_url.is_empty()
        && ![
            "creativecommons.org",
            "www.gnu.org",
            "commons.wikimedia.org",
        ]
        .into_iter()
        .any(|host| url_has_origin(&credit.license_url, host))
    {
        credit.license_url.clear();
    }
    Some(credit)
}

pub(crate) fn bounded_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_CREDIT_TEXT_CHARS)
        .collect()
}

pub(crate) fn bounded_url(value: &str) -> String {
    value.trim().chars().take(MAX_CREDIT_URL_CHARS).collect()
}

fn url_has_origin(value: &str, host: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| url.scheme() == "https" && url.host_str() == Some(host))
}
