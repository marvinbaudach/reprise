//! Bounded extended playlist listing.

use std::ffi::OsString;

use super::{parse_playlist, PodcastError, YtDlp, YtDlpPlaylist};

impl YtDlp {
    pub fn list_range(&self, url: &str, end: usize) -> Result<YtDlpPlaylist, PodcastError> {
        let range = format!("1:{end}");
        let mut arguments = vec![
            OsString::from("--no-warnings"),
            OsString::from("--flat-playlist"),
            // This is the bounded form of the same channel listing as
            // `YtDlp::list`, so loaded-more episodes need dates too.
            OsString::from("--extractor-args"),
            OsString::from("youtubetab:approximate_date"),
        ];
        self.append_metadata_language(&mut arguments);
        arguments.extend([
            OsString::from("-I"),
            OsString::from(range),
            OsString::from("-J"),
            OsString::from(url),
        ]);
        let output = self.run("list_range", arguments, self.timeouts.list)?;
        parse_playlist("list_range", &output)
    }
}
