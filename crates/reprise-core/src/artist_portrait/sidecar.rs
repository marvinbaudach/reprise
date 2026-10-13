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
    match (&record.source, &record.credit) {
        (PortraitSource::Deezer, None) | (PortraitSource::Commons, Some(_)) => Some(record),
        (PortraitSource::Deezer, Some(_)) | (PortraitSource::Commons, None) => None,
    }
}

pub(crate) fn encoded(record: &PortraitRecord) -> Option<Vec<u8>> {
    serde_json::to_vec(record).ok()
}

pub(crate) fn load_credit(dir: &Path, name: &str) -> Option<PortraitCredit> {
    read(dir, name)?.credit
}
