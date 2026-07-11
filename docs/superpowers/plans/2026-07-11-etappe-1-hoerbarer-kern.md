# Reprise — Etappe 1: Hörbarer Kern — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine startbare Tauri-2-App „Reprise", die einen Musikordner scannt (Tags via lofty → SQLite), die Titel in einer sortierbaren Spaltenliste zeigt und per Doppelklick über GStreamer abspielt — mit Playerleiste (Play/Pause, Seekbar, Lautstärke, Zeitanzeige) und Statusleiste.

**Architecture:** Rust-Backend (Module `db`, `library`, `player`, `ipc`) hinter Tauri-Commands/-Events; React/TypeScript-Frontend fordert Track-Fenster per SQL-Windowing an und hält nie die ganze Bibliothek im Speicher. GStreamer `playbin3` spielt; Positions-Ticks kommen als Tauri-Events.

**Tech Stack:** Tauri 2, Rust (rusqlite bundled, lofty, walkdir, gstreamer-rs, thiserror), React 19 + TypeScript + Vite, Zustand, Vitest + React Testing Library.

**Spec:** `docs/superpowers/specs/2026-07-11-reprise-design.md` (freigegeben 2026-07-11)

## Global Constraints

- Projektpfad: `/home/marvin/Projects/reprise` (Git-Repo existiert, Branch `main`)
- Lizenz: GPL-3.0 (LICENSE-Datei in Task 1)
- UI-Sprache Deutsch; alle UI-Strings zentral in `src/lib/strings.ts` — keine Literale in Komponenten
- Reprise verändert Musikdateien NIEMALS ungefragt (Etappe 1 schreibt ausschließlich lesend, außer im Test-Roundtrip auf Fixture-Kopien)
- Immutabilität im Frontend: neue Objekte statt Mutation (Zustand-Store: `set({...})`)
- Dateien < 800 Zeilen, Funktionen klein, frühe Returns
- Fehler nie verschlucken: Rust `thiserror` + `Result`, Frontend `console.error` + (ab Etappe 4) Toast
- Commit-Format: `<type>: <beschreibung>` (feat, fix, refactor, docs, test, chore), keine Attribution
- DB-Pfad zur Laufzeit: `~/.local/share/reprise/reprise.db`; Tests nutzen In-Memory-DB
- Rust edition 2021; TypeScript strict
- Systemvoraussetzungen (Manjaro): `webkit2gtk-4.1`, `gstreamer`, `gst-plugins-base`, `gst-plugins-good`, `gst-plugins-bad`, `base-devel`, `nodejs`/`npm` — Task 1 prüft sie
- Sicherheit (Spec-Abschnitt „Sicherheit"): strikte CSP ohne Remote-Quellen (Task 1), Capabilities minimal halten (nur `core:default` + explizit hinzugefügte Plugins), SQL nur parametrisiert, Sortierfelder per Whitelist (Task 4), Limits gedeckelt, keine Telemetrie

---

### Task 1: Scaffold — Tauri 2 + React + TypeScript + Vite

**Files:**
- Create: kompletter Scaffold unter `/home/marvin/Projects/reprise/` (`src-tauri/`, `src/`, `package.json`, `vite.config.ts`, …)
- Create: `LICENSE` (GPL-3.0), `.gitignore`
- Modify: `src-tauri/tauri.conf.json` (productName, identifier, Fenstergröße)

**Interfaces:**
- Produces: lauffähiges `npm run tauri dev`; Verzeichnislayout, auf dem alle weiteren Tasks aufbauen

- [ ] **Step 1: Systemvoraussetzungen prüfen**

Run:
```bash
pkg-config --modversion webkit2gtk-4.1 gstreamer-1.0 && rustc --version && node --version
```
Expected: drei Versionsnummern, keine Fehler. Falls etwas fehlt:
```bash
sudo pacman -S --needed webkit2gtk-4.1 gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad base-devel rust nodejs npm
```

- [ ] **Step 2: Scaffold erzeugen**

Der Ordner enthält bereits `docs/`, `Musikplayer.pdf` und `.git/` — deshalb Scaffold in Temp-Ordner erzeugen und hineinkopieren:

```bash
cd /tmp && npm create tauri-app@latest reprise-scaffold -- --template react-ts --manager npm --yes
cp -rn /tmp/reprise-scaffold/. /home/marvin/Projects/reprise/
cd /home/marvin/Projects/reprise && npm install
```

- [ ] **Step 3: App-Identität + Sicherheits-Baseline setzen**

In `src-tauri/tauri.conf.json` diese Felder setzen (Rest bleibt wie generiert). Die CSP ist die Sicherheits-Baseline aus der Spec — kein Remote-Content, `asset:` nur für spätere Cover; die Wayland-App-ID ergibt sich aus `identifier` und muss exakt `org.reprise.Reprise` sein (GNOME-Dock-Gruppierung, Spec „GNOME-Integration"):

```json
{
  "productName": "Reprise",
  "identifier": "org.reprise.Reprise",
  "app": {
    "security": {
      "csp": "default-src 'self'; img-src 'self' asset: data:; style-src 'self' 'unsafe-inline'; media-src 'self' asset:"
    },
    "windows": [
      {
        "title": "Reprise",
        "width": 1280,
        "height": 800,
        "minWidth": 900,
        "minHeight": 600
      }
    ]
  }
}
```

`src-tauri/capabilities/default.json` prüfen: Es darf nur `core:default` enthalten (Plugins kommen einzeln dazu, z. B. `dialog:default` in Task 9 — nie pauschal erweitern).

In `src-tauri/Cargo.toml`: `name = "reprise"`, `default-run = "reprise"` (falls generiert anders). In `package.json`: `"name": "reprise"`.

- [ ] **Step 4: GPL-3.0-Lizenz ablegen**

```bash
curl -sL https://www.gnu.org/licenses/gpl-3.0.txt -o LICENSE
```
In `src-tauri/Cargo.toml` unter `[package]`: `license = "GPL-3.0-or-later"`.

- [ ] **Step 5: Dev-Build verifizieren**

Run: `npm run tauri dev`
Expected: Fenster „Reprise" öffnet sich mit dem Vite-Template-Inhalt. Fenster schließen (Prozess beendet sich).

- [ ] **Step 6: .gitignore ergänzen und committen**

`.gitignore` muss enthalten: `node_modules/`, `dist/`, `src-tauri/target/`, `*.local`.

```bash
git add -A && git commit -m "chore: Tauri-2-Scaffold (React+TS+Vite), GPL-3.0, App-Identität Reprise"
```

---

### Task 2: Rust — Datenbankmodul mit Migrationen

**Files:**
- Create: `src-tauri/src/db.rs`
- Create: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/lib.rs` (Module deklarieren)
- Modify: `src-tauri/Cargo.toml` (Dependencies)

**Interfaces:**
- Produces:
  - `db::open(path: Option<&Path>) -> Result<Connection, DbError>` — `None` = In-Memory (Tests)
  - `db::migrate(conn: &Connection) -> Result<(), DbError>` — idempotent via `PRAGMA user_version`
  - `models::Track { id: i64, path: String, title: String, artist: String, album: String, album_artist: String, year: Option<i32>, track_no: Option<i32>, genre: String, duration_ms: i64, bitrate_kbps: Option<i32>, rating: i32, play_count: i64, last_played_at: Option<i64>, added_at: i64, file_mtime: i64, missing: bool }` (serde::Serialize)

- [ ] **Step 1: Dependencies eintragen**

In `src-tauri/Cargo.toml` unter `[dependencies]` ergänzen:

```toml
rusqlite = { version = "0.32", features = ["bundled"] }
thiserror = "2"
```

(`serde`/`serde_json` sind vom Scaffold bereits da.)

- [ ] **Step 2: Failing Test schreiben**

`src-tauri/src/db.rs` anlegen — zunächst nur Test und leere Signaturen:

```rust
use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Datenbankfehler: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO-Fehler: {0}")]
    Io(#[from] std::io::Error),
}

pub fn open(path: Option<&Path>) -> Result<Connection, DbError> {
    todo!()
}

pub fn migrate(conn: &Connection) -> Result<(), DbError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_tracks_table_and_is_idempotent() {
        let conn = open(None).unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap(); // zweiter Lauf darf nicht knallen
        let n: i64 = conn
            .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 1);
    }
}
```

In `src-tauri/src/lib.rs` oben ergänzen: `pub mod db;` und `pub mod models;` (models.rs zunächst leer anlegen: `// Track-Modelle folgen in Step 5`).

- [ ] **Step 3: Test laufen lassen — muss fehlschlagen**

Run: `cd src-tauri && cargo test migrate_creates`
Expected: FAIL/Panic wegen `todo!()`.

- [ ] **Step 4: Implementieren**

`todo!()`-Rümpfe in `db.rs` ersetzen:

```rust
pub fn open(path: Option<&Path>) -> Result<Connection, DbError> {
    let conn = match path {
        Some(p) => {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir)?;
            }
            Connection::open(p)?
        }
        None => Connection::open_in_memory()?,
    };
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(conn)
}

const SCHEMA_V1: &str = r#"
CREATE TABLE tracks (
  id            INTEGER PRIMARY KEY,
  path          TEXT NOT NULL UNIQUE,
  title         TEXT NOT NULL DEFAULT '',
  artist        TEXT NOT NULL DEFAULT '',
  album         TEXT NOT NULL DEFAULT '',
  album_artist  TEXT NOT NULL DEFAULT '',
  year          INTEGER,
  track_no      INTEGER,
  genre         TEXT NOT NULL DEFAULT '',
  duration_ms   INTEGER NOT NULL DEFAULT 0,
  bitrate_kbps  INTEGER,
  rating        INTEGER NOT NULL DEFAULT 0,
  play_count    INTEGER NOT NULL DEFAULT 0,
  last_played_at INTEGER,
  added_at      INTEGER NOT NULL,
  file_mtime    INTEGER NOT NULL DEFAULT 0,
  missing       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_tracks_artist ON tracks(artist);
CREATE INDEX idx_tracks_album  ON tracks(album);
CREATE TABLE import_errors (
  id          INTEGER PRIMARY KEY,
  path        TEXT NOT NULL,
  reason      TEXT NOT NULL,
  occurred_at INTEGER NOT NULL
);
"#;

pub fn migrate(conn: &Connection) -> Result<(), DbError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(SCHEMA_V1)?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    Ok(())
}
```

- [ ] **Step 5: Track-Modell in `models.rs`**

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<i32>,
    pub track_no: Option<i32>,
    pub genre: String,
    pub duration_ms: i64,
    pub bitrate_kbps: Option<i32>,
    pub rating: i32,
    pub play_count: i64,
    pub last_played_at: Option<i64>,
    pub added_at: i64,
    pub file_mtime: i64,
    pub missing: bool,
}
```

- [ ] **Step 6: Tests laufen lassen — müssen bestehen**

Run: `cd src-tauri && cargo test`
Expected: PASS (1 Test).

- [ ] **Step 7: Commit**

```bash
git add src-tauri && git commit -m "feat: SQLite-Schema v1 (tracks, import_errors) mit idempotenter Migration"
```

---

### Task 3: Rust — Scanner (walkdir + lofty) mit Fixture-Roundtrip-Test

**Files:**
- Create: `src-tauri/src/library/mod.rs`, `src-tauri/src/library/scanner.rs`
- Create: `src-tauri/tests/fixtures/sine.flac` (generiert, ~10 KB, wird committet)
- Modify: `src-tauri/src/lib.rs` (`pub mod library;`)
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: `db::open`, `db::migrate` (Task 2)
- Produces:
  - `library::scanner::scan_folder(conn: &mut Connection, root: &Path) -> Result<ScanReport, ScanError>`
  - `ScanReport { added: u32, updated: u32, skipped_unchanged: u32, errors: u32 }` (serde::Serialize)
  - `library::scanner::read_meta(path: &Path) -> Result<TrackMeta, ScanError>` — `TrackMeta { title, artist, album, album_artist, year, track_no, genre, duration_ms, bitrate_kbps }` (Strings leer statt None)
  - Upsert-Regel: Konflikt auf `path` → UPDATE der Tag-Felder; `rating`/`play_count`/`added_at` bleiben unangetastet

- [ ] **Step 1: Dependencies + Fixture**

`src-tauri/Cargo.toml`:

```toml
walkdir = "2"
lofty = "0.22"
```

Fixture erzeugen (GStreamer ist Systemvoraussetzung aus Task 1):

```bash
mkdir -p src-tauri/tests/fixtures
gst-launch-1.0 audiotestsrc num-buffers=50 ! audioconvert ! flacenc ! filesink location=src-tauri/tests/fixtures/sine.flac
```

Expected: Datei existiert, `ls -la src-tauri/tests/fixtures/sine.flac` zeigt > 5 KB.

- [ ] **Step 2: Failing Tests schreiben**

`src-tauri/src/library/mod.rs`: `pub mod scanner;`

`src-tauri/src/library/scanner.rs`:

```rust
use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("Datenbankfehler: {0}")]
    Db(#[from] crate::db::DbError),
    #[error("Sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Tags unlesbar: {0}")]
    Tags(String),
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")] // Frontend erwartet skippedUnchanged
pub struct ScanReport {
    pub added: u32,
    pub updated: u32,
    pub skipped_unchanged: u32,
    pub errors: u32,
}

#[derive(Debug, Default)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<i32>,
    pub track_no: Option<i32>,
    pub genre: String,
    pub duration_ms: i64,
    pub bitrate_kbps: Option<i32>,
}

const AUDIO_EXTENSIONS: [&str; 7] = ["mp3", "flac", "ogg", "opus", "m4a", "aac", "wav"];

pub fn read_meta(path: &Path) -> Result<TrackMeta, ScanError> {
    todo!()
}

pub fn scan_folder(conn: &mut Connection, root: &Path) -> Result<ScanReport, ScanError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};

    fn fixture_copy(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sine.flac");
        let dst = dir.join(name);
        std::fs::copy(&src, &dst).unwrap();
        dst
    }

    /// Schreibt Tags auf eine Fixture-KOPIE (niemals aufs Original) und liest
    /// sie mit read_meta zurück — Roundtrip aus der Spec.
    #[test]
    fn read_meta_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let file = fixture_copy(tmp.path(), "tagged.flac");
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title("Beast of Darkness".into());
        tag.set_artist("Brand of Sacrifice".into());
        tag.set_album("God Hand".into());
        tag.set_year(2019);
        tag.set_track(9);
        tag.set_genre("Deathcore".into());
        tag.save_to_path(&file, lofty::config::WriteOptions::default()).unwrap();

        let meta = read_meta(&file).unwrap();
        assert_eq!(meta.title, "Beast of Darkness");
        assert_eq!(meta.artist, "Brand of Sacrifice");
        assert_eq!(meta.album, "God Hand");
        assert_eq!(meta.year, Some(2019));
        assert_eq!(meta.track_no, Some(9));
        assert!(meta.duration_ms > 0);
    }

    #[test]
    fn scan_adds_updates_and_reports_errors() {
        let tmp = tempfile::tempdir().unwrap();
        fixture_copy(tmp.path(), "a.flac");
        fixture_copy(tmp.path(), "b.flac");
        // Defekte "Audio"-Datei → import_errors
        std::fs::write(tmp.path().join("kaputt.mp3"), b"kein audio").unwrap();
        // Nicht-Audio wird ignoriert
        std::fs::write(tmp.path().join("cover.jpg"), b"jpg").unwrap();

        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();

        let r1 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!((r1.added, r1.errors), (2, 1));

        // Zweiter Scan: nichts geändert → alles übersprungen
        let r2 = scan_folder(&mut conn, tmp.path()).unwrap();
        assert_eq!(r2.skipped_unchanged, 2);
        assert_eq!(r2.added, 0);

        let errs: i64 = conn
            .query_row("SELECT count(*) FROM import_errors", [], |r| r.get(0))
            .unwrap();
        assert_eq!(errs, 1);
    }
}
```

Dazu in `Cargo.toml` unter `[dev-dependencies]`: `tempfile = "3"`.

- [ ] **Step 3: Tests laufen lassen — müssen fehlschlagen**

Run: `cd src-tauri && cargo test scanner`
Expected: FAIL/Panic wegen `todo!()`.

- [ ] **Step 4: Implementieren**

`todo!()`-Rümpfe ersetzen:

```rust
pub fn read_meta(path: &Path) -> Result<TrackMeta, ScanError> {
    use lofty::prelude::*;
    let tagged = lofty::read_from_path(path)
        .map_err(|e| ScanError::Tags(e.to_string()))?;
    let props = tagged.properties();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    let get = |f: &dyn Fn(&lofty::tag::Tag) -> Option<String>| {
        tag.and_then(|t| f(t)).unwrap_or_default()
    };
    Ok(TrackMeta {
        title: get(&|t| t.title().map(|s| s.to_string())),
        artist: get(&|t| t.artist().map(|s| s.to_string())),
        album: get(&|t| t.album().map(|s| s.to_string())),
        album_artist: get(&|t| {
            t.get_string(&lofty::tag::ItemKey::AlbumArtist).map(|s| s.to_string())
        }),
        year: tag.and_then(|t| t.year()).map(|y| y as i32),
        track_no: tag.and_then(|t| t.track()).map(|n| n as i32),
        genre: get(&|t| t.genre().map(|s| s.to_string())),
        duration_ms: props.duration().as_millis() as i64,
        bitrate_kbps: props.audio_bitrate().map(|b| b as i32),
    })
}

fn file_mtime(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn scan_folder(conn: &mut Connection, root: &Path) -> Result<ScanReport, ScanError> {
    let mut report = ScanReport::default();
    let tx = conn.transaction()?;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        let Some(ext) = ext else { continue };
        if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        let path_str = path.to_string_lossy().to_string();
        let mtime = file_mtime(path);
        let known_mtime: Option<i64> = tx
            .query_row("SELECT file_mtime FROM tracks WHERE path = ?1", [&path_str], |r| r.get(0))
            .ok();
        if known_mtime == Some(mtime) {
            report.skipped_unchanged += 1;
            continue;
        }
        match read_meta(path) {
            Ok(meta) => {
                let is_update = known_mtime.is_some();
                tx.execute(
                    "INSERT INTO tracks (path, title, artist, album, album_artist, year,
                       track_no, genre, duration_ms, bitrate_kbps, added_at, file_mtime, missing)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,0)
                     ON CONFLICT(path) DO UPDATE SET
                       title=?2, artist=?3, album=?4, album_artist=?5, year=?6,
                       track_no=?7, genre=?8, duration_ms=?9, bitrate_kbps=?10,
                       file_mtime=?12, missing=0",
                    rusqlite::params![
                        path_str,
                        if meta.title.is_empty() {
                            path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string()
                        } else { meta.title },
                        meta.artist, meta.album, meta.album_artist, meta.year,
                        meta.track_no, meta.genre, meta.duration_ms, meta.bitrate_kbps,
                        now_unix(), mtime,
                    ],
                )?;
                if is_update { report.updated += 1 } else { report.added += 1 }
            }
            Err(e) => {
                tx.execute(
                    "INSERT INTO import_errors (path, reason, occurred_at) VALUES (?1,?2,?3)",
                    rusqlite::params![path_str, e.to_string(), now_unix()],
                )?;
                report.errors += 1;
            }
        }
    }
    tx.commit()?;
    Ok(report)
}
```

- [ ] **Step 5: Tests laufen lassen — müssen bestehen**

Run: `cd src-tauri && cargo test`
Expected: PASS (3 Tests). Falls lofty-API-Namen abweichen (z. B. `audio_bitrate`), Compiler-Fehlermeldung lesen und die dokumentierte 0.22-API verwenden — Verhalten der Tests ist die Wahrheit, nicht die exakte Methodenschreibweise.

- [ ] **Step 6: Commit (inkl. Fixture)**

```bash
git add src-tauri && git commit -m "feat: Bibliotheks-Scanner mit lofty-Tags, inkrementellem Rescan und Importfehler-Protokoll"
```

---

### Task 4: Rust — IPC: `scan_folder` + `get_track_window` + `get_library_stats`

**Files:**
- Create: `src-tauri/src/ipc.rs`
- Modify: `src-tauri/src/lib.rs` (AppState, Handler registrieren, DB beim Start öffnen)

**Interfaces:**
- Consumes: `db::*` (Task 2), `library::scanner::*` (Task 3), `models::Track`
- Produces (Tauri-Commands, vom Frontend per `invoke` aufrufbar):
  - `scan_music_folder(root: String) -> ScanReport`
  - `get_track_window(sort_field: String, sort_dir: String, filter: String, offset: i64, limit: i64) -> Vec<Track>` — sort_field ∈ {title, artist, album, year, duration_ms, rating}, whitelisted; Sekundär-Sortierung `album, track_no` bei artist/album
  - `get_library_stats() -> LibraryStats { track_count: i64, total_duration_ms: i64, filtered_count: Option<i64> }`
  - `build_track_query(sort_field, sort_dir, has_filter) -> String` — pure Funktion, unit-getestet
  - AppState: `struct AppState { db: std::sync::Mutex<rusqlite::Connection> }`

- [ ] **Step 1: Failing Test für den Query-Builder**

`src-tauri/src/ipc.rs`:

```rust
use crate::models::Track;
use rusqlite::Connection;
use std::sync::Mutex;
use tauri::State;

pub struct AppState {
    pub db: Mutex<Connection>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub track_count: i64,
    pub total_duration_ms: i64,
}

const SORT_WHITELIST: [(&str, &str); 6] = [
    ("title", "title COLLATE NOCASE"),
    ("artist", "artist COLLATE NOCASE, album COLLATE NOCASE, track_no"),
    ("album", "album COLLATE NOCASE, track_no"),
    ("year", "year"),
    ("duration_ms", "duration_ms"),
    ("rating", "rating"),
];

pub fn build_track_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_builder_whitelists_and_sorts() {
        let q = build_track_query("artist", "asc", false);
        assert!(q.contains("ORDER BY artist COLLATE NOCASE, album COLLATE NOCASE, track_no ASC"));
        assert!(q.contains("WHERE missing = 0"));
        assert!(!q.contains("?3")); // ohne Filter kein Filter-Platzhalter
    }

    #[test]
    fn query_builder_rejects_unknown_column_with_title_fallback() {
        let q = build_track_query("path; DROP TABLE tracks", "desc", true);
        assert!(q.contains("ORDER BY title COLLATE NOCASE DESC"));
        assert!(q.contains("(title LIKE ?3 OR artist LIKE ?3 OR album LIKE ?3 OR genre LIKE ?3)"));
    }

    #[test]
    fn window_returns_filtered_sorted_tracks() {
        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (t, a) in [("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")] {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![format!("/x/{t}.flac"), t, a],
            )
            .unwrap();
        }
        let rows = query_track_window(&mut conn, "title", "asc", "", 0, 10).unwrap();
        assert_eq!(rows[0].title, "Alpha");
        let rows = query_track_window(&mut conn, "title", "asc", "zu", 0, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Zulu");
    }
}
```

- [ ] **Step 2: Tests laufen lassen — müssen fehlschlagen**

Run: `cd src-tauri && cargo test ipc`
Expected: FAIL wegen `todo!()` bzw. fehlendem `query_track_window`.

- [ ] **Step 3: Implementieren**

In `ipc.rs` ergänzen/ersetzen:

```rust
pub fn build_track_query(sort_field: &str, sort_dir: &str, has_filter: bool) -> String {
    let order_expr = SORT_WHITELIST
        .iter()
        .find(|(k, _)| *k == sort_field)
        .map(|(_, v)| *v)
        .unwrap_or("title COLLATE NOCASE");
    let dir = if sort_dir.eq_ignore_ascii_case("desc") { "DESC" } else { "ASC" };
    let filter_clause = if has_filter {
        " AND (title LIKE ?3 OR artist LIKE ?3 OR album LIKE ?3 OR genre LIKE ?3)"
    } else {
        ""
    };
    format!(
        "SELECT id, path, title, artist, album, album_artist, year, track_no, genre, \
         duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
         file_mtime, missing \
         FROM tracks WHERE missing = 0{filter_clause} \
         ORDER BY {order_expr} {dir} LIMIT ?1 OFFSET ?2"
    )
}

fn row_to_track(r: &rusqlite::Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: r.get(0)?, path: r.get(1)?, title: r.get(2)?, artist: r.get(3)?,
        album: r.get(4)?, album_artist: r.get(5)?, year: r.get(6)?, track_no: r.get(7)?,
        genre: r.get(8)?, duration_ms: r.get(9)?, bitrate_kbps: r.get(10)?,
        rating: r.get(11)?, play_count: r.get(12)?, last_played_at: r.get(13)?,
        added_at: r.get(14)?, file_mtime: r.get(15)?, missing: r.get::<_, i64>(16)? != 0,
    })
}

pub fn query_track_window(
    conn: &mut Connection, sort_field: &str, sort_dir: &str,
    filter: &str, offset: i64, limit: i64,
) -> Result<Vec<Track>, rusqlite::Error> {
    let has_filter = !filter.trim().is_empty();
    let sql = build_track_query(sort_field, sort_dir, has_filter);
    let mut stmt = conn.prepare(&sql)?;
    let like = format!("%{}%", filter.trim());
    let rows = if has_filter {
        stmt.query_map(rusqlite::params![limit, offset, like], row_to_track)?
    } else {
        stmt.query_map(rusqlite::params![limit, offset], row_to_track)?
    };
    rows.collect()
}

#[tauri::command]
pub fn get_track_window(
    state: State<AppState>, sort_field: String, sort_dir: String,
    filter: String, offset: i64, limit: i64,
) -> Result<Vec<Track>, String> {
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    query_track_window(&mut conn, &sort_field, &sort_dir, &filter, offset, limit.min(500))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scan_music_folder(
    state: State<AppState>, root: String,
) -> Result<crate::library::scanner::ScanReport, String> {
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    crate::library::scanner::scan_folder(&mut conn, std::path::Path::new(&root))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_library_stats(state: State<AppState>) -> Result<LibraryStats, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT count(*), coalesce(sum(duration_ms),0) FROM tracks WHERE missing = 0",
        [],
        |r| Ok(LibraryStats { track_count: r.get(0)?, total_duration_ms: r.get(1)? }),
    )
    .map_err(|e| e.to_string())
}
```

In `src-tauri/src/lib.rs` den Builder erweitern (generierten `run()` anpassen):

```rust
pub mod db;
pub mod ipc;
pub mod library;
pub mod models;

use ipc::AppState;

fn db_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("reprise/reprise.db")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let conn = db::open(Some(&db_path())).expect("Datenbank kann nicht geöffnet werden");
    db::migrate(&conn).expect("Migration fehlgeschlagen");
    tauri::Builder::default()
        .manage(AppState { db: std::sync::Mutex::new(conn) })
        .invoke_handler(tauri::generate_handler![
            ipc::get_track_window,
            ipc::scan_music_folder,
            ipc::get_library_stats,
        ])
        .run(tauri::generate_context!())
        .expect("Fehler beim Start von Reprise");
}
```

`Cargo.toml`: `dirs = "5"` ergänzen.

- [ ] **Step 4: Tests + Build verifizieren**

Run: `cd src-tauri && cargo test && cargo check`
Expected: alle Tests PASS, `cargo check` ohne Fehler.

- [ ] **Step 5: Commit**

```bash
git add src-tauri && git commit -m "feat: IPC-Commands scan_music_folder, get_track_window (SQL-Windowing), get_library_stats"
```

---

### Task 5: Rust — GStreamer-Player mit Events

**Files:**
- Create: `src-tauri/src/player.rs`
- Modify: `src-tauri/src/lib.rs` (Player in AppState, Commands registrieren)
- Modify: `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: AppState (Task 4)
- Produces (Commands): `play_track(path: String)`, `toggle_pause()`, `seek_to(position_ms: i64)`, `set_volume(volume: f64)` (0.0–1.0), `stop()`
- Produces (Events an Frontend):
  - `player:state` → `{ "state": "playing" | "paused" | "stopped" }`
  - `player:position` → `{ "positionMs": number, "durationMs": number }` (alle 500 ms während der Wiedergabe)
  - `player:track-finished` → `{}` (Stream-Ende; Queue-Logik kommt in Etappe 3)
- Pure Funktion `path_to_uri(path: &str) -> Result<String, PlayerError>` — unit-getestet

- [ ] **Step 1: Dependency**

`src-tauri/Cargo.toml`:

```toml
gstreamer = "0.23"
```

Run: `cd src-tauri && cargo check`
Expected: kompiliert (nutzt System-GStreamer via pkg-config).

- [ ] **Step 2: Failing Test für URI-Konvertierung**

`src-tauri/src/player.rs`:

```rust
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::Mutex;
use tauri::Emitter;

#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    #[error("GStreamer: {0}")]
    Gst(String),
    #[error("Ungültiger Pfad: {0}")]
    BadPath(String),
}

pub fn path_to_uri(path: &str) -> Result<String, PlayerError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_to_uri_encodes_special_chars() {
        let uri = path_to_uri("/home/marvin/Music/Björk/Jóga (Live).flac").unwrap();
        assert!(uri.starts_with("file:///home/marvin/Music/"));
        assert!(uri.contains("J%C3%B3ga%20(Live).flac"));
        assert!(path_to_uri("relativ/pfad.mp3").is_err());
    }
}
```

- [ ] **Step 3: Test laufen lassen — muss fehlschlagen**

Run: `cd src-tauri && cargo test path_to_uri`
Expected: FAIL wegen `todo!()`.

- [ ] **Step 4: Implementieren**

```rust
pub fn path_to_uri(path: &str) -> Result<String, PlayerError> {
    if !path.starts_with('/') {
        return Err(PlayerError::BadPath(path.into()));
    }
    gst::glib::filename_to_uri(path, None)
        .map(|u| u.to_string())
        .map_err(|e| PlayerError::BadPath(e.to_string()))
}

pub struct Player {
    playbin: gst::Element,
    // Muss gehalten werden: Drop des Guards entfernt den Bus-Watch wieder
    _bus_watch: gst::bus::BusWatchGuard,
}

impl Player {
    pub fn new(app: tauri::AppHandle) -> Result<Self, PlayerError> {
        gst::init().map_err(|e| PlayerError::Gst(e.to_string()))?;
        let playbin = gst::ElementFactory::make("playbin3")
            .build()
            .map_err(|e| PlayerError::Gst(e.to_string()))?;

        // Bus-Watch: Stream-Ende + Fehler an das Frontend melden
        let bus = playbin.bus().ok_or_else(|| PlayerError::Gst("kein Bus".into()))?;
        let app_bus = app.clone();
        let bus_watch = bus.add_watch(move |_, msg| {
            use gst::MessageView;
            match msg.view() {
                MessageView::Eos(_) => {
                    let _ = app_bus.emit("player:track-finished", serde_json::json!({}));
                }
                MessageView::Error(e) => {
                    let _ = app_bus.emit(
                        "player:state",
                        serde_json::json!({ "state": "stopped", "error": e.error().to_string() }),
                    );
                }
                _ => {}
            }
            gst::glib::ControlFlow::Continue
        })
        .map_err(|e| PlayerError::Gst(e.to_string()))?;

        // Positions-Ticker: alle 500 ms Position + Dauer emittieren
        let tick_playbin = playbin.clone();
        let app_tick = app.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if tick_playbin.current_state() == gst::State::Playing {
                let pos = tick_playbin
                    .query_position::<gst::ClockTime>()
                    .map(|t| t.mseconds() as i64)
                    .unwrap_or(0);
                let dur = tick_playbin
                    .query_duration::<gst::ClockTime>()
                    .map(|t| t.mseconds() as i64)
                    .unwrap_or(0);
                let _ = app_tick.emit(
                    "player:position",
                    serde_json::json!({ "positionMs": pos, "durationMs": dur }),
                );
            }
        });

        Ok(Self { playbin, _bus_watch: bus_watch })
    }

    pub fn play(&self, path: &str) -> Result<(), PlayerError> {
        let uri = path_to_uri(path)?;
        self.playbin
            .set_state(gst::State::Null)
            .map_err(|e| PlayerError::Gst(e.to_string()))?;
        self.playbin.set_property("uri", &uri);
        self.playbin
            .set_state(gst::State::Playing)
            .map_err(|e| PlayerError::Gst(e.to_string()))?;
        Ok(())
    }

    pub fn toggle_pause(&self) -> Result<&'static str, PlayerError> {
        let next = match self.playbin.current_state() {
            gst::State::Playing => (gst::State::Paused, "paused"),
            _ => (gst::State::Playing, "playing"),
        };
        self.playbin
            .set_state(next.0)
            .map_err(|e| PlayerError::Gst(e.to_string()))?;
        Ok(next.1)
    }

    pub fn seek_to(&self, position_ms: i64) -> Result<(), PlayerError> {
        self.playbin
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_mseconds(position_ms.max(0) as u64),
            )
            .map_err(|e| PlayerError::Gst(e.to_string()))
    }

    pub fn set_volume(&self, volume: f64) {
        self.playbin.set_property("volume", volume.clamp(0.0, 1.0));
    }

    pub fn stop(&self) -> Result<(), PlayerError> {
        self.playbin
            .set_state(gst::State::Null)
            .map(|_| ())
            .map_err(|e| PlayerError::Gst(e.to_string()))
    }
}

pub struct PlayerState(pub Mutex<Option<Player>>);

#[tauri::command]
pub fn play_track(
    app: tauri::AppHandle,
    ps: tauri::State<PlayerState>,
    path: String,
) -> Result<(), String> {
    let mut guard = ps.0.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        *guard = Some(Player::new(app.clone()).map_err(|e| e.to_string())?);
    }
    guard.as_ref().unwrap().play(&path).map_err(|e| e.to_string())?;
    let _ = app.emit("player:state", serde_json::json!({ "state": "playing" }));
    let _ = app.emit("player:track-changed", serde_json::json!({ "path": path }));
    Ok(())
}

#[tauri::command]
pub fn toggle_pause(app: tauri::AppHandle, ps: tauri::State<PlayerState>) -> Result<(), String> {
    let guard = ps.0.lock().map_err(|e| e.to_string())?;
    if let Some(p) = guard.as_ref() {
        let state = p.toggle_pause().map_err(|e| e.to_string())?;
        let _ = app.emit("player:state", serde_json::json!({ "state": state }));
    }
    Ok(())
}

#[tauri::command]
pub fn seek_to(ps: tauri::State<PlayerState>, position_ms: i64) -> Result<(), String> {
    let guard = ps.0.lock().map_err(|e| e.to_string())?;
    if let Some(p) = guard.as_ref() {
        p.seek_to(position_ms).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_volume(ps: tauri::State<PlayerState>, volume: f64) -> Result<(), String> {
    let guard = ps.0.lock().map_err(|e| e.to_string())?;
    if let Some(p) = guard.as_ref() {
        p.set_volume(volume);
    }
    Ok(())
}

#[tauri::command]
pub fn stop(app: tauri::AppHandle, ps: tauri::State<PlayerState>) -> Result<(), String> {
    let guard = ps.0.lock().map_err(|e| e.to_string())?;
    if let Some(p) = guard.as_ref() {
        p.stop().map_err(|e| e.to_string())?;
        let _ = app.emit("player:state", serde_json::json!({ "state": "stopped" }));
    }
    Ok(())
}
```

In `lib.rs`: `pub mod player;`, beim Builder `.manage(player::PlayerState(std::sync::Mutex::new(None)))` und die fünf Player-Commands in `generate_handler![...]` ergänzen. Der Player wird lazy beim ersten `play_track` erzeugt, weil er das `AppHandle` braucht.

- [ ] **Step 5: Tests + Hörprobe**

Run: `cd src-tauri && cargo test && cargo check`
Expected: PASS.

Manuelle Hörprobe (erst nach Task 6 komplett im UI; hier Kurztest):
```bash
gst-launch-1.0 playbin3 uri="$(python3 -c "import pathlib,sys; print(pathlib.Path('src-tauri/tests/fixtures/sine.flac').resolve().as_uri())")"
```
Expected: 1 Sekunde Sinuston — bestätigt, dass die GStreamer-Pipeline systemseitig funktioniert.

- [ ] **Step 6: Commit**

```bash
git add src-tauri && git commit -m "feat: GStreamer-Player (playbin3) mit Play/Pause/Seek/Volume und Positions-Events"
```

---

### Task 6: Frontend — Strings, Format-Helfer, IPC-Wrapper, Design-Tokens

**Files:**
- Create: `src/lib/strings.ts`, `src/lib/format.ts`, `src/lib/ipc.ts`, `src/lib/types.ts`
- Create: `src/styles/tokens.css`, `src/styles/global.css`
- Create: `src/lib/format.test.ts`
- Modify: `package.json`, `vite.config.ts` (Vitest), `src/main.tsx` (CSS-Importe)
- Delete: Scaffold-Reste (`src/App.css`, Logo-Assets)

**Interfaces:**
- Consumes: Commands aus Task 4/5 (exakte Namen: `get_track_window`, `scan_music_folder`, `get_library_stats`, `play_track`, `toggle_pause`, `seek_to`, `set_volume`, `stop`)
- Produces:
  - `types.ts`: `interface Track { id: number; path: string; title: string; artist: string; album: string; albumArtist: string; year: number | null; trackNo: number | null; genre: string; durationMs: number; bitrateKbps: number | null; rating: number; playCount: number; lastPlayedAt: number | null; addedAt: number; fileMtime: number; missing: boolean }`, `interface LibraryStats { trackCount: number; totalDurationMs: number }`, `type SortField = 'title' | 'artist' | 'album' | 'year' | 'duration_ms' | 'rating'`, `type SortDir = 'asc' | 'desc'`
  - `ipc.ts`: `fetchTrackWindow(opts: { sortField: SortField; sortDir: SortDir; filter: string; offset: number; limit: number }): Promise<Track[]>`, `scanFolder(root: string): Promise<{added: number; updated: number; skippedUnchanged: number; errors: number}>`, `fetchLibraryStats(): Promise<LibraryStats>`, `playTrack(path: string)`, `togglePause()`, `seekTo(positionMs: number)`, `setVolume(volume: number)`
  - `format.ts`: `formatDuration(ms: number): string` („3:01", „1:02:33"), `formatTotalDuration(ms: number): string` („4 Tage, 6 Std. 28 Min.")
  - CSS-Tokens: `--color-surface`, `--color-surface-raised`, `--color-text`, `--color-text-dim`, `--color-accent`, `--radius-md`, `--blur-glass`

- [ ] **Step 1: Vitest einrichten**

```bash
npm install -D vitest @testing-library/react @testing-library/user-event @testing-library/jest-dom jsdom
npm install zustand
```

`vite.config.ts` um Test-Block ergänzen:

```ts
/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: "./src/test-setup.ts",
  },
});
```

`src/test-setup.ts`: `import "@testing-library/jest-dom/vitest";`
`package.json` scripts: `"test": "vitest run", "test:watch": "vitest"`.

- [ ] **Step 2: Failing Test für format.ts**

`src/lib/format.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { formatDuration, formatTotalDuration } from "./format";

describe("formatDuration", () => {
  it("formatiert Minuten:Sekunden mit führender Null", () => {
    expect(formatDuration(181_000)).toBe("3:01");
    expect(formatDuration(59_000)).toBe("0:59");
  });
  it("formatiert Stunden", () => {
    expect(formatDuration(3_753_000)).toBe("1:02:33");
  });
  it("ist robust gegen Unsinn", () => {
    expect(formatDuration(-5)).toBe("0:00");
    expect(formatDuration(NaN)).toBe("0:00");
  });
});

describe("formatTotalDuration", () => {
  it("formatiert Tage/Stunden/Minuten wie Rhythmbox", () => {
    const ms = ((4 * 24 + 6) * 60 + 28) * 60 * 1000;
    expect(formatTotalDuration(ms)).toBe("4 Tage, 6 Std. 28 Min.");
    expect(formatTotalDuration(90 * 60 * 1000)).toBe("1 Std. 30 Min.");
    expect(formatTotalDuration(5 * 60 * 1000)).toBe("5 Min.");
  });
});
```

- [ ] **Step 3: Test laufen lassen — muss fehlschlagen**

Run: `npm test`
Expected: FAIL („Cannot find module ./format").

- [ ] **Step 4: Implementieren**

`src/lib/format.ts`:

```ts
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "0:00";
  const totalSec = Math.floor(ms / 1000);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  const mm = h > 0 ? String(m).padStart(2, "0") : String(m);
  return h > 0
    ? `${h}:${mm}:${String(s).padStart(2, "0")}`
    : `${mm}:${String(s).padStart(2, "0")}`;
}

export function formatTotalDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "0 Min.";
  const totalMin = Math.floor(ms / 60_000);
  const days = Math.floor(totalMin / (60 * 24));
  const hours = Math.floor((totalMin % (60 * 24)) / 60);
  const mins = totalMin % 60;
  const parts: string[] = [];
  if (days > 0) parts.push(`${days} ${days === 1 ? "Tag" : "Tage"},`);
  if (hours > 0) parts.push(`${hours} Std.`);
  parts.push(`${mins} Min.`);
  return parts.join(" ");
}
```

`src/lib/types.ts` und `src/lib/ipc.ts` exakt nach dem Interfaces-Block dieses Tasks:

```ts
// types.ts — siehe Interfaces-Block oben, 1:1 übernehmen
```

```ts
// ipc.ts
import { invoke } from "@tauri-apps/api/core";
import type { LibraryStats, SortDir, SortField, Track } from "./types";

export function fetchTrackWindow(opts: {
  sortField: SortField; sortDir: SortDir; filter: string; offset: number; limit: number;
}): Promise<Track[]> {
  return invoke("get_track_window", {
    sortField: opts.sortField, sortDir: opts.sortDir,
    filter: opts.filter, offset: opts.offset, limit: opts.limit,
  });
}

export function scanFolder(root: string) {
  return invoke<{ added: number; updated: number; skippedUnchanged: number; errors: number }>(
    "scan_music_folder", { root },
  );
}

export const fetchLibraryStats = () => invoke<LibraryStats>("get_library_stats");
export const playTrack = (path: string) => invoke<void>("play_track", { path });
export const togglePause = () => invoke<void>("toggle_pause");
export const seekTo = (positionMs: number) => invoke<void>("seek_to", { positionMs });
export const setVolume = (volume: number) => invoke<void>("set_volume", { volume });
```

`src/lib/strings.ts` (wächst mit jeder UI-Komponente):

```ts
export const STRINGS = {
  appName: "Reprise",
  searchPlaceholder: "Alle Felder durchsuchen",
  scanFolder: "Ordner scannen…",
  columns: { title: "Titel", artist: "Interpret", album: "Album", year: "Jahr", duration: "Länge", rating: "Bewertung" },
  statusTracks: (n: number) => `${n.toLocaleString("de-DE")} Titel`,
  play: "Wiedergabe", pause: "Pause",
} as const;
```

`src/styles/tokens.css` (dunkles 2a-Schema, Etappe 5 verfeinert):

```css
:root {
  --color-surface: oklch(16% 0.01 260);
  --color-surface-raised: oklch(21% 0.015 260);
  --color-text: oklch(93% 0.005 260);
  --color-text-dim: oklch(65% 0.01 260);
  --color-accent: oklch(70% 0.12 230);
  --radius-md: 10px;
  --blur-glass: blur(24px);
  --duration-fast: 150ms;
}
```

`src/styles/global.css`: Reset (`box-sizing`, `margin:0`), `body { background: var(--color-surface); color: var(--color-text); font-family: system-ui, sans-serif; }`. In `src/main.tsx` beide CSS-Dateien importieren, Scaffold-CSS/Logos löschen.

- [ ] **Step 5: Tests laufen lassen — müssen bestehen**

Run: `npm test`
Expected: PASS (5 Tests).

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: Frontend-Fundament — Strings, Formatierung, typisierte IPC-Wrapper, Design-Tokens"
```

---

### Task 7: Frontend — Track-Tabelle mit Sortierung und Doppelklick-Wiedergabe

**Files:**
- Create: `src/components/track-table/TrackTable.tsx`, `src/components/track-table/track-table.css`
- Create: `src/components/track-table/TrackTable.test.tsx`
- Create: `src/store/library.ts` (Zustand-Store)
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `fetchTrackWindow`, `playTrack` (Task 6), `STRINGS.columns`
- Produces:
  - `<TrackTable />` — lädt Fenster (limit 200, Nachladen bei Scroll-Ende folgt in Etappe 2 mit TanStack Virtual), Spaltenkopf-Klick toggelt Sortierung, Doppelklick auf Zeile ruft `playTrack(track.path)`
  - Store `useLibraryStore`: `{ tracks: Track[], sortField: SortField, sortDir: SortDir, filter: string, setSort(f: SortField): void, setFilter(s: string): void, reload(): Promise<void> }`

- [ ] **Step 1: Failing Component-Test**

`src/components/track-table/TrackTable.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Track } from "../../lib/types";

const sample: Track[] = [
  { id: 1, path: "/m/a.flac", title: "Beast of Darkness", artist: "Brand of Sacrifice",
    album: "God Hand", albumArtist: "", year: 2019, trackNo: 9, genre: "", durationMs: 181000,
    bitrateKbps: 900, rating: 5, playCount: 3, lastPlayedAt: null, addedAt: 0, fileMtime: 0, missing: false },
  { id: 2, path: "/m/b.flac", title: "Dynasty", artist: "Brand of Sacrifice",
    album: "Between Death and Dreams", albumArtist: "", year: 2023, trackNo: 2, genre: "", durationMs: 266000,
    bitrateKbps: 900, rating: 4, playCount: 1, lastPlayedAt: null, addedAt: 0, fileMtime: 0, missing: false },
];

const fetchTrackWindow = vi.fn(async () => sample);
const playTrack = vi.fn(async () => {});
vi.mock("../../lib/ipc", () => ({
  fetchTrackWindow: (...a: unknown[]) => fetchTrackWindow(...a),
  playTrack: (...a: unknown[]) => playTrack(...a),
}));

import { TrackTable } from "./TrackTable";
import { useLibraryStore } from "../../store/library";

beforeEach(() => {
  useLibraryStore.setState({ tracks: [], sortField: "artist", sortDir: "asc", filter: "" });
  vi.clearAllMocks();
});

describe("TrackTable", () => {
  it("zeigt geladene Titel mit formatierter Länge", async () => {
    render(<TrackTable />);
    expect(await screen.findByText("Beast of Darkness")).toBeInTheDocument();
    expect(screen.getByText("3:01")).toBeInTheDocument();
  });

  it("Doppelklick spielt den Titel", async () => {
    render(<TrackTable />);
    await userEvent.dblClick(await screen.findByText("Dynasty"));
    expect(playTrack).toHaveBeenCalledWith("/m/b.flac");
  });

  it("Klick auf Spaltenkopf ändert die Sortierung", async () => {
    render(<TrackTable />);
    await screen.findByText("Dynasty");
    await userEvent.click(screen.getByRole("columnheader", { name: /Jahr/ }));
    expect(useLibraryStore.getState().sortField).toBe("year");
  });
});
```

- [ ] **Step 2: Test laufen lassen — muss fehlschlagen**

Run: `npm test`
Expected: FAIL („Cannot find module ./TrackTable").

- [ ] **Step 3: Store implementieren**

`src/store/library.ts`:

```ts
import { create } from "zustand";
import { fetchTrackWindow } from "../lib/ipc";
import type { SortDir, SortField, Track } from "../lib/types";

interface LibraryState {
  tracks: Track[];
  sortField: SortField;
  sortDir: SortDir;
  filter: string;
  setSort: (field: SortField) => void;
  setFilter: (filter: string) => void;
  reload: () => Promise<void>;
}

export const useLibraryStore = create<LibraryState>((set, get) => ({
  tracks: [],
  sortField: "artist",
  sortDir: "asc",
  filter: "",
  setSort: (field) => {
    const { sortField, sortDir } = get();
    const nextDir: SortDir = field === sortField && sortDir === "asc" ? "desc" : "asc";
    set({ sortField: field, sortDir: nextDir });
    void get().reload();
  },
  setFilter: (filter) => {
    set({ filter });
    void get().reload();
  },
  reload: async () => {
    const { sortField, sortDir, filter } = get();
    try {
      const tracks = await fetchTrackWindow({ sortField, sortDir, filter, offset: 0, limit: 200 });
      set({ tracks });
    } catch (err) {
      console.error("Bibliothek laden fehlgeschlagen:", err);
    }
  },
}));
```

- [ ] **Step 4: Komponente implementieren**

`src/components/track-table/TrackTable.tsx`:

```tsx
import { useEffect } from "react";
import { playTrack } from "../../lib/ipc";
import { formatDuration } from "../../lib/format";
import { STRINGS } from "../../lib/strings";
import type { SortField } from "../../lib/types";
import { useLibraryStore } from "../../store/library";
import "./track-table.css";

const COLUMNS: { field: SortField; label: string }[] = [
  { field: "title", label: STRINGS.columns.title },
  { field: "artist", label: STRINGS.columns.artist },
  { field: "album", label: STRINGS.columns.album },
  { field: "year", label: STRINGS.columns.year },
  { field: "duration_ms", label: STRINGS.columns.duration },
  { field: "rating", label: STRINGS.columns.rating },
];

export function TrackTable() {
  const { tracks, sortField, sortDir, setSort, reload } = useLibraryStore();

  useEffect(() => {
    void reload();
  }, [reload]);

  return (
    <table className="track-table">
      <thead>
        <tr>
          {COLUMNS.map(({ field, label }) => (
            <th
              key={field}
              role="columnheader"
              aria-sort={sortField === field ? (sortDir === "asc" ? "ascending" : "descending") : "none"}
              onClick={() => setSort(field)}
            >
              {label}
              {sortField === field ? (sortDir === "asc" ? " ▲" : " ▼") : ""}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {tracks.map((t) => (
          <tr key={t.id} onDoubleClick={() => void playTrack(t.path)}>
            <td>{t.title}</td>
            <td>{t.artist}</td>
            <td>{t.album}</td>
            <td>{t.year ?? ""}</td>
            <td className="num">{formatDuration(t.durationMs)}</td>
            <td>{"★".repeat(t.rating)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
```

`track-table.css`: Spaltenbreiten (`table-layout: fixed`), Zeilen-Hover (`background: var(--color-surface-raised)`), `th { cursor: pointer; text-transform: uppercase; letter-spacing: 0.08em; font-size: 0.75rem; color: var(--color-text-dim); user-select: none; }`, `.num { text-align: right; font-variant-numeric: tabular-nums; }`.

- [ ] **Step 5: Tests laufen lassen — müssen bestehen**

Run: `npm test`
Expected: PASS (alle Tests inkl. Task 6).

- [ ] **Step 6: In App einhängen + Commit**

`src/App.tsx` minimal ersetzen:

```tsx
import { TrackTable } from "./components/track-table/TrackTable";

export default function App() {
  return (
    <main className="app-shell">
      <TrackTable />
    </main>
  );
}
```

```bash
git add -A && git commit -m "feat: sortierbare Track-Tabelle mit Doppelklick-Wiedergabe"
```

---

### Task 8: Frontend — Playerleiste (Layout 2a: unten) + Player-Events

**Files:**
- Create: `src/components/player-bar/PlayerBar.tsx`, `src/components/player-bar/player-bar.css`
- Create: `src/components/player-bar/PlayerBar.test.tsx`
- Create: `src/hooks/usePlayerEvents.ts`, `src/store/player.ts`
- Modify: `src/App.tsx`, `src/lib/strings.ts`

**Interfaces:**
- Consumes: `togglePause`, `seekTo`, `setVolume` (Task 6); Tauri-Events `player:state`, `player:position`, `player:track-changed` (Task 5); `useLibraryStore` (Titel-Metadaten zum Pfad)
- Produces:
  - Store `usePlayerStore`: `{ state: 'playing' | 'paused' | 'stopped', currentPath: string | null, positionMs: number, durationMs: number }` + Setter
  - `usePlayerEvents()` — abonniert die drei Tauri-Events (via `@tauri-apps/api/event` `listen`), schreibt in den Store, räumt beim Unmount auf
  - `<PlayerBar />` — fixe Leiste unten: Titel/Interpret links, Play/Pause-Button + Seekbar (`<input type="range">`) + Zeit „1:07 / 3:01" mittig, Lautstärke-Slider rechts

- [ ] **Step 1: Failing Test**

`src/components/player-bar/PlayerBar.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const togglePause = vi.fn(async () => {});
const seekTo = vi.fn(async () => {});
vi.mock("../../lib/ipc", () => ({
  togglePause: () => togglePause(),
  seekTo: (ms: number) => seekTo(ms),
  setVolume: async () => {},
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { PlayerBar } from "./PlayerBar";
import { usePlayerStore } from "../../store/player";

beforeEach(() => {
  usePlayerStore.setState({
    state: "playing", currentPath: "/m/a.flac",
    currentTitle: "Beast of Darkness", currentArtist: "Brand of Sacrifice",
    positionMs: 67_000, durationMs: 181_000,
  });
  vi.clearAllMocks();
});

describe("PlayerBar", () => {
  it("zeigt Titel, Interpret und Zeit", () => {
    render(<PlayerBar />);
    expect(screen.getByText("Beast of Darkness")).toBeInTheDocument();
    expect(screen.getByText("Brand of Sacrifice")).toBeInTheDocument();
    expect(screen.getByText("1:07")).toBeInTheDocument();
    expect(screen.getByText("3:01")).toBeInTheDocument();
  });

  it("Play/Pause-Button ruft togglePause", async () => {
    render(<PlayerBar />);
    await userEvent.click(screen.getByRole("button", { name: "Pause" }));
    expect(togglePause).toHaveBeenCalledOnce();
  });

  it("Seekbar-Änderung ruft seekTo mit dem Zielwert", () => {
    render(<PlayerBar />);
    const slider = screen.getByRole("slider", { name: "Wiedergabeposition" });
    // fireEvent statt userEvent: range-Inputs unterstützen kein Tippen
    fireEvent.change(slider, { target: { value: "120000" } });
    expect(seekTo).toHaveBeenCalledWith(120000);
  });
});
```

- [ ] **Step 2: Test laufen lassen — muss fehlschlagen**

Run: `npm test`
Expected: FAIL (Module fehlen).

- [ ] **Step 3: Store + Hook implementieren**

`src/store/player.ts`:

```ts
import { create } from "zustand";

export type PlaybackState = "playing" | "paused" | "stopped";

interface PlayerState {
  state: PlaybackState;
  currentPath: string | null;
  currentTitle: string;
  currentArtist: string;
  positionMs: number;
  durationMs: number;
}

export const usePlayerStore = create<PlayerState>(() => ({
  state: "stopped",
  currentPath: null,
  currentTitle: "",
  currentArtist: "",
  positionMs: 0,
  durationMs: 0,
}));
```

`src/hooks/usePlayerEvents.ts`:

```ts
import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { usePlayerStore, type PlaybackState } from "../store/player";
import { useLibraryStore } from "../store/library";

export function usePlayerEvents() {
  useEffect(() => {
    const subs = [
      listen<{ state: PlaybackState }>("player:state", (e) =>
        usePlayerStore.setState({ state: e.payload.state }),
      ),
      listen<{ positionMs: number; durationMs: number }>("player:position", (e) =>
        usePlayerStore.setState({
          positionMs: e.payload.positionMs,
          durationMs: e.payload.durationMs,
        }),
      ),
      listen<{ path: string }>("player:track-changed", (e) => {
        const track = useLibraryStore.getState().tracks.find((t) => t.path === e.payload.path);
        usePlayerStore.setState({
          currentPath: e.payload.path,
          currentTitle: track?.title ?? e.payload.path.split("/").pop() ?? "",
          currentArtist: track?.artist ?? "",
          positionMs: 0,
        });
      }),
    ];
    return () => {
      for (const s of subs) void s.then((unlisten) => unlisten());
    };
  }, []);
}
```

- [ ] **Step 4: PlayerBar implementieren**

`src/lib/strings.ts` ergänzen: `playerPosition: "Wiedergabeposition", volume: "Lautstärke",`.

`src/components/player-bar/PlayerBar.tsx`:

```tsx
import { seekTo, setVolume, togglePause } from "../../lib/ipc";
import { formatDuration } from "../../lib/format";
import { STRINGS } from "../../lib/strings";
import { usePlayerStore } from "../../store/player";
import "./player-bar.css";

export function PlayerBar() {
  const { state, currentTitle, currentArtist, positionMs, durationMs } = usePlayerStore();
  const isPlaying = state === "playing";

  return (
    <footer className="player-bar">
      <div className="player-bar__info">
        <span className="player-bar__title">{currentTitle}</span>
        <span className="player-bar__artist">{currentArtist}</span>
      </div>
      <div className="player-bar__transport">
        <button
          type="button"
          aria-label={isPlaying ? STRINGS.pause : STRINGS.play}
          onClick={() => void togglePause()}
          disabled={state === "stopped"}
        >
          {isPlaying ? "⏸" : "▶"}
        </button>
        <span className="player-bar__time">{formatDuration(positionMs)}</span>
        <input
          type="range"
          aria-label={STRINGS.playerPosition}
          min={0}
          max={Math.max(durationMs, 1)}
          value={Math.min(positionMs, durationMs)}
          onChange={(e) => void seekTo(Number(e.currentTarget.value))}
        />
        <span className="player-bar__time">{formatDuration(durationMs)}</span>
      </div>
      <div className="player-bar__volume">
        <input
          type="range"
          aria-label={STRINGS.volume}
          min={0}
          max={100}
          defaultValue={100}
          onChange={(e) => void setVolume(Number(e.currentTarget.value) / 100)}
        />
      </div>
    </footer>
  );
}
```

`player-bar.css`: Grid `grid-template-columns: 1fr auto 1fr`, `position: sticky; bottom: 0`, Hintergrund `color-mix(in oklch, var(--color-surface-raised) 85%, transparent)` mit `backdrop-filter: var(--blur-glass)` (erster Blur-Baustein!), Seekbar `width: min(40vw, 480px)`.

`src/App.tsx`:

```tsx
import { TrackTable } from "./components/track-table/TrackTable";
import { PlayerBar } from "./components/player-bar/PlayerBar";
import { usePlayerEvents } from "./hooks/usePlayerEvents";

export default function App() {
  usePlayerEvents();
  return (
    <main className="app-shell">
      <div className="app-shell__content">
        <TrackTable />
      </div>
      <PlayerBar />
    </main>
  );
}
```

`global.css` ergänzen: `.app-shell { display: flex; flex-direction: column; height: 100vh; } .app-shell__content { flex: 1; overflow-y: auto; }`.

- [ ] **Step 5: Tests laufen lassen — müssen bestehen**

Run: `npm test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: Playerleiste unten (2a) mit Play/Pause, Seekbar, Zeit und Lautstärke"
```

---

### Task 9: Scan-Flow — Ordner wählen, scannen, Statusleiste

**Files:**
- Create: `src/components/toolbar/Toolbar.tsx`, `src/components/toolbar/toolbar.css`, `src/components/toolbar/Toolbar.test.tsx`
- Create: `src/components/status-bar/StatusBar.tsx`
- Modify: `src/App.tsx`, `src/lib/strings.ts`, `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`, `package.json`

**Interfaces:**
- Consumes: `scanFolder`, `fetchLibraryStats`, `useLibraryStore.reload`, `STRINGS`
- Produces: Toolbar mit Suchfeld (filtert live über `setFilter`) und Button „Ordner scannen…" (öffnet Tauri-Dialog, scannt, lädt Liste neu); Statusleiste „1.704 Titel, 4 Tage, 6 Std. 28 Min."

- [ ] **Step 1: Dialog-Plugin installieren**

```bash
npm run tauri add dialog
```
(Das registriert Rust-Plugin + npm-Paket + Capability automatisch. Verifizieren: `src-tauri/capabilities/default.json` enthält `"dialog:default"`.)

- [ ] **Step 2: Failing Test**

`src/components/toolbar/Toolbar.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const open = vi.fn(async () => "/home/marvin/Music");
const scanFolder = vi.fn(async () => ({ added: 3, updated: 0, skippedUnchanged: 0, errors: 0 }));
const reload = vi.fn(async () => {});
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: () => open() }));
vi.mock("../../lib/ipc", async (orig) => ({
  ...(await orig<Record<string, unknown>>()),
  scanFolder: (root: string) => scanFolder(root),
  fetchTrackWindow: async () => [],
}));

import { Toolbar } from "./Toolbar";
import { useLibraryStore } from "../../store/library";

beforeEach(() => {
  useLibraryStore.setState({ reload, filter: "" });
  vi.clearAllMocks();
});

describe("Toolbar", () => {
  it("Scan-Button öffnet Dialog, scannt und lädt neu", async () => {
    render(<Toolbar />);
    await userEvent.click(screen.getByRole("button", { name: /Ordner scannen/ }));
    expect(open).toHaveBeenCalledOnce();
    expect(scanFolder).toHaveBeenCalledWith("/home/marvin/Music");
    expect(reload).toHaveBeenCalled();
  });

  it("Suchfeld setzt den Filter", async () => {
    render(<Toolbar />);
    await userEvent.type(screen.getByPlaceholderText("Alle Felder durchsuchen"), "brand");
    expect(useLibraryStore.getState().filter).toBe("brand");
  });
});
```

- [ ] **Step 3: Test laufen lassen — muss fehlschlagen**

Run: `npm test`
Expected: FAIL (Toolbar fehlt).

- [ ] **Step 4: Implementieren**

`src/components/toolbar/Toolbar.tsx`:

```tsx
import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { scanFolder } from "../../lib/ipc";
import { STRINGS } from "../../lib/strings";
import { useLibraryStore } from "../../store/library";
import "./toolbar.css";

export function Toolbar() {
  const { filter, setFilter, reload } = useLibraryStore();
  const [isScanning, setIsScanning] = useState(false);

  async function handleScan() {
    const dir = await open({ directory: true, title: STRINGS.scanFolder });
    if (typeof dir !== "string") return;
    setIsScanning(true);
    try {
      await scanFolder(dir);
      await reload();
    } catch (err) {
      console.error("Scan fehlgeschlagen:", err);
    } finally {
      setIsScanning(false);
    }
  }

  return (
    <header className="toolbar">
      <h1 className="toolbar__brand">{STRINGS.appName}</h1>
      <input
        type="search"
        className="toolbar__search"
        placeholder={STRINGS.searchPlaceholder}
        value={filter}
        onChange={(e) => setFilter(e.currentTarget.value)}
      />
      <button type="button" onClick={() => void handleScan()} disabled={isScanning}>
        {isScanning ? "Scanne…" : STRINGS.scanFolder}
      </button>
    </header>
  );
}
```

`StatusBar.tsx` (lädt Stats nach jedem `tracks`-Wechsel):

```tsx
import { useEffect, useState } from "react";
import { fetchLibraryStats } from "../../lib/ipc";
import { formatTotalDuration } from "../../lib/format";
import { STRINGS } from "../../lib/strings";
import { useLibraryStore } from "../../store/library";

export function StatusBar() {
  const tracks = useLibraryStore((s) => s.tracks);
  const [stats, setStats] = useState({ trackCount: 0, totalDurationMs: 0 });

  useEffect(() => {
    fetchLibraryStats().then(setStats).catch((e) => console.error(e));
  }, [tracks]);

  return (
    <div className="status-bar">
      {STRINGS.statusTracks(stats.trackCount)}, {formatTotalDuration(stats.totalDurationMs)}
    </div>
  );
}
```

In `App.tsx` einhängen: Toolbar oben, StatusBar zwischen Content und PlayerBar.

- [ ] **Step 5: Tests laufen lassen — müssen bestehen**

Run: `npm test`
Expected: PASS (alle Frontend-Tests).

- [ ] **Step 6: End-to-End-Hörprobe (manuell)**

Run: `npm run tauri dev`
Ablauf: „Ordner scannen…" → `~/Music` wählen → Liste füllt sich → Doppelklick auf einen Titel → **Musik spielt**, Playerleiste zeigt Titel + laufende Zeit, Seekbar springt beim Ziehen, Pause pausiert, Suchfeld filtert, Statusleiste zeigt Titelanzahl und Gesamtdauer. Spaltenkopf-Klick sortiert.

- [ ] **Step 7: Commit + Etappen-Abschluss**

```bash
git add -A && git commit -m "feat: Scan-Flow mit Ordnerdialog, Live-Suche und Statusleiste — Etappe 1 komplett"
```

---

## Verifikation Etappe 1 (Definition of Done)

- [ ] `cd src-tauri && cargo test` — alle Rust-Tests grün
- [ ] `npm test` — alle Frontend-Tests grün
- [ ] `npm run tauri dev` — App startet, Scan → Liste → Doppelklick → hörbare Wiedergabe, Seek/Pause/Lautstärke funktionieren
- [ ] Zweiter Scan desselben Ordners ist schnell (inkrementell) und verdoppelt nichts
- [ ] Defekte Datei im Musikordner erscheint in `import_errors` (DB prüfen: `sqlite3 ~/.local/share/reprise/reprise.db "SELECT * FROM import_errors"`)

**Nicht in Etappe 1** (kommt laut Spec in Etappe 2–6): virtuelles Scrolling (TanStack Virtual), Browse-Leiste, Cover, Sidebar, Playlists, Bewertungen setzen, Warteschlange, MPRIS, Rhythmbox-Import, Tag-Editor, Löschen, Watcher, Einstellungen, EQ/ReplayGain, Layout-Varianten, Blur-Vollausbau, Flatpak.
