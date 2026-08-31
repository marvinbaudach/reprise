//! Projection of yt-dlp flat-playlist responses.

use serde_json::Value;

use super::{response_error, PodcastError, YtDlpPlaylist, YtDlpVideo};

pub(super) fn parse(operation: &'static str, body: &str) -> Result<YtDlpPlaylist, PodcastError> {
    let value: Value = serde_json::from_str(body).map_err(|_| response_error(operation))?;
    let raw_entries = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| response_error(operation))?;
    let source_url = raw_entries
        .iter()
        .find_map(|entry| {
            entry
                .get("channel_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|id| format!("https://www.youtube.com/channel/{id}"))
        })
        .or_else(|| super::super::ytdlp_search::stable_source_url(&value))
        .or_else(|| {
            raw_entries
                .iter()
                .find_map(super::super::ytdlp_search::stable_source_url)
        });
    let entries = raw_entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.trim().to_string();
            let title = entry.get("title")?.as_str()?.trim().to_string();
            if id.is_empty() || title.is_empty() {
                return None;
            }
            Some(YtDlpVideo {
                id,
                title,
                duration_secs: duration_secs(entry.get("duration")),
                timestamp: integer_value(entry.get("timestamp")),
                upload_date: entry
                    .get("upload_date")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned),
                image_url: super::super::ytdlp_search::entry_image_url(entry),
            })
        })
        .collect();

    Ok(YtDlpPlaylist {
        title: value
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        channel: std::iter::once(&value)
            .chain(raw_entries.iter())
            .find_map(super::super::ytdlp_search::channel_name),
        source_url,
        image_url: super::super::ytdlp_search::listing_image_url(&value),
        entries,
    })
}

pub(super) fn duration_secs(value: Option<&Value>) -> Option<i64> {
    value
        .and_then(|value| {
            value
                .as_f64()
                .or_else(|| value.as_str()?.parse::<f64>().ok())
        })
        .filter(|duration| duration.is_finite() && *duration >= 0.0)
        .map(|duration| duration as i64)
}

fn integer_value(value: Option<&Value>) -> Option<i64> {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str()?.parse::<i64>().ok())
        })
        .filter(|value| *value >= 0)
}
