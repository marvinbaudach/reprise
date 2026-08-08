#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // MATCH-2 scores it; MATCH-3 constructs it for live scans.
pub(crate) struct AlbumQuery {
    pub(crate) album_artist: String,
    pub(crate) album: String,
    pub(crate) track_titles: Vec<String>,
    pub(crate) track_count: u32,
    pub(crate) year: Option<u32>,
}
