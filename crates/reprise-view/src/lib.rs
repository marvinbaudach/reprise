//! Die toolkit-freie Präsentationsschicht von Reprise.
//!
//! Diese Crate hält alles, was zwischen Kern und Oberfläche steht und kein
//! Toolkit braucht: ViewModels, Formatierung, Filterung, Sortierung,
//! Zustandsmaschinen, Navigationshistorie und übersetzbare Texte. Die
//! GTK-, Compose- und Web-Oberflächen konsumieren dieselben Werte, damit
//! diese Logik genau einmal existiert.
//!
//! Verbindliche Grenze: hier darf niemals `gtk4`, `libadwaita`, `glib`,
//! `gstreamer` oder `zbus` hineinlinken. `scripts/check-architecture.sh`
//! erzwingt das mechanisch.
//!
//! Die Crate ist beim Anlegen leer. `docs/superpowers/specs/
//! 2026-08-01-multi-surface-frontends-design.md` §4 (P1a) beschreibt, was
//! zuerst hier einzieht.

pub mod browse;
pub mod column_widths;
pub mod columns;
pub mod lyrics;
pub mod playlists;
pub mod queue;
pub mod strings;
pub mod waveform;
