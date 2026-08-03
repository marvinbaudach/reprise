/// Whether a library source has a notion of an importable Rhythmbox collection.
///
/// This capability is distinct from the presence of any concrete XML file.
/// A path-backed desktop source supports the concept and answers concrete
/// presence through `LibrarySource::probe`. A DocumentsProvider tree does not
/// contain another desktop application's private data and reports
/// `Unsupported` even while its own library documents remain reachable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RhythmboxImportCapability {
    Supported,
    Unsupported,
}
