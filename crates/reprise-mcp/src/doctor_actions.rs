use std::collections::HashSet;
use std::path::Path;

use reprise_core::library_doctor::{
    group_review_rows, scan_summary, DoctorError, DoctorReviewDisplayRow, DoctorReviewFilter,
    DoctorReviewSession, DoctorScan, DoctorScanOptions, DoctorScanOutcome, DoctorScanRequest,
    DoctorScopeRequest, DoctorValue, DoctorViewSnapshot, DoctorWriteControl, DoctorWriteReport,
    DoctorWriteRow, DoctorWriteRowState, LibraryDoctor, ProblemClass, ProposalSource, ScanControl,
};

use crate::data::{self, DataError};
use crate::doctor_dto::{
    ApplyTagsAction, ApplyTagsParams, ApplyTagsResult, DoctorAlbumDto, DoctorCandidateDto,
    DoctorCategoryArg, DoctorConflictDto, DoctorFailureDto, DoctorReviewRowDto, DoctorScopeArg,
    ReviewTagsParams, ReviewTagsResult, ScanTagsParams, ScanTagsResult,
};

const DEFAULT_REVIEW_LIMIT: usize = 50;
const MAX_REVIEW_LIMIT: usize = 200;

pub fn scan_tags(
    path: &Path,
    tags_write_granted_at_startup: bool,
    params: &ScanTagsParams,
) -> Result<ScanTagsResult, DataError> {
    let db = data::open(path)?;
    data::require_read(&db)?;
    let apply_safe = params.apply_safe.unwrap_or(false);
    if apply_safe
        && !crate::capability::tags_write_effective(&db, tags_write_granted_at_startup)
            .map_err(DataError::Db)?
    {
        return Err(DataError::CapabilityDenied("tags:write"));
    }
    let remote_enabled = params.remote.unwrap_or(
        reprise_core::library_doctor::remote_suggestion_preference(&db)
            .map_err(DataError::Db)?
            .enabled,
    );
    let request = DoctorScanRequest {
        scope: scope_request(params)?,
        options: DoctorScanOptions { remote_enabled },
    };
    let mut doctor = LibraryDoctor::new(&db);
    let outcome = doctor
        .scan(&request, None, |_| ScanControl::Continue)
        .map_err(map_doctor_error)?;
    let DoctorScanOutcome::Completed(scan) = outcome else {
        return Err(DataError::InvalidInput(
            "the requested scan scope contains no present tracks".to_owned(),
        ));
    };
    let summary = scan_summary(&scan, remote_enabled);
    let applied = if apply_safe {
        doctor
            .apply_auto_tier(&scan, |_| DoctorWriteControl::Continue)
            .map_err(map_doctor_error)?
            .map_or(0, |report| {
                report
                    .rows
                    .iter()
                    .filter(|row| row.state == DoctorWriteRowState::Applied)
                    .count()
            })
    } else {
        0
    };
    Ok(ScanTagsResult {
        scan_id: scan.id,
        applied,
        needs_review: summary.review_changes,
        conflicts: summary.unresolved_groups,
        checked: summary.checked_tracks,
        skipped: summary.skipped_tracks,
    })
}

pub fn apply_tags(
    path: &Path,
    tags_write_granted_at_startup: bool,
    params: &ApplyTagsParams,
) -> Result<ApplyTagsResult, DataError> {
    let db = data::open(path)?;
    let allowed = crate::capability::tags_write_effective(&db, tags_write_granted_at_startup)
        .map_err(DataError::Db)?;
    if !allowed {
        return Err(DataError::CapabilityDenied("tags:write"));
    }
    match params.action {
        ApplyTagsAction::Apply => apply_selected(&db, params),
        ApplyTagsAction::Resolve => apply_resolution(&db, params),
        ApplyTagsAction::Revert => revert_cleanup(&db),
    }
}

fn apply_selected(
    db: &reprise_core::db::Db,
    params: &ApplyTagsParams,
) -> Result<ApplyTagsResult, DataError> {
    let scan = last_scan(db)?;
    let mut session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
    let mut requested = params
        .row_ids
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect::<HashSet<_>>();
    let album_keys = params
        .album_keys
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect::<HashSet<_>>();
    if requested.is_empty() && album_keys.is_empty() {
        return Err(DataError::InvalidInput(
            "apply requires at least one row_id or album_key".to_owned(),
        ));
    }
    let albums = group_review_rows(&scan, &session);
    for key in &album_keys {
        let album = albums
            .iter()
            .find(|album| &album.key == key)
            .ok_or_else(|| DataError::InvalidInput(format!("unknown album_key '{key}'")))?;
        for display in &album.rows {
            match display {
                DoctorReviewDisplayRow::Track { row_id, .. } => {
                    requested.insert(row_id.raw());
                }
                DoctorReviewDisplayRow::AllTracks { row_ids, .. } => {
                    requested.extend(row_ids.iter().map(|id| id.raw()));
                }
            }
        }
    }
    let available = session
        .rows()
        .iter()
        .map(|row| row.id.raw())
        .collect::<HashSet<_>>();
    if let Some(unknown) = requested.iter().find(|id| !available.contains(id)) {
        return Err(DataError::InvalidInput(format!(
            "unknown Library Doctor row_id {unknown}"
        )));
    }
    session.none();
    for row_id in requested {
        session
            .set_selected(
                reprise_core::library_doctor::DoctorReviewRowId::from_raw(row_id),
                true,
            )
            .map_err(|error| DataError::InvalidInput(error.to_string()))?;
    }
    apply_session(db, &scan, &session, "apply")
}

fn apply_resolution(
    db: &reprise_core::db::Db,
    params: &ApplyTagsParams,
) -> Result<ApplyTagsResult, DataError> {
    let group_key = params
        .group_key
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| DataError::InvalidInput("group_key is required for resolve".to_owned()))?;
    let candidate = params
        .candidate
        .as_deref()
        .ok_or_else(|| DataError::InvalidInput("candidate is required for resolve".to_owned()))?;
    let scan = last_scan(db)?;
    let mut session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
    let group = session
        .groups()
        .iter()
        .find(|group| group.group_key == group_key)
        .cloned()
        .ok_or_else(|| DataError::InvalidInput(format!("unknown group_key '{group_key}'")))?;
    let value = match group.field {
        reprise_core::library_doctor::DoctorField::Year => candidate
            .parse::<u32>()
            .map(DoctorValue::Year)
            .map_err(|_| DataError::InvalidInput("year candidate must be a number".to_owned()))?,
        _ if candidate.is_empty() => DoctorValue::Empty,
        _ => DoctorValue::Text(candidate.to_owned()),
    };
    session
        .choose_candidate(group.id, &value)
        .map_err(|error| DataError::InvalidInput(error.to_string()))?;
    apply_session(db, &scan, &session, "resolve")
}

fn apply_session(
    db: &reprise_core::db::Db,
    scan: &DoctorScan,
    session: &DoctorReviewSession,
    action: &'static str,
) -> Result<ApplyTagsResult, DataError> {
    let plan = session.freeze_plan();
    if plan.tag_change_count() == 0 {
        return Err(DataError::InvalidInput(
            "the requested action selected no ready tag changes".to_owned(),
        ));
    }
    let report = LibraryDoctor::new(db)
        .apply_review_plan(&plan, |_| DoctorWriteControl::Continue)
        .map_err(map_doctor_error)?;
    Ok(result_from_reports(
        action,
        Some(scan),
        std::slice::from_ref(&report),
    ))
}

fn revert_cleanup(db: &reprise_core::db::Db) -> Result<ApplyTagsResult, DataError> {
    let scan = LibraryDoctor::new(db)
        .last_complete_scan()
        .map_err(map_doctor_error)?;
    let report = LibraryDoctor::new(db)
        .revert_last_cleanup(|_| DoctorWriteControl::Continue)
        .map_err(map_doctor_error)?
        .ok_or_else(|| {
            DataError::InvalidInput("no revertible Library Doctor cleanup".to_owned())
        })?;
    Ok(result_from_reports(
        "revert",
        scan.as_ref(),
        &report.reports,
    ))
}

fn last_scan(db: &reprise_core::db::Db) -> Result<DoctorScan, DataError> {
    LibraryDoctor::new(db)
        .last_complete_scan()
        .map_err(map_doctor_error)?
        .ok_or_else(|| DataError::InvalidInput("no completed Library Doctor scan".to_owned()))
}

fn result_from_reports(
    action: &'static str,
    scan: Option<&DoctorScan>,
    reports: &[DoctorWriteReport],
) -> ApplyTagsResult {
    let rows = reports
        .iter()
        .flat_map(|report| report.rows.iter())
        .collect::<Vec<_>>();
    let count = |state| rows.iter().filter(|row| row.state == state).count();
    let failures = rows
        .iter()
        .filter(|row| {
            matches!(
                row.state,
                DoctorWriteRowState::Failed
                    | DoctorWriteRowState::Conflict
                    | DoctorWriteRowState::Unavailable
            )
        })
        .map(|row| failure_dto(scan, row))
        .collect();
    ApplyTagsResult {
        action,
        applied: count(DoctorWriteRowState::Applied),
        reverted: count(DoctorWriteRowState::Reverted),
        failed: count(DoctorWriteRowState::Failed),
        conflicts: count(DoctorWriteRowState::Conflict),
        unavailable: count(DoctorWriteRowState::Unavailable),
        cancelled: count(DoctorWriteRowState::Cancelled),
        failures,
    }
}

fn failure_dto(scan: Option<&DoctorScan>, row: &DoctorWriteRow) -> DoctorFailureDto {
    let track_title = scan
        .and_then(|scan| {
            scan.tracks
                .iter()
                .find(|track| track.reference.track_id == row.track_id)
                .and_then(|track| track.tags.as_ref())
        })
        .map_or_else(
            || format!("Track {}", row.track_id),
            |tags| tags.title.clone(),
        );
    DoctorFailureDto {
        track_id: row.track_id,
        track_title,
        field: row.field,
        error_kind: write_error_kind(row),
    }
}

fn write_error_kind(row: &DoctorWriteRow) -> &'static str {
    use reprise_core::library::tag_edit::WriteErrorKind;
    match row.error_kind {
        Some(WriteErrorKind::PermissionDenied) => "permission_denied",
        Some(WriteErrorKind::NotFound) => "not_found",
        Some(WriteErrorKind::UnsupportedFormat) => "unsupported_format",
        Some(WriteErrorKind::UnreadableTags) => "unreadable_tags",
        Some(WriteErrorKind::Io) => "io",
        None if row.state == DoctorWriteRowState::Conflict => "conflict",
        None if row.state == DoctorWriteRowState::Unavailable => "unavailable",
        None => "failed",
    }
}

pub fn review_tags(path: &Path, params: &ReviewTagsParams) -> Result<ReviewTagsResult, DataError> {
    let db = data::open(path)?;
    data::require_read(&db)?;
    let scan = LibraryDoctor::new(&db)
        .last_complete_scan()
        .map_err(map_doctor_error)?
        .ok_or_else(|| DataError::InvalidInput("no completed Library Doctor scan".to_owned()))?;
    let scan = filtered_scan(scan, params.category);
    let session = DoctorReviewSession::from_scan(scan.clone(), DoctorReviewFilter::NeedsReview);
    let conflicts = session
        .groups()
        .iter()
        .map(|group| {
            let track_ids = scan
                .unresolved_groups
                .iter()
                .find(|stored| stored.group_key == group.group_key && stored.field == group.field)
                .map(|stored| {
                    stored
                        .members
                        .iter()
                        .map(|member| member.track_id)
                        .collect()
                })
                .unwrap_or_default();
            DoctorConflictDto {
                group_key: group.group_key.clone(),
                field: group.field,
                track_ids,
                candidates: group
                    .candidates
                    .iter()
                    .map(|candidate| DoctorCandidateDto {
                        value: candidate.value.clone(),
                        applies_to_tracks: candidate.count,
                    })
                    .collect(),
            }
        })
        .collect();
    let albums = group_review_rows(&scan, &session);
    let total_albums = albums.len();
    let offset = usize::try_from(params.offset.unwrap_or(0)).unwrap_or(usize::MAX);
    let limit = usize::try_from(params.limit.unwrap_or(DEFAULT_REVIEW_LIMIT as u32))
        .unwrap_or(MAX_REVIEW_LIMIT)
        .clamp(1, MAX_REVIEW_LIMIT);
    let page = albums
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|album| album_dto(&scan, &session, album))
        .collect::<Vec<_>>();
    let returned_albums = page.len();
    let change_count = page.iter().map(|album| album.change_count).sum();
    Ok(ReviewTagsResult {
        scan_id: scan.id,
        albums: page,
        conflicts,
        change_count,
        total_albums,
        offset,
        limit,
        returned_albums,
        has_more: offset.saturating_add(returned_albums) < total_albums,
    })
}

fn filtered_scan(mut scan: DoctorScan, category: Option<DoctorCategoryArg>) -> DoctorScan {
    let Some(category) = category else {
        return scan;
    };
    scan.proposals
        .retain(|proposal| category_matches(category, proposal.problem_class));
    scan.unresolved_groups.retain(|group| {
        category_matches(
            category,
            match group.field {
                reprise_core::library_doctor::DoctorField::Year => ProblemClass::MissingWrongYear,
                reprise_core::library_doctor::DoctorField::Genre => ProblemClass::GenreVariant,
                _ => ProblemClass::CasingWhitespace,
            },
        )
    });
    scan
}

const fn category_matches(category: DoctorCategoryArg, class: ProblemClass) -> bool {
    match category {
        DoctorCategoryArg::Casing => matches!(
            class,
            ProblemClass::CasingWhitespace | ProblemClass::MissingAlbumArtist
        ),
        DoctorCategoryArg::Year => matches!(class, ProblemClass::MissingWrongYear),
        DoctorCategoryArg::Genre => matches!(class, ProblemClass::GenreVariant),
    }
}

fn album_dto(
    scan: &DoctorScan,
    session: &DoctorReviewSession,
    album: reprise_core::library_doctor::DoctorReviewAlbum,
) -> DoctorAlbumDto {
    let rows = album
        .rows
        .into_iter()
        .map(|display| review_row_dto(scan, session, display))
        .collect();
    DoctorAlbumDto {
        album_key: album.key,
        title: album.title,
        artist: album.artist,
        track_count: album.track_count,
        change_count: album.change_count,
        rows,
    }
}

fn review_row_dto(
    scan: &DoctorScan,
    session: &DoctorReviewSession,
    display: DoctorReviewDisplayRow,
) -> DoctorReviewRowDto {
    let (row_ids, track_ids, applies_to_tracks) = match display {
        DoctorReviewDisplayRow::Track { row_id, track_id } => (vec![row_id], vec![track_id], 1),
        DoctorReviewDisplayRow::AllTracks {
            row_ids,
            track_count,
        } => {
            let track_ids = row_ids
                .iter()
                .filter_map(|id| session.rows().iter().find(|row| row.id == *id))
                .map(|row| row.track_id)
                .collect();
            (row_ids, track_ids, track_count)
        }
    };
    let row = session
        .rows()
        .iter()
        .find(|row| row.id == row_ids[0])
        .expect("grouped review row must belong to the session");
    let track_title = (track_ids.len() == 1).then(|| {
        scan.tracks
            .iter()
            .find(|track| track.reference.track_id == track_ids[0])
            .and_then(|track| track.tags.as_ref())
            .map(|tags| tags.title.clone())
            .unwrap_or_default()
    });
    DoctorReviewRowDto {
        row_ids: row_ids
            .into_iter()
            .map(reprise_core::library_doctor::DoctorReviewRowId::raw)
            .collect(),
        track_ids,
        applies_to_tracks,
        track_title,
        field: row.field,
        current: row.current.clone(),
        proposed: row.proposed.clone(),
        source: proposal_source(row.source),
    }
}

const fn proposal_source(source: ProposalSource) -> &'static str {
    match source {
        ProposalSource::Local => "local",
        ProposalSource::MusicBrainz => "musicbrainz",
        ProposalSource::AcoustId => "acoustid",
    }
}

fn scope_request(params: &ScanTagsParams) -> Result<DoctorScopeRequest, DataError> {
    match params.scope {
        DoctorScopeArg::WholeLibrary => {
            if params.track_ids.as_ref().is_some_and(|ids| !ids.is_empty()) {
                return Err(DataError::InvalidInput(
                    "track_ids are not accepted for whole_library scope".to_owned(),
                ));
            }
            Ok(DoctorScopeRequest::WholeLibrary)
        }
        DoctorScopeArg::Selection | DoctorScopeArg::CurrentView => {
            let track_ids = params
                .track_ids
                .clone()
                .filter(|ids| !ids.is_empty())
                .ok_or_else(|| {
                    DataError::InvalidInput(
                        "track_ids are required for selection and current_view scopes".to_owned(),
                    )
                })?;
            if matches!(params.scope, DoctorScopeArg::Selection) {
                Ok(DoctorScopeRequest::Selection { track_ids })
            } else {
                Ok(DoctorScopeRequest::CurrentView(Box::new(
                    DoctorViewSnapshot {
                        source: reprise_core::view_source::ViewSource::Queue,
                        sort_field: "id".to_owned(),
                        sort_dir: "asc".to_owned(),
                        filter: String::new(),
                        browse: reprise_core::queries::BrowseFilter::default(),
                        queue_ids: track_ids,
                    },
                )))
            }
        }
    }
}

pub(crate) fn map_doctor_error(error: DoctorError) -> DataError {
    match error {
        DoctorError::Database(error) => DataError::Db(error),
        DoctorError::TagWriteBusy(_) => DataError::TagWriteBusy,
        DoctorError::InvalidStoredData(message) => DataError::Internal(message),
    }
}
