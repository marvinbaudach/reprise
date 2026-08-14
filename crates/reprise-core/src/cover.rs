//! Cover-art resolution and thumbnailing (portable, GUI-free). These APIs read
//! covers from the user's files (embedded picture, or a sidecar image in the
//! album folder) and produce cached thumbnails below a platform-provided cache
//! root. The desktop convenience wrappers keep using the XDG cache dir. The
//! separate `cover_writeback` module publishes downloaded covers into album
//! folders with non-overwriting, best-effort safeguards.

use std::io::Read;
use std::path::{Path, PathBuf};

/// Where a cover for a track comes from.
#[derive(Debug)]
pub enum CoverSource {
    /// A picture embedded in the audio file (via lofty).
    Embedded(Vec<u8>),
    /// An image file sitting in the album folder (cover.*, folder.*).
    FolderImage(PathBuf),
}

/// Canonical sidecar cover file stems and extensions, in priority order.
pub(crate) const FOLDER_STEMS: &[&str] = &["cover", "folder", "front", "album"];
pub(crate) const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

/// All cover-relevant tag fields, read in ONE lofty pass (so `resolve_source`
/// and the download worker never open the file twice).
#[derive(Debug, Default)]
pub struct CoverTag {
    pub picture: Option<Vec<u8>>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub release_mbid: Option<String>,
}

pub fn read_cover_tag(track_path: &Path) -> CoverTag {
    read_cover_tag_with_source(&crate::library::source::UnixLibrarySource, track_path)
}

pub fn read_cover_tag_with_source(
    source: &dyn crate::library::source::LibrarySource,
    track_path: &Path,
) -> CoverTag {
    use lofty::prelude::*;
    let Some(file_type) = lofty::file::FileType::from_path(track_path) else {
        return CoverTag::default();
    };
    let Ok(reader) = source.open_read(track_path) else {
        return CoverTag::default();
    };
    let Ok(tagged) = lofty::probe::Probe::with_file_type(reader, file_type).read() else {
        return CoverTag::default();
    };
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return CoverTag::default();
    };
    CoverTag {
        picture: tag
            .pictures()
            .first()
            .map(|picture| picture.data().to_vec()),
        album_artist: tag
            .get_string(lofty::tag::ItemKey::AlbumArtist)
            .map(str::to_string),
        album: tag.album().map(|album| album.to_string()),
        release_mbid: tag
            .get_string(lofty::tag::ItemKey::MusicBrainzReleaseId)
            .map(str::to_string),
    }
}

/// Resolves the best available cover source for a track: an album-wide
/// downloaded cover first, then the existing local embedded-picture and
/// sidecar fallbacks. A shared downloaded source takes precedence so a
/// detected album mismatch can converge without modifying any audio file.
/// Pure read — it never writes to either the library or cache.
pub fn resolve_source(track_path: &Path) -> Option<CoverSource> {
    resolve_source_with_source(
        &crate::library::source::UnixLibrarySource,
        track_path,
        &default_cache_root(),
    )
}

pub fn resolve_source_with_source(
    source: &dyn crate::library::source::LibrarySource,
    track_path: &Path,
    cache_root: &Path,
) -> Option<CoverSource> {
    let tag = read_cover_tag_with_source(source, track_path);
    // Stage 1 (offline): a previously downloaded canonical cover for this
    // album takes precedence over track-local embedded artwork.
    if let (Some(album_artist), Some(album)) = (tag.album_artist.as_deref(), tag.album.as_deref()) {
        let key = crate::cover_download::album_key(album_artist, album);
        if let Some(path) = crate::cover_download::downloaded_cover_path_in(cache_root, &key) {
            return Some(CoverSource::FolderImage(path));
        }
    }
    if let Some(bytes) = tag.picture {
        return Some(CoverSource::Embedded(bytes));
    }
    track_path
        .parent()
        .and_then(|directory| folder_image_with_source(source, directory))
        .map(CoverSource::FolderImage)
}

pub(crate) fn folder_image_with_source(
    source: &dyn crate::library::source::LibrarySource,
    dir: &Path,
) -> Option<PathBuf> {
    let entries = source.read_directory(dir)?;
    for stem in FOLDER_STEMS {
        for ext in IMAGE_EXTS {
            for entry in &entries {
                let matches_stem = entry
                    .path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(stem));
                let matches_ext = entry
                    .path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(ext));
                if matches_stem && matches_ext {
                    return Some(entry.path.clone());
                }
            }
        }
    }
    None
}

use std::hash::{Hash, Hasher};

#[cfg(test)]
#[path = "cover_mobile_tests.rs"]
mod mobile_tests;

/// The cached edge lengths — one per consumer (list row / player bar / artist
/// portrait / album grid / Now-Playing view). Each maps to its own on-disk
/// cache file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailSize {
    Glow,
    List,
    Bar,
    Portrait,
    Grid,
    Full,
    /// A 56 dp Android list/mini-player slot at the measured 3x density.
    MobileList,
    /// A 210 dp Android artist portrait at the measured 3x density.
    MobilePortrait,
    /// A 364 dp Android Now Playing cover at the measured 3x density.
    MobileFull,
}

impl ThumbnailSize {
    pub fn pixels(self) -> u32 {
        match self {
            ThumbnailSize::Glow => 32,
            ThumbnailSize::List => 48,
            ThumbnailSize::Bar => 96,
            ThumbnailSize::Portrait => 192,
            ThumbnailSize::Grid => 256,
            ThumbnailSize::Full => 1024,
            ThumbnailSize::MobileList => 168,
            ThumbnailSize::MobilePortrait => 640,
            ThumbnailSize::MobileFull => 1092,
        }
    }
}

#[derive(Debug)]
pub enum CoverError {
    Decode(String),
    Io(String),
}

impl std::fmt::Display for CoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoverError::Decode(m) => write!(f, "cover decode failed: {m}"),
            CoverError::Io(m) => write!(f, "cover cache I/O failed: {m}"),
        }
    }
}

impl std::error::Error for CoverError {}

/// The cover thumbnail cache directory: `<XDG cache>/reprise/covers`. NEVER a
/// path inside the user's library — this is the load-bearing half of the
/// "we don't touch your files" promise.
pub fn cache_dir() -> PathBuf {
    cache_dir_with_root(&default_cache_root())
}

fn default_cache_root() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir)
}

/// The cover cache directory below a cache root supplied by the platform.
pub(crate) fn cache_dir_with_root(cache_root: &Path) -> PathBuf {
    cache_root.join("reprise/covers")
}

/// Returns the cache path to a thumbnail of `source` at `size`, creating it if
/// missing: hash the source bytes -> cache hit? -> else decode, resize (aspect
/// preserved, longest side = size), write PNG atomically (temp + rename).
pub fn thumbnail(source: &CoverSource, size: ThumbnailSize) -> Result<PathBuf, CoverError> {
    thumbnail_with_source(
        &crate::library::source::UnixLibrarySource,
        source,
        size,
        &default_cache_root(),
    )
}

pub fn thumbnail_with_source(
    library_source: &dyn crate::library::source::LibrarySource,
    source: &CoverSource,
    size: ThumbnailSize,
    cache_root: &Path,
) -> Result<PathBuf, CoverError> {
    let bytes = source_bytes(library_source, source)?;
    let key = hash_hex(&bytes);
    let dir = cache_dir_with_root(cache_root);
    let out = dir.join(format!("{key}-{}.png", size.pixels()));
    if out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(&dir).map_err(|e| CoverError::Io(e.to_string()))?;

    let decoded = image::load_from_memory(&bytes).map_err(|e| CoverError::Decode(e.to_string()))?;
    // Never upscale: clamp the target to the source's longest side. A source
    // smaller than the requested box is kept at native size (best available,
    // no blur) instead of being blown up by `image::thumbnail`, which does
    // upscale when the source is smaller than the target box.
    let longest = decoded.width().max(decoded.height());
    let target = size.pixels().min(longest);
    let thumb = decoded.thumbnail(target, target); // aspect-preserving

    // Atomic publish: write a UNIQUE temp file in the same dir, then rename.
    // Uniqueness matters because concurrent calls for the same cache key must
    // not race on one temp path (the loser would otherwise see a spurious
    // ENOENT on rename after the winner already unlinked it).
    let tmp = dir.join(format!(
        ".{key}-{}-{}.png.tmp",
        size.pixels(),
        fastrand::u64(..)
    ));
    if let Err(e) = thumb.save_with_format(&tmp, image::ImageFormat::Png) {
        let _ = std::fs::remove_file(&tmp);
        return Err(CoverError::Io(e.to_string()));
    }
    if let Err(e) = std::fs::rename(&tmp, &out) {
        let _ = std::fs::remove_file(&tmp);
        // A concurrent writer may have already published this exact key —
        // that's success, not an error.
        if out.exists() {
            return Ok(out);
        }
        return Err(CoverError::Io(e.to_string()));
    }
    Ok(out)
}

/// Returns a cached, already-blurred thumbnail. The source is first reduced
/// to `size`, then blurred exactly once and atomically published as a PNG.
/// Consumers can scale the tiny texture without scheduling a live blur node
/// on every rendered frame.
pub fn blurred_thumbnail(
    source: &CoverSource,
    size: ThumbnailSize,
    sigma: f32,
) -> Result<PathBuf, CoverError> {
    let thumbnail_path = thumbnail(source, size)?;
    blur_reduced_thumbnail(&thumbnail_path, sigma)
}

/// Blurs an already-reduced thumbnail exactly once and caches the result.
/// This is the companion for resolvers that must first materialize a specific
/// thumbnail size before reporting the resolved cover path.
pub fn blur_reduced_thumbnail(
    thumbnail_path: &std::path::Path,
    sigma: f32,
) -> Result<PathBuf, CoverError> {
    if !sigma.is_finite() || sigma <= 0.0 {
        return Err(CoverError::Decode(
            "blur sigma must be finite and positive".into(),
        ));
    }
    let stem = thumbnail_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| CoverError::Io("thumbnail cache path has no file stem".into()))?;
    let out = thumbnail_path.with_file_name(format!("{stem}-blur-{:08x}.png", sigma.to_bits()));
    if out.exists() {
        return Ok(out);
    }

    let decoded =
        image::open(thumbnail_path).map_err(|error| CoverError::Decode(error.to_string()))?;
    let blurred = decoded.blur(sigma);
    let dir = out
        .parent()
        .ok_or_else(|| CoverError::Io("blur cache path has no parent".into()))?;
    let tmp = dir.join(format!(".{stem}-blur-{}.png.tmp", fastrand::u64(..)));
    if let Err(error) = blurred.save_with_format(&tmp, image::ImageFormat::Png) {
        let _ = std::fs::remove_file(&tmp);
        return Err(CoverError::Io(error.to_string()));
    }
    if let Err(error) = std::fs::rename(&tmp, &out) {
        let _ = std::fs::remove_file(&tmp);
        if out.exists() {
            return Ok(out);
        }
        return Err(CoverError::Io(error.to_string()));
    }
    Ok(out)
}

fn source_bytes(
    library_source: &dyn crate::library::source::LibrarySource,
    source: &CoverSource,
) -> Result<Vec<u8>, CoverError> {
    match source {
        CoverSource::Embedded(b) => Ok(b.clone()),
        CoverSource::FolderImage(path) => {
            let mut reader = library_source
                .open_read(path)
                .map_err(|error| CoverError::Io(error.to_string()))?;
            let mut bytes = Vec::new();
            reader
                .read_to_end(&mut bytes)
                .map_err(|error| CoverError::Io(error.to_string()))?;
            Ok(bytes)
        }
    }
}

/// Fast, non-cryptographic content hash (std DefaultHasher) over the source
/// bytes, hex-encoded. The key only needs to be deterministic on one machine
/// and collision-resistant enough for a cache — no crypto property required,
/// so no new hashing dependency.
/// Where a track's last cover resolution is remembered.
///
/// One file per track and size, named from the track's path. It holds the
/// stamp the resolution was valid for and the thumbnail it produced — or
/// nothing, when the track has no cover at all, which is worth remembering
/// just as much.
fn resolution_index_path(track_path: &Path, size: ThumbnailSize) -> PathBuf {
    let key = hash_hex(track_path.as_os_str().as_encoded_bytes());
    cache_dir()
        .join("resolved")
        .join(format!("{key}-{}", size.pixels()))
}

fn mtime_nanos(path: &Path) -> u128 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_nanos())
}

/// Bumped whenever an index entry's meaning changes, so entries written by an
/// older format fall out as a stamp mismatch instead of being misread.
///
/// Version 2 introduced [`RESOLUTION_UNKNOWN`]. Before it, an entry the batch
/// had settled without a resolution behind it was indistinguishable from "this
/// track has no cover" — both are an empty answer line — so those entries have
/// to be discarded rather than believed.
const RESOLUTION_FORMAT: u32 = 2;

/// What has to stay the same for a remembered resolution to still be true.
///
/// Three things can change a track's cover without the track itself changing:
/// the file being rewritten, a sidecar image appearing in the album folder, or
/// a cover being downloaded for the album. Hence three stamps — each one a
/// `stat`, which is microseconds against the milliseconds a tag read costs.
///
/// The download side is stamped from a marker the publisher bumps, not from
/// the download directory's mtime: negative markers and temp files land there
/// too, and none of them change what a track resolves to.
fn resolution_stamp(track_path: &Path) -> Option<String> {
    let meta = std::fs::metadata(track_path).ok()?;
    let track = mtime_nanos(track_path);
    let folder = track_path.parent().map_or(0, mtime_nanos);
    let downloaded = mtime_nanos(&crate::cover_download::publish_marker());
    Some(format!(
        "{RESOLUTION_FORMAT}:{track}:{}:{folder}:{downloaded}",
        meta.len()
    ))
}

/// The thumbnail for a track, asking what was resolved last time before
/// reading anything.
///
/// The thumbnail cache is content-addressed: its key is a hash of the cover
/// bytes, so it cannot be asked until those bytes are in hand — and getting
/// them means reading the file's tags. That read is the expensive part, it
/// happens for every cover the app shows, and a warm cache never saved it: a
/// 1800-track library read 455 sets of tags on every single launch, cache warm
/// or cold. This index is asked first and answers from three `stat` calls.
///
/// `None` means the track has no cover — remembered too, so a coverless track
/// costs three stats rather than a full tag read on every launch.
pub fn thumbnail_for_track(track_path: &Path, size: ThumbnailSize) -> Option<PathBuf> {
    let stamp = resolution_stamp(track_path);
    let index = resolution_index_path(track_path, size);

    if let Some(stamp) = stamp.as_deref() {
        if let Some(remembered) = read_resolution(&index, stamp) {
            return remembered;
        }
    }

    let resolved = resolve_source(track_path).and_then(|source| thumbnail(&source, size).ok());
    if let Some(stamp) = stamp.as_deref() {
        write_resolution(&index, stamp, resolved.as_deref());
    }
    resolved
}

/// Marker in the index's third line for "the download side has nothing to do
/// here".
///
/// One marker for both of its answers — already covered, or nothing found —
/// because they lead to the same thing: do not ask again until something that
/// could change the answer has changed.
///
/// It lives on its own line because it is a different question from what the
/// cover resolved to. Writing it into the answer line cost an evening: the
/// batch marks a *covered* track settled, that overwrote the remembered
/// thumbnail, and the whole library rendered as placeholders.
const DOWNLOAD_EXHAUSTED: &str = "-";

/// Answer-line marker for "nothing has been resolved for this track yet", as
/// opposed to an empty line, which is the real answer "this track has no
/// cover".
///
/// The two have to be distinguishable because the batch settles the download
/// side for every track it walks, and on launch that happens before most rows
/// have ever been rendered — so it settles tracks whose cover has never been
/// looked at. Without this marker that wrote an empty answer, and an empty
/// answer is final: the stamp stays valid, nothing opens the file again, and
/// the row keeps its placeholder however many covers the file actually holds.
const RESOLUTION_UNKNOWN: &str = "?";

/// The three lines an index entry holds: the stamp it is valid for, the
/// thumbnail the cover resolved to (empty for "no cover"), and whether the
/// download side is settled.
struct IndexEntry {
    stamp: String,
    thumbnail: String,
    download_settled: bool,
}

fn read_entry(index: &Path) -> Option<IndexEntry> {
    let contents = std::fs::read_to_string(index).ok()?;
    let mut lines = contents.split('\n');
    Some(IndexEntry {
        stamp: lines.next()?.to_owned(),
        thumbnail: lines.next().unwrap_or_default().to_owned(),
        download_settled: lines.next().unwrap_or_default() == DOWNLOAD_EXHAUSTED,
    })
}

/// Whether the download side has already been asked about this track and had
/// nothing to do, while nothing relevant has changed since.
///
/// Without this, every launch asks the download worker about every track in
/// the library — the cover batch does exactly that, unconditionally — and the
/// worker reads each file's tags to work out which album to ask about, only to
/// arrive at the same answer as last time.
pub fn download_marked_unavailable(track_path: &Path, size: ThumbnailSize) -> bool {
    let Some(stamp) = resolution_stamp(track_path) else {
        return false;
    };
    read_entry(&resolution_index_path(track_path, size))
        .is_some_and(|entry| entry.stamp == stamp && entry.download_settled)
}

/// Remember that the download side had nothing to do for this track, keeping
/// whatever the cover itself resolved to.
///
/// With no resolution on file the answer stays `RESOLUTION_UNKNOWN`: this
/// call knows only what the download side found, and says nothing about what
/// the track's own tags or album folder hold.
pub fn remember_download_unavailable(track_path: &Path, size: ThumbnailSize) {
    let Some(stamp) = resolution_stamp(track_path) else {
        return;
    };
    let index = resolution_index_path(track_path, size);
    let thumbnail = read_entry(&index)
        .filter(|entry| entry.stamp == stamp)
        .map_or_else(|| RESOLUTION_UNKNOWN.to_owned(), |entry| entry.thumbnail);
    write_entry(&index, &stamp, &thumbnail, true);
}

/// `Some(entry)` when the index still applies, `None` when it must be redone.
/// The inner `Option` is the answer itself: `None` for "this track has no
/// cover", which is a real answer and not a miss.
fn read_resolution(index: &Path, stamp: &str) -> Option<Option<PathBuf>> {
    let entry = read_entry(index)?;
    if entry.stamp != stamp {
        return None;
    }
    if entry.thumbnail == RESOLUTION_UNKNOWN {
        return None;
    }
    if entry.thumbnail.is_empty() {
        return Some(None);
    }
    let path = PathBuf::from(entry.thumbnail);
    // The thumbnail cache can be cleared independently of this index; a
    // remembered path that no longer exists is a miss, not an answer.
    path.exists().then_some(Some(path))
}

fn write_resolution(index: &Path, stamp: &str, thumbnail: Option<&Path>) {
    let answer = thumbnail
        .map(|path| path.to_string_lossy())
        .unwrap_or_default();
    // A fresh resolution says nothing about the download side; keep what is
    // known about it rather than making the batch ask all over again.
    let settled =
        read_entry(index).is_some_and(|entry| entry.stamp == stamp && entry.download_settled);
    write_entry(index, stamp, &answer, settled);
}

fn write_entry(index: &Path, stamp: &str, thumbnail: &str, download_settled: bool) {
    let answer = format!(
        "{thumbnail}\n{}",
        if download_settled {
            DOWNLOAD_EXHAUSTED
        } else {
            ""
        }
    );
    write_resolution_body(index, stamp, &answer);
}

fn write_resolution_body(index: &Path, stamp: &str, answer: &str) {
    let Some(dir) = index.parent() else {
        return;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let body = format!("{stamp}\n{answer}");
    // Same atomic publish as the thumbnails: a torn index file would be read
    // back as a stamp mismatch at best and a wrong path at worst.
    let tmp = dir.join(format!(
        ".{}-{}.tmp",
        index
            .file_name()
            .map_or("index", |name| name.to_str().unwrap_or("index")),
        fastrand::u64(..)
    ));
    if std::fs::write(&tmp, body).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    if std::fs::rename(&tmp, index).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

pub(crate) fn hash_hex(bytes: &[u8]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

#[cfg(test)]
#[path = "cover_tests.rs"]
mod tests;
