//! The one place that opens library content for a `lofty` parser.
//!
//! Four call sites read tags out of a library file — the scanner's two passes,
//! the tag editor, the guarded tag reader and Library Doctor's remote metadata
//! pass. Each of them used to call `lofty::read_from_path` or
//! `lofty::probe::Probe::open`, both of which call `std::fs::File::open`
//! internally. That is precisely why no measurement over `fs::` ever saw them,
//! and why a SAF-backed scan would have failed on every single track while
//! traversal, presence and classification all went correctly through
//! [`LibrarySource`].
//!
//! They now all come here, so the reasoning below lives once instead of four
//! times.

use std::path::Path;

use crate::library::source::{LibraryReadHandle, LibrarySource};

/// Reproduces `Probe::open(path)` after the source has opened the content.
///
/// Lofty seeds a path-backed probe from the **extension** and does not sniff
/// the content; `Probe::new(reader)` seeds nothing, because it has no path.
/// So the extension is applied here — and an unknown extension deliberately
/// leaves the probe unseeded, so `Probe::read` still answers `UnknownFormat`
/// exactly as `read_from_path` did.
///
/// **That last branch is the whole reason this is a `match` and not a `?`.**
/// `FileType::from_path` returns an `Option`, and bailing out on `None` would
/// produce a different typed error than lofty's own — which
/// `import_errors::classify_lofty` turns into a stored `ImportErrorKind` and
/// the user sees. A file with an unrecognised extension must keep being
/// reported as `UnsupportedFormat`, not as an I/O failure.
///
/// The counter-example lives sixteen lines away in
/// `scanner_meta::read_meta_content_based`, which must **not** use this: it
/// reads `scanner_repair`'s temp file, deliberately given a non-audio
/// extension so the walk ignores it, so its parser has to be chosen by content.
pub(crate) fn open_probe(
    source: &dyn LibrarySource,
    path: &Path,
) -> lofty::error::Result<lofty::probe::Probe<LibraryReadHandle>> {
    let probe = lofty::probe::Probe::new(source.open_read(path)?);
    Ok(match lofty::file::FileType::from_path(path) {
        Some(file_type) => probe.set_file_type(file_type),
        None => probe,
    })
}
