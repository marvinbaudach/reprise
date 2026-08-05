//! Seek-bar colouring: the one-off colour-scale legend, its context-menu entry,
//! and the Appearance preference that chooses between the two colourings.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

// The legend under the bar. Two words and a caption: a colour scale nobody
// explains is a decorative strip, so it gets explained — exactly once.
pub const SEEK_LEGEND_LOW: &str = N_!("low");
pub const SEEK_LEGEND_HIGH: &str = N_!("high");
pub const SEEK_LEGEND_CAPTION: &str = N_!("Frequency centroid");

/// The way back to the legend once it has stopped appearing on its own. A
/// one-off hint that can never be called up again is a trap for everyone who
/// missed it the first time.
pub const EXPLAIN_COLOR_SCALE: &str = N_!("Explain the Color Scale");

// Appearance → Seek Bar → Coloring.
pub const SEEK_BAR: &str = N_!("Seek Bar");
pub const SEEK_COLORING: &str = N_!("Coloring");
pub const SEEK_COLORING_SUBTITLE: &str =
    N_!("Color the bar by its frequency centroid, or in a single color");
pub const SEEK_COLORING_FREQUENCY: &str = N_!("Frequency");
/// Deliberately not "Off": the quiet variant is a second coloring and a
/// legitimate taste, so it is named for what it does.
pub const SEEK_COLORING_SOLID: &str = N_!("Single Color");
