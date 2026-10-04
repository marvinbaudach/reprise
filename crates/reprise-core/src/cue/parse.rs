use std::collections::HashSet;

use super::text::decode;
use super::{CueError, CueFile, CueSheet, CueTrack, Frames};

#[derive(Default)]
struct TrackBuilder {
    number: u32,
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
            title: self.title,
            performer: if self.performer.is_empty() {
                album_performer.to_owned()
            } else {
                self.performer
            },
            index00: self.index00,
            index01,
        })
    }
}

#[derive(Default)]
struct FileBuilder {
    name: String,
    tracks: Vec<TrackBuilder>,
}

pub fn parse(bytes: &[u8]) -> Result<CueSheet, CueError> {
    let text = decode(bytes);
    let mut sheet = CueSheet {
        title: String::new(),
        performer: String::new(),
        date: None,
        genre: None,
        files: Vec::new(),
    };
    let mut files: Vec<FileBuilder> = Vec::new();
    let mut track_numbers = HashSet::new();

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
                let number = rest
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse().ok())
                    .ok_or_else(|| CueError::InvalidStatement {
                        line: line_number,
                        statement: line.to_owned(),
                    })?;
                if !track_numbers.insert(number) {
                    return Err(CueError::DuplicateTrackNumber { track: number });
                }
                current_file(&mut files, line_number, line)?
                    .tracks
                    .push(TrackBuilder {
                        number,
                        ..TrackBuilder::default()
                    });
            }
            "TITLE" => {
                let value = parse_value(rest);
                if let Some(track) = current_track(&mut files) {
                    track.title = value;
                } else {
                    sheet.title = value;
                }
            }
            "PERFORMER" => {
                let value = parse_value(rest);
                if let Some(track) = current_track(&mut files) {
                    track.performer = value;
                } else {
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
                position > track.index01 || previous.is_some_and(|previous| position < previous)
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
    if let Some(quoted) = rest.strip_prefix('"') {
        return quoted.find('"').map(|end| quoted[..end].to_owned());
    }
    rest.split_whitespace().next().map(str::to_owned)
}

fn parse_value(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
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
    let mut fields = rest.split_whitespace();
    let kind = fields.next();
    let value = fields.next().unwrap_or_default();
    let frames = parse_frames(value).ok_or_else(|| CueError::InvalidIndex {
        track: track.number,
        value: value.to_owned(),
    })?;
    match kind {
        Some("00") => track.index00 = Some(frames),
        Some("01") => track.index01 = Some(frames),
        Some(_) => {}
        None => {
            return Err(CueError::InvalidStatement {
                line,
                statement: statement.to_owned(),
            });
        }
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
