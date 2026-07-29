//! Bounded extended playlist listing.

use std::ffi::OsString;

use super::{parse_playlist, PodcastError, YtDlp, YtDlpPlaylist};

impl YtDlp {
    pub fn list_range(&self, url: &str, end: usize) -> Result<YtDlpPlaylist, PodcastError> {
        let range = format!("1:{end}");
        let output = self.run(
            [
                OsString::from("--no-warnings"),
                OsString::from("--flat-playlist"),
                OsString::from("-I"),
                OsString::from(range),
                OsString::from("-J"),
                OsString::from(url),
            ],
            self.timeouts.list,
        )?;
        parse_playlist(&output)
    }
}
