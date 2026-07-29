//! Projection of yt-dlp video search output into stable channel results.

use serde_json::Value;

use super::PodcastError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YtDlpChannel {
    pub id: String,
    pub title: String,
    pub url: String,
    pub image_url: Option<String>,
    pub matching_video_count: usize,
    pub matching_video_ids: Vec<String>,
    /// yt-dlp's optional `channel_follower_count`. `None` whenever the channel
    /// hides it or the provider omits it — never a substituted zero.
    pub follower_count: Option<u64>,
}

pub(super) fn parse_search_channels(body: &str) -> Result<Vec<YtDlpChannel>, PodcastError> {
    let value: Value =
        serde_json::from_str(body).map_err(|error| PodcastError::Parse(error.to_string()))?;
    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| PodcastError::Parse("yt-dlp response has no entries".to_string()))?;
    let mut channels = Vec::<YtDlpChannel>::new();
    for entry in entries {
        let channel_id = non_empty_json_string(entry, "channel_id");
        let uploader_id = non_empty_json_string(entry, "uploader_id");
        let Some(id) = channel_id.clone().or(uploader_id) else {
            continue;
        };
        if let Some(channel) = channels.iter_mut().find(|channel| channel.id == id) {
            channel.matching_video_count += 1;
            if let Some(video_id) = non_empty_json_string(entry, "id") {
                channel.matching_video_ids.push(video_id);
            }
            if channel.image_url.is_none() {
                channel.image_url = entry_image_url(entry);
            }
            if channel.follower_count.is_none() {
                channel.follower_count = entry_follower_count(entry);
            }
            continue;
        }
        let Some(title) = non_empty_json_string(entry, "channel")
            .or_else(|| non_empty_json_string(entry, "uploader"))
        else {
            continue;
        };
        let url = channel_id
            .map(|id| format!("https://www.youtube.com/channel/{id}"))
            .or_else(|| non_empty_json_string(entry, "channel_url"))
            .or_else(|| non_empty_json_string(entry, "uploader_url"))
            .unwrap_or_else(|| format!("https://www.youtube.com/{id}"));
        channels.push(YtDlpChannel {
            id,
            title,
            url,
            image_url: entry_image_url(entry),
            matching_video_count: 1,
            matching_video_ids: non_empty_json_string(entry, "id").into_iter().collect(),
            follower_count: entry_follower_count(entry),
        });
    }
    Ok(channels)
}

fn non_empty_json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(super) fn stable_source_url(value: &Value) -> Option<String> {
    non_empty_json_string(value, "channel_id")
        .map(|id| format!("https://www.youtube.com/channel/{id}"))
        .or_else(|| non_empty_json_string(value, "channel_url"))
        .or_else(|| non_empty_json_string(value, "uploader_url"))
        .or_else(|| {
            non_empty_json_string(value, "uploader_id")
                .map(|id| format!("https://www.youtube.com/{id}"))
        })
}

/// yt-dlp reports `channel_follower_count` on video entries, but only when the
/// channel publishes it. A hidden count is absent or null, and some responses
/// carry a float — none of those may become a visible zero.
fn entry_follower_count(entry: &Value) -> Option<u64> {
    let value = entry.get("channel_follower_count")?;
    if let Some(count) = value.as_u64() {
        return Some(count);
    }
    let count = value.as_f64()?;
    if count.is_finite() && count >= 0.0 {
        // `as` saturates at the integer bounds for finite floats.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(count as u64)
    } else {
        None
    }
}

pub(super) fn entry_image_url(entry: &Value) -> Option<String> {
    non_empty_json_string(entry, "thumbnail").or_else(|| {
        entry
            .get("thumbnails")
            .and_then(Value::as_array)?
            .iter()
            .find_map(|thumbnail| non_empty_json_string(thumbnail, "url"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn src_9_hidden_or_malformed_follower_counts_never_become_zero() {
        let channels = parse_search_channels(
            r#"{"entries":[
              {"id":"v1","title":"A","channel_id":"UC-a","channel":"Visible","channel_follower_count":62400},
              {"id":"v2","title":"B","channel_id":"UC-b","channel":"Hidden"},
              {"id":"v3","title":"C","channel_id":"UC-c","channel":"Null","channel_follower_count":null},
              {"id":"v4","title":"D","channel_id":"UC-d","channel":"Float","channel_follower_count":1200000.0},
              {"id":"v5","title":"E","channel_id":"UC-e","channel":"Bogus","channel_follower_count":"many"}
            ]}"#,
        )
        .unwrap();

        let by_title = |title: &str| {
            channels
                .iter()
                .find(|channel| channel.title == title)
                .unwrap()
                .follower_count
        };

        assert_eq!(by_title("Visible"), Some(62_400));
        assert_eq!(by_title("Float"), Some(1_200_000));
        assert_eq!(by_title("Hidden"), None, "an absent count stays absent");
        assert_eq!(by_title("Null"), None, "a null count stays absent");
        assert_eq!(by_title("Bogus"), None, "a malformed count stays absent");
    }

    #[test]
    fn search_channels_groups_video_matches_by_canonical_channel() {
        let channels = parse_search_channels(
            r#"{"entries":[
              {"id":"v1","title":"First","channel_id":"UC-one","channel":"Rust Audio","channel_url":"https://www.youtube.com/@rust"},
              {"id":"v2","title":"Second","channel_id":"UC-one","channel":"Rust Audio","thumbnail":"https://img.test/two.jpg"},
              {"id":"v3","title":"Third","uploader_id":"@ferris","uploader":"Ferris FM","uploader_url":"https://www.youtube.com/@ferris","thumbnails":[{"url":"https://img.test/ferris.jpg"}]},
              {"id":"v4","title":"No channel"}
            ]}"#,
        )
        .unwrap();

        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].id, "UC-one");
        assert_eq!(channels[0].title, "Rust Audio");
        assert_eq!(channels[0].url, "https://www.youtube.com/channel/UC-one");
        assert_eq!(
            channels[0].image_url.as_deref(),
            Some("https://img.test/two.jpg")
        );
        assert_eq!(channels[0].matching_video_count, 2);
        assert_eq!(channels[0].matching_video_ids, ["v1", "v2"]);
        assert_eq!(channels[1].id, "@ferris");
        assert_eq!(channels[1].url, "https://www.youtube.com/@ferris");
        assert_eq!(channels[1].matching_video_count, 1);
        assert_eq!(channels[1].matching_video_ids, ["v3"]);
    }
}
