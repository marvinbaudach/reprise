use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DoctorScopeArg {
    WholeLibrary,
    CurrentView,
    Selection,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ScanTagsParams {
    pub scope: DoctorScopeArg,
    #[serde(default)]
    pub track_ids: Option<Vec<i64>>,
    #[serde(default)]
    pub remote: Option<bool>,
    #[serde(default)]
    pub apply_safe: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ScanTagsResult {
    pub scan_id: i64,
    pub applied: usize,
    pub needs_review: usize,
    pub conflicts: usize,
    pub checked: usize,
    pub skipped: usize,
}

impl ScanTagsResult {
    pub fn summary(&self) -> String {
        format!(
            "Scanned {} track(s): {} applied, {} need review, {} conflict group(s)",
            self.checked, self.applied, self.needs_review, self.conflicts
        )
    }
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DoctorCategoryArg {
    Casing,
    Year,
    Genre,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ReviewTagsParams {
    #[serde(default)]
    pub category: Option<DoctorCategoryArg>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ReviewTagsResult {
    pub scan_id: i64,
    pub albums: Vec<DoctorAlbumDto>,
    pub conflicts: Vec<DoctorConflictDto>,
    pub change_count: usize,
    pub total_albums: usize,
    pub offset: usize,
    pub limit: usize,
    pub returned_albums: usize,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
pub struct DoctorConflictDto {
    pub group_key: String,
    pub field: reprise_core::library_doctor::DoctorField,
    pub track_ids: Vec<i64>,
    pub candidates: Vec<DoctorCandidateDto>,
}

#[derive(Debug, Serialize)]
pub struct DoctorCandidateDto {
    pub value: reprise_core::library_doctor::DoctorValue,
    pub applies_to_tracks: usize,
}

impl ReviewTagsResult {
    pub fn summary(&self) -> String {
        format!(
            "{} tag change(s) across {} returned album(s)",
            self.change_count, self.returned_albums
        )
    }
}

#[derive(Debug, Serialize)]
pub struct DoctorAlbumDto {
    pub album_key: String,
    pub title: String,
    pub artist: String,
    pub track_count: usize,
    pub change_count: usize,
    pub rows: Vec<DoctorReviewRowDto>,
}

#[derive(Debug, Serialize)]
pub struct DoctorReviewRowDto {
    pub row_ids: Vec<u64>,
    pub track_ids: Vec<i64>,
    pub applies_to_tracks: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_title: Option<String>,
    pub field: reprise_core::library_doctor::DoctorField,
    pub current: reprise_core::library_doctor::DoctorValue,
    pub proposed: reprise_core::library_doctor::DoctorValue,
    pub source: &'static str,
}

#[derive(Debug, Clone, Copy, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplyTagsAction {
    Apply,
    Resolve,
    Revert,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ApplyTagsParams {
    pub action: ApplyTagsAction,
    #[serde(default)]
    pub row_ids: Option<Vec<u64>>,
    #[serde(default)]
    pub album_keys: Option<Vec<String>>,
    #[serde(default)]
    pub group_key: Option<String>,
    #[serde(default)]
    pub candidate: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApplyTagsResult {
    pub action: &'static str,
    pub applied: usize,
    pub reverted: usize,
    pub failed: usize,
    pub conflicts: usize,
    pub unavailable: usize,
    pub cancelled: usize,
    pub failures: Vec<DoctorFailureDto>,
}

impl ApplyTagsResult {
    pub fn summary(&self) -> String {
        match self.action {
            "revert" => format!("Reverted {} tag change(s)", self.reverted),
            _ => format!("Applied {} tag change(s)", self.applied),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DoctorFailureDto {
    pub track_id: i64,
    pub track_title: String,
    pub field: reprise_core::library_doctor::DoctorField,
    pub error_kind: &'static str,
}
