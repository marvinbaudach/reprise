//! RSS and Atom feed parsing.

use chrono::DateTime;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::PodcastError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedFeed {
    pub title: String,
    pub author: Option<String>,
    pub image_url: Option<String>,
    pub episodes: Vec<ParsedEpisode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedEpisode {
    pub guid: String,
    pub title: String,
    pub audio_url: String,
    pub page_url: Option<String>,
    pub published_at: Option<i64>,
    pub duration_secs: Option<i64>,
}

#[derive(Default)]
struct EpisodeBuilder {
    guid: Option<String>,
    title: Option<String>,
    audio_enclosure: Option<String>,
    fallback_enclosure: Option<String>,
    page_url: Option<String>,
    published_at: Option<i64>,
    duration_secs: Option<i64>,
}

impl EpisodeBuilder {
    fn enclosure(&mut self, url: String, content_type: Option<&str>) {
        if self.fallback_enclosure.is_none() {
            self.fallback_enclosure = Some(url.clone());
        }
        if content_type.is_some_and(|value| value.starts_with("audio/"))
            && self.audio_enclosure.is_none()
        {
            self.audio_enclosure = Some(url);
        }
    }

    fn finish(self) -> Option<ParsedEpisode> {
        let audio_url = self.audio_enclosure.or(self.fallback_enclosure)?;
        let title = self.title.filter(|value| !value.trim().is_empty())?;
        Some(ParsedEpisode {
            guid: self
                .guid
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| audio_url.clone()),
            title,
            audio_url,
            page_url: self.page_url,
            published_at: self.published_at,
            duration_secs: self.duration_secs,
        })
    }
}

pub fn parse_feed(xml: &str, limit: usize) -> Result<ParsedFeed, PodcastError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    reader.config_mut().check_end_names = true;

    let mut path = Vec::<String>::new();
    let mut title = None;
    let mut author = None;
    let mut image_url = None;
    let mut episode = None::<EpisodeBuilder>;
    let mut episodes = Vec::new();

    loop {
        match reader.read_event().map_err(parse_error)? {
            Event::Start(element) => {
                let name = local_name(element.name().as_ref()).to_owned();
                handle_element(&reader, &name, &element, &mut episode, &mut image_url)?;
                if matches!(name.as_str(), "item" | "entry") {
                    episode = Some(EpisodeBuilder::default());
                }
                path.push(name);
            }
            Event::Empty(element) => {
                let name = local_name(element.name().as_ref()).to_owned();
                handle_element(&reader, &name, &element, &mut episode, &mut image_url)?;
            }
            Event::Text(text) => {
                let value = text.decode().map_err(parse_error)?.trim().to_owned();
                if !value.is_empty() {
                    handle_text(
                        &path,
                        &value,
                        &mut title,
                        &mut author,
                        &mut episode,
                        &mut image_url,
                    );
                }
            }
            Event::CData(text) => {
                let value = text.decode().map_err(parse_error)?.trim().to_owned();
                if !value.is_empty() {
                    handle_text(
                        &path,
                        &value,
                        &mut title,
                        &mut author,
                        &mut episode,
                        &mut image_url,
                    );
                }
            }
            Event::End(element) => {
                let name = local_name(element.name().as_ref()).to_owned();
                if matches!(name.as_str(), "item" | "entry") {
                    if episodes.len() < limit {
                        if let Some(parsed) = episode.take().and_then(EpisodeBuilder::finish) {
                            episodes.push(parsed);
                        }
                    } else {
                        episode = None;
                    }
                }
                path.pop();
            }
            Event::Eof => {
                if !path.is_empty() {
                    return Err(PodcastError::Parse("unexpected end of feed".to_owned()));
                }
                break;
            }
            _ => {}
        }
    }

    let title = title
        .filter(|value: &String| !value.trim().is_empty())
        .ok_or_else(|| PodcastError::Parse("feed has no title".to_owned()))?;
    Ok(ParsedFeed {
        title,
        author,
        image_url,
        episodes,
    })
}

fn handle_element(
    reader: &Reader<&[u8]>,
    name: &str,
    element: &BytesStart<'_>,
    episode: &mut Option<EpisodeBuilder>,
    image_url: &mut Option<String>,
) -> Result<(), PodcastError> {
    let attributes = attributes(reader, element)?;
    match name {
        "enclosure" => {
            if let Some(builder) = episode {
                if let Some(url) = attribute(&attributes, "url") {
                    builder.enclosure(url.to_owned(), attribute(&attributes, "type"));
                }
            }
        }
        "link" => {
            if let (Some(builder), Some(href)) = (episode, attribute(&attributes, "href")) {
                let rel = attribute(&attributes, "rel");
                let content_type = attribute(&attributes, "type");
                if rel == Some("enclosure") {
                    builder.enclosure(href.to_owned(), content_type);
                } else if rel.is_none() || rel == Some("alternate") {
                    builder.page_url.get_or_insert_with(|| href.to_owned());
                }
            }
        }
        "image" if episode.is_none() => {
            if let Some(url) = attribute(&attributes, "href") {
                image_url.get_or_insert_with(|| url.to_owned());
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_text(
    path: &[String],
    value: &str,
    title: &mut Option<String>,
    author: &mut Option<String>,
    episode: &mut Option<EpisodeBuilder>,
    image_url: &mut Option<String>,
) {
    let current = path.last().map(String::as_str).unwrap_or_default();
    if let Some(builder) = episode {
        match current {
            "title" => {
                builder.title.get_or_insert_with(|| value.to_owned());
            }
            "guid" | "id" => {
                builder.guid.get_or_insert_with(|| value.to_owned());
            }
            "link" => {
                builder.page_url.get_or_insert_with(|| value.to_owned());
            }
            "pubDate" | "published" | "updated" => {
                if let Some(timestamp) = parse_published_at(value) {
                    builder.published_at.get_or_insert(timestamp);
                }
            }
            "duration" => {
                if let Some(duration) = parse_duration(value) {
                    builder.duration_secs.get_or_insert(duration);
                }
            }
            _ => {}
        };
        return;
    }

    match current {
        "title" if title.is_none() => *title = Some(value.to_owned()),
        "author" if author.is_none() => *author = Some(value.to_owned()),
        "name"
            if author.is_none()
                && path
                    .iter()
                    .rev()
                    .nth(1)
                    .is_some_and(|parent| parent == "author") =>
        {
            *author = Some(value.to_owned());
        }
        "url"
            if image_url.is_none()
                && path
                    .iter()
                    .rev()
                    .nth(1)
                    .is_some_and(|parent| parent == "image") =>
        {
            *image_url = Some(value.to_owned());
        }
        _ => {}
    }
}

#[must_use]
pub fn parse_duration(value: &str) -> Option<i64> {
    let parts = value
        .trim()
        .split(':')
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if parts.is_empty() || parts.len() > 3 || parts.iter().any(|part| *part < 0) {
        return None;
    }
    match parts.as_slice() {
        [seconds] => Some(*seconds),
        [minutes, seconds] if *seconds < 60 => minutes.checked_mul(60)?.checked_add(*seconds),
        [hours, minutes, seconds] if *minutes < 60 && *seconds < 60 => hours
            .checked_mul(3_600)?
            .checked_add(minutes.checked_mul(60)?)?
            .checked_add(*seconds),
        _ => None,
    }
}

fn parse_published_at(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc2822(value)
        .or_else(|_| DateTime::parse_from_rfc3339(value))
        .ok()
        .map(|date| date.timestamp())
}

fn local_name(name: &[u8]) -> &str {
    let local = name.rsplit(|byte| *byte == b':').next().unwrap_or(name);
    std::str::from_utf8(local).unwrap_or_default()
}

fn attributes(
    reader: &Reader<&[u8]>,
    element: &BytesStart<'_>,
) -> Result<Vec<(String, String)>, PodcastError> {
    element
        .attributes()
        .map(|attribute| {
            let attribute = attribute.map_err(parse_error)?;
            let key = local_name(attribute.key.as_ref()).to_owned();
            let value = attribute
                .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder())
                .map_err(parse_error)?
                .into_owned();
            Ok((key, value))
        })
        .collect()
}

fn attribute<'a>(attributes: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find_map(|(candidate, value)| (candidate == key).then_some(value.as_str()))
}

fn parse_error(error: impl std::fmt::Display) -> PodcastError {
    PodcastError::Parse(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_feed_keeps_audio_items_and_uses_enclosure_as_guid_fallback() {
        let parsed = parse_feed(
            r#"<?xml version="1.0"?>
            <rss xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd">
              <channel>
                <title>Systems Weekly</title><itunes:author>Ada</itunes:author>
                <itunes:image href="https://example.test/show.jpg"/>
                <item>
                  <title>Episode 2</title>
                  <enclosure url="https://example.test/two.mp3" type="audio/mpeg"/>
                  <pubDate>Wed, 22 Jul 2026 10:00:00 +0000</pubDate>
                  <itunes:duration>1:15:33</itunes:duration>
                </item>
                <item><title>Web only</title><link>https://example.test/post</link></item>
              </channel>
            </rss>"#,
            25,
        )
        .unwrap();

        assert_eq!(parsed.title, "Systems Weekly");
        assert_eq!(parsed.author.as_deref(), Some("Ada"));
        assert_eq!(
            parsed.image_url.as_deref(),
            Some("https://example.test/show.jpg")
        );
        assert_eq!(parsed.episodes.len(), 1);
        assert_eq!(parsed.episodes[0].guid, "https://example.test/two.mp3");
        assert_eq!(parsed.episodes[0].duration_secs, Some(4_533));
        assert!(parsed.episodes[0].published_at.is_some());
    }

    #[test]
    fn atom_feed_reads_namespaced_fields_and_honors_limit() {
        let parsed = parse_feed(
            r#"<feed xmlns="http://www.w3.org/2005/Atom">
              <title>Atom Show</title><author><name>Lin</name></author>
              <entry><id>one</id><title>One</title>
                <link rel="enclosure" type="audio/ogg" href="https://e.test/1.ogg"/>
                <published>2026-07-22T10:00:00Z</published>
              </entry>
              <entry><id>two</id><title>Two</title>
                <link rel="enclosure" href="https://e.test/2.mp3"/>
              </entry>
            </feed>"#,
            1,
        )
        .unwrap();
        assert_eq!(parsed.author.as_deref(), Some("Lin"));
        assert_eq!(parsed.episodes.len(), 1);
        assert_eq!(parsed.episodes[0].guid, "one");
    }

    #[test]
    fn duration_parser_accepts_podcast_conventions() {
        assert_eq!(parse_duration("4533"), Some(4_533));
        assert_eq!(parse_duration("75:33"), Some(4_533));
        assert_eq!(parse_duration("1:15:33"), Some(4_533));
        assert_eq!(parse_duration("1:99:00"), None);
        assert_eq!(parse_duration("garbage"), None);
    }

    #[test]
    fn malformed_xml_is_a_parse_error() {
        assert!(matches!(
            parse_feed("<rss><channel><title>Broken", 10),
            Err(PodcastError::Parse(_))
        ));
    }

    #[test]
    fn invalid_publication_date_is_kept_as_unknown() {
        let parsed = parse_feed(
            r#"<rss><channel><title>Show</title><item>
              <title>Undated</title>
              <guid>undated</guid>
              <pubDate>sometime soon</pubDate>
              <enclosure url="https://example.test/undated.mp3" type="audio/mpeg"/>
            </item></channel></rss>"#,
            10,
        )
        .unwrap();

        assert_eq!(parsed.episodes[0].published_at, None);
    }
}
