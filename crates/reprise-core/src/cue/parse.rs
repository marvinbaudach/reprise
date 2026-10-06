use std::collections::HashSet;

use super::text::decode;
use super::{CueError, CueFile, CueSheet, CueTrack, Frames, MAX_SHEET_BYTES, MAX_TRACKS};

#[derive(Default)]
struct TrackBuilder {
    number: u32,
    is_audio: bool,
    title: String,
    performer: String,
    index00: Option<Frames>,
    index01: Option<Frames>,
}

impl TrackBuilder {
    fn finish(self, album_performer: &str) -> Result<CueTrack, CueError> {
        let index01 = self
            .index01
            .ok_or(CueError::MissingIndex01 { track: self.number })?;
        Ok(CueTrack {
            number: self.number,
            is_audio: self.is_audio,
            title: self.title,
            performer: if self.performer.is_empty() {
                album_performer.to_owned()
            } else {
                self.performer
            },
            // A pregap that starts where the track does is no pregap at all.
            index00: self.index00.filter(|index00| *index00 != index01),
            index01,
        })
    }
}

#[derive(Default)]
struct FileBuilder {
    name: String,
    tracks: Vec<TrackBuilder>,
}

/// Parses the bytes of a CUE sheet, decoding UTF-8, UTF-16 or Windows-1252 text.
/// A sheet larger than [`MAX_SHEET_BYTES`] or with more than [`MAX_TRACKS`]
/// tracks is refused.
pub fn parse(bytes: &[u8]) -> Result<CueSheet, CueError> {
    if bytes.len() > MAX_SHEET_BYTES {
        return Err(CueError::TooLarge);
    }
    let text = decode(bytes).map_err(|()| CueError::InvalidTextEncoding)?;
    let mut sheet = CueSheet {
        title: String::new(),
        performer: String::new(),
        date: None,
        genre: None,
        files: Vec::new(),
    };
    let mut files: Vec<FileBuilder> = Vec::new();
    let mut track_numbers = HashSet::new();

    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    for (offset, raw_line) in text.lines().enumerate() {
        let line_number = offset + 1;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let (keyword, rest) = split_keyword(line);
        match keyword.to_ascii_uppercase().as_str() {
            "REM" => parse_rem(rest, &mut sheet),
            "FILE" => files.push(FileBuilder {
                name: parse_file_name(rest).ok_or_else(|| CueError::InvalidStatement {
                    line: line_number,
                    statement: line.to_owned(),
                })?,
                tracks: Vec::new(),
            }),
            "TRACK" => {
                let mut fields = rest.split_whitespace();
                let invalid = || CueError::InvalidStatement {
                    line: line_number,
                    statement: line.to_owned(),
                };
                let number = fields
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or_else(invalid)?;
                let is_audio = fields
                    .next()
                    .ok_or_else(invalid)?
                    .eq_ignore_ascii_case("AUDIO");
                if !track_numbers.insert(number) {
                    return Err(CueError::DuplicateTrackNumber { track: number });
                }
                if track_numbers.len() > MAX_TRACKS {
                    return Err(CueError::TooManyTracks { limit: MAX_TRACKS });
                }
                current_file(&mut files, line_number, line)?
                    .tracks
                    .push(TrackBuilder {
                        number,
                        is_audio,
                        ..TrackBuilder::default()
                    });
            }
            // Between a later FILE and its first TRACK there is no owner for these
            // fields; they must not overwrite the album's.
            "TITLE" => {
                let value = parse_value(rest);
                if let Some(track) = current_track(&mut files) {
                    track.title = value;
                } else if track_numbers.is_empty() {
                    sheet.title = value;
                }
            }
            "PERFORMER" => {
                let value = parse_value(rest);
                if let Some(track) = current_track(&mut files) {
                    track.performer = value;
                } else if track_numbers.is_empty() {
                    sheet.performer = value;
                }
            }
            "INDEX" => parse_index(
                current_track(&mut files).ok_or_else(|| CueError::InvalidStatement {
                    line: line_number,
                    statement: line.to_owned(),
                })?,
                rest,
                line_number,
                line,
            )?,
            _ => {}
        }
    }

    if track_numbers.is_empty() {
        return Err(CueError::EmptySheet);
    }

    sheet.files = files
        .into_iter()
        .map(|file| {
            Ok(CueFile {
                name: file.name,
                tracks: file
                    .tracks
                    .into_iter()
                    .map(|track| track.finish(&sheet.performer))
                    .collect::<Result<_, _>>()?,
            })
        })
        .collect::<Result<_, CueError>>()?;
    for file in &sheet.files {
        validate_indices(file)?;
    }
    Ok(sheet)
}

fn validate_indices(file: &CueFile) -> Result<(), CueError> {
    let mut previous = None;
    for track in &file.tracks {
        if previous.is_some_and(|position| track.index01 <= position)
            || track.index00.is_some_and(|position| {
                position >= track.index01 || previous.is_some_and(|previous| position < previous)
            })
        {
            return Err(CueError::NonMonotonicIndex {
                track: track.number,
            });
        }
        previous = Some(track.index01);
    }
    Ok(())
}

fn split_keyword(line: &str) -> (&str, &str) {
    line.find(char::is_whitespace)
        .map_or((line, ""), |at| (&line[..at], line[at..].trim()))
}

fn parse_rem(rest: &str, sheet: &mut CueSheet) {
    let (kind, value) = split_keyword(rest);
    match kind.to_ascii_uppercase().as_str() {
        "DATE" => sheet.date = Some(parse_value(value)),
        "GENRE" => sheet.genre = Some(parse_value(value)),
        _ => {}
    }
}

fn parse_file_name(rest: &str) -> Option<String> {
    // The type token never contains a quote, so the last quote closes the name.
    if let Some(quoted) = rest.strip_prefix('"') {
        return quoted.rfind('"').map(|end| quoted[..end].to_owned());
    }
    // Unquoted names may contain spaces; only the trailing type token is split off.
    let name = rest
        .rsplit_once(char::is_whitespace)
        .map_or(rest, |(name, _type)| name.trim_end());
    (!name.is_empty()).then(|| name.to_owned())
}

fn parse_value(value: &str) -> String {
    let Some(quoted) = value.strip_prefix('"') else {
        return value.to_owned();
    };
    // A value that ends in a quote keeps any quotes inside it; otherwise it
    // ends at the first closing quote and whatever follows is junk.
    if let Some(inner) = quoted.strip_suffix('"') {
        return inner.to_owned();
    }
    quoted
        .find('"')
        .map_or(quoted, |end| &quoted[..end])
        .to_owned()
}

fn current_file<'a>(
    files: &'a mut [FileBuilder],
    line: usize,
    statement: &str,
) -> Result<&'a mut FileBuilder, CueError> {
    files.last_mut().ok_or_else(|| CueError::InvalidStatement {
        line,
        statement: statement.to_owned(),
    })
}

fn current_track(files: &mut [FileBuilder]) -> Option<&mut TrackBuilder> {
    files.last_mut()?.tracks.last_mut()
}

fn parse_index(
    track: &mut TrackBuilder,
    rest: &str,
    line: usize,
    statement: &str,
) -> Result<(), CueError> {
    let invalid = || CueError::InvalidStatement {
        line,
        statement: statement.to_owned(),
    };
    let mut fields = rest.split_whitespace();
    let kind: u32 = fields
        .next()
        .and_then(|kind| kind.parse().ok())
        .ok_or_else(invalid)?;
    // Only INDEX 00 and 01 position a track; sub-indices are not interpreted.
    if kind > 1 {
        return Ok(());
    }
    let value = fields.next().unwrap_or_default();
    let frames = parse_frames(value).ok_or_else(|| CueError::InvalidIndex {
        track: track.number,
        value: value.to_owned(),
    })?;
    let slot = if kind == 0 {
        &mut track.index00
    } else {
        &mut track.index01
    };
    if slot.replace(frames).is_some() {
        return Err(CueError::DuplicateIndex {
            track: track.number,
            index: kind,
        });
    }
    Ok(())
}

fn parse_frames(value: &str) -> Option<Frames> {
    let mut fields = value.split(':');
    let minutes: u32 = fields.next()?.parse().ok()?;
    let seconds: u32 = fields.next()?.parse().ok()?;
    let frames: u32 = fields.next()?.parse().ok()?;
    if fields.next().is_some() || seconds >= 60 || frames >= 75 {
        return None;
    }
    minutes
        .checked_mul(60)?
        .checked_add(seconds)?
        .checked_mul(75)?
        .checked_add(frames)
        .map(Frames)
}
