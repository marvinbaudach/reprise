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
//! Die Crate ist nicht mehr leer: Spaltenmodell, Queue-Komposition,
//! Browse-Zustand, Playlists, Lyrics, Suchchips und die spektrale Färbung
//! wohnen bereits hier. Was hier fehlt, steht meist noch in
//! `crates/reprise-gnome/src/ui` und ist damit für Android unerreichbar —
//! toolkitfreie Präsentationslogik gehört hierher, bevor ein zweites Frontend
//! sie von Hand nachbaut.

pub mod analysis_progress;
pub mod browse;
pub mod colour;
pub mod column_widths;
pub mod columns;
pub mod device_sync;
pub mod filter_chip;
pub mod lyrics;
pub mod playlists;
pub mod queue;
pub mod search_chip;
pub mod search_scope;
pub mod spectral_colour;
pub mod strings;
pub mod waveform;
