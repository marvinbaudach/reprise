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

pub(super) fn channel_name(value: &Value) -> Option<String> {
    non_empty_json_string(value, "channel").or_else(|| non_empty_json_string(value, "uploader"))
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
pub(super) fn entry_follower_count(entry: &Value) -> Option<u64> {
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

/// The channel's own avatar out of a `--flat-playlist` channel dump.
///
/// [`entry_image_url`] must not be reused here. A channel dump carries no flat
/// `thumbnail` field at all, and its `thumbnails[0]` is the *banner* — measured
/// 2026-08-18 against `youtube.com/@kurzgesagt/videos`: six banner crops from
/// 1060×175 to 2560×424, then `banner_uncropped`, a square 900×900 avatar and
/// `avatar_uncropped`. A 6:1 strip in a square 40 px tile looks worse than the
/// episode cover this replaces, so the banner is never a candidate.
///
/// The square entry wins over `avatar_uncropped` on purpose: the uncropped
/// variants carry no `width`/`height` at all and resolve to `=s0`, the
/// ungoverned original, while the square one is the ready-made `=s900`.
/// `remote_image` places no ceiling on what it downloads, so that difference is
/// paid on every cache miss.
pub(super) fn channel_avatar_url(value: &Value) -> Option<String> {
    let thumbnails = value.get("thumbnails").and_then(Value::as_array)?;
    let square = thumbnails
        .iter()
        .filter_map(|thumbnail| {
            let width = positive_dimension(thumbnail, "width")?;
            let height = positive_dimension(thumbnail, "height")?;
            if width != height {
                return None;
            }
            Some((width, non_empty_json_string(thumbnail, "url")?))
        })
        .max_by_key(|(width, _)| *width)
        .map(|(_, url)| url);
    square.or_else(|| {
        thumbnails
            .iter()
            .find(|thumbnail| {
                non_empty_json_string(thumbnail, "id").as_deref() == Some("avatar_uncropped")
            })
            .and_then(|thumbnail| non_empty_json_string(thumbnail, "url"))
    })
}

/// A listing's own picture, for every URL form `YtDlp::list` accepts.
///
/// `url_detect` routes channels, playlists (`/playlist`, `?list=`) and single
/// videos (`youtu.be/…`) into the same subscribe flow, so this one function has
/// to serve all three. A channel is served by its avatar; a playlist or video
/// has no avatar at all and is served by its own cover — measured 2026-08-18
/// against `playlist?list=PLFgquLnL59al…`: four 16:9 crops from 168×94 to
/// 336×188, no square entry and no `avatar_uncropped`.
///
/// What must never happen for either form is the *banner*, which is what
/// [`entry_image_url`]'s `thumbnails[0]` yields on a channel dump. So the cover
/// fallback skips banner-shaped entries instead of trusting the first one: a
/// channel without an avatar keeps its glyph rather than wearing a 6:1 strip.
pub(super) fn listing_image_url(value: &Value) -> Option<String> {
    channel_avatar_url(value).or_else(|| listing_cover_url(value))
}

/// Widest aspect ratio still treated as a cover rather than a banner. The
/// measured extremes are far apart — 1.79 for a playlist crop, 6.06 for the
/// narrowest banner crop — so the exact cut only has to sit between them.
const MAX_COVER_ASPECT: u64 = 3;

fn listing_cover_url(value: &Value) -> Option<String> {
    value
        .get("thumbnails")
        .and_then(Value::as_array)?
        .iter()
        .filter(|thumbnail| {
            if non_empty_json_string(thumbnail, "id").as_deref() == Some("banner_uncropped") {
                return false;
            }
            // Entries without usable dimensions stay eligible: a cover-only dump
            // may omit them, and the banner ids are already excluded above.
            match (
                positive_dimension(thumbnail, "width"),
                positive_dimension(thumbnail, "height"),
            ) {
                (Some(width), Some(height)) => width <= height.saturating_mul(MAX_COVER_ASPECT),
                _ => true,
            }
        })
        .find_map(|thumbnail| non_empty_json_string(thumbnail, "url"))
}

fn positive_dimension(thumbnail: &Value, key: &str) -> Option<u64> {
    let value = thumbnail.get(key)?;
    let dimension = value.as_u64().or_else(|| {
        value
            .as_f64()
            // Defence in depth rather than a reachable case: `INFINITY >= 0.0`
            // holds and the cast below would saturate to `u64::MAX`, winning
            // every "largest square" comparison — but `serde_json` rejects an
            // out-of-range literal while parsing ("number out of range") and
            // cannot represent a non-finite `Value` at all, so no yt-dlp output
            // can reach this branch. Kept because it costs one comparison.
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(|value| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    value as u64
                }
            })
    })?;
    (dimension > 0).then_some(dimension)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The channel dump measured 2026-08-18 against `@kurzgesagt/videos`,
    /// trimmed to the fields the selection reads. Note there is no flat
    /// `thumbnail` key on channel level, and `thumbnails[0]` is a banner.
    fn kurzgesagt_channel_dump() -> Value {
        serde_json::from_str(
            r#"{"title":"Kurzgesagt","thumbnails":[
              {"url":"https://yt3.googleusercontent.com/banner=w1060-fcrop64","height":175,"width":1060,"id":"0"},
              {"url":"https://yt3.googleusercontent.com/banner=w1138-fcrop64","height":188,"width":1138,"id":"1"},
              {"url":"https://yt3.googleusercontent.com/banner=w2560-fcrop64","height":424,"width":2560,"id":"5"},
              {"url":"https://yt3.googleusercontent.com/banner=s0","id":"banner_uncropped","preference":-5},
              {"url":"https://yt3.googleusercontent.com/ytc/AIdro=s900-c-k-c0x00ffffff-no-rj","height":900,"width":900,"id":"7"},
              {"url":"https://yt3.googleusercontent.com/ytc/AIdro=s0","id":"avatar_uncropped","preference":-5}
            ]}"#,
        )
        .unwrap()
    }

    #[test]
    fn a_channel_dump_yields_the_square_avatar_not_the_banner() {
        assert_eq!(
            channel_avatar_url(&kurzgesagt_channel_dump()).as_deref(),
            Some("https://yt3.googleusercontent.com/ytc/AIdro=s900-c-k-c0x00ffffff-no-rj")
        );
    }

    #[test]
    fn the_video_level_rule_would_have_picked_the_banner() {
        // Guards the reason `channel_avatar_url` exists at all: reusing
        // `entry_image_url` on channel level hands out the 6:1 banner strip.
        assert_eq!(
            entry_image_url(&kurzgesagt_channel_dump()).as_deref(),
            Some("https://yt3.googleusercontent.com/banner=w1060-fcrop64")
        );
    }

    #[test]
    fn a_channel_with_only_banners_has_no_avatar() {
        let value: Value = serde_json::from_str(
            r#"{"thumbnails":[
              {"url":"https://yt3.googleusercontent.com/banner=w1060","height":175,"width":1060,"id":"0"},
              {"url":"https://yt3.googleusercontent.com/banner=s0","id":"banner_uncropped"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(channel_avatar_url(&value), None);
    }

    #[test]
    fn avatar_uncropped_carries_the_channel_when_no_square_entry_does() {
        let value: Value = serde_json::from_str(
            r#"{"thumbnails":[
              {"url":"https://yt3.googleusercontent.com/banner=w1060","height":175,"width":1060,"id":"0"},
              {"url":"https://yt3.googleusercontent.com/ytc/only=s0","id":"avatar_uncropped"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            channel_avatar_url(&value).as_deref(),
            Some("https://yt3.googleusercontent.com/ytc/only=s0")
        );
    }

    #[test]
    fn the_largest_square_entry_wins() {
        let value: Value = serde_json::from_str(
            r#"{"thumbnails":[
              {"url":"https://yt3.googleusercontent.com/ytc/small=s88","height":88,"width":88,"id":"0"},
              {"url":"https://yt3.googleusercontent.com/ytc/large=s900","height":900,"width":900,"id":"7"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            channel_avatar_url(&value).as_deref(),
            Some("https://yt3.googleusercontent.com/ytc/large=s900")
        );
    }

    #[test]
    fn a_zero_sized_entry_is_not_square() {
        let value: Value = serde_json::from_str(
            r#"{"thumbnails":[{"url":"https://yt3.googleusercontent.com/ytc/zero","height":0,"width":0,"id":"0"}]}"#,
        )
        .unwrap();
        assert_eq!(channel_avatar_url(&value), None);
    }

    #[test]
    fn a_dump_without_usable_thumbnails_has_no_avatar() {
        for body in [
            r#"{"title":"No thumbnails"}"#,
            r#"{"thumbnails":[]}"#,
            r#"{"thumbnails":"not an array"}"#,
            r#"{"thumbnails":[{"height":900,"width":900,"id":"7"}]}"#,
        ] {
            let value: Value = serde_json::from_str(body).unwrap();
            assert_eq!(channel_avatar_url(&value), None, "body: {body}");
        }
    }

    /// A playlist dump measured 2026-08-18 against
    /// `playlist?list=PLFgquLnL59al…`: no square entry, no `avatar_uncropped`,
    /// just its own 16:9 cover crops.
    fn playlist_dump() -> Value {
        serde_json::from_str(
            r#"{"title":"Popular Music Videos","thumbnails":[
              {"url":"https://i.ytimg.com/vi/fOT0BUpITw8/hqdefault.jpg?s=168","height":94,"width":168,"id":"0"},
              {"url":"https://i.ytimg.com/vi/fOT0BUpITw8/hqdefault.jpg?s=336","height":188,"width":336,"id":"3"}
            ]}"#,
        )
        .unwrap()
    }

    #[test]
    fn a_playlist_keeps_its_own_cover() {
        // `url_detect` routes `?list=` and `youtu.be` links into the same
        // subscribe flow as channels, and those have no avatar to find. The
        // channel rule alone would leave them permanently without a picture.
        assert_eq!(
            listing_image_url(&playlist_dump()).as_deref(),
            Some("https://i.ytimg.com/vi/fOT0BUpITw8/hqdefault.jpg?s=168")
        );
    }

    #[test]
    fn a_channel_prefers_its_avatar_over_any_cover_fallback() {
        assert_eq!(
            listing_image_url(&kurzgesagt_channel_dump()).as_deref(),
            Some("https://yt3.googleusercontent.com/ytc/AIdro=s900-c-k-c0x00ffffff-no-rj")
        );
    }

    #[test]
    fn a_channel_without_an_avatar_never_falls_back_to_its_banner() {
        let value: Value = serde_json::from_str(
            r#"{"thumbnails":[
              {"url":"https://yt3.googleusercontent.com/banner=w1060","height":175,"width":1060,"id":"0"},
              {"url":"https://yt3.googleusercontent.com/banner=w2560","height":424,"width":2560,"id":"5"},
              {"url":"https://yt3.googleusercontent.com/banner=s0","id":"banner_uncropped"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(listing_image_url(&value), None);
    }

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
