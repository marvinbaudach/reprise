# Rhythmbox-Import-Redesign — Designspezifikation

## Ziel

Der bisherige Rhythmbox-Import (ActionRow → Push-NavigationPage → 6 SwitchRows →
AlertDialog) wird durch einen dreistufigen Dialog (Auswahl → Fortschritt →
Abschluss) ersetzt, der dem kanonischen Mock-Frame 12 entspricht. Der Nutzer
sieht VOR dem Import Bibliotheks-Metadaten (Pfad, Einträge, Treffer, letztes
Datum), wählt aus fünf statt sechs Optionen (Play counts + Last played
zusammengelegt), verfolgt den Import live mit Fortschrittsbalken und erhält am
Ende eine detaillierte Zusammenfassung mit aufklappbarem Skip-Grund-Bereich,
Undo-Möglichkeit und einem „Done"-Button.

## Umfang

### Enthalten

- **Backend (`reprise-core`):**
  - `prescan_rhythmdb()` — parst rhythmdb.xml und liefert Bibliotheks-Metadaten
    + Treffer gegen die Reprise-DB zurück, OHNE etwas zu schreiben.
  - Zusammenlegung von `play_counts` und `last_played_at` in einen einzigen
    Bool `play_counts_and_last_played` in `RhythmboxImportChoices`.
  - Fortschritts-Callback für `merge_stats()` — meldet je verarbeiteten Track.
  - Undo-Unterstützung: `merge_stats()` gibt ein `RhythmboxRollback` zurück;
    neue Funktion `undo_rhythmbox_import()` stellt Originalwerte wieder her.
  - Skip-Aufschlüsselung im Prescan: außerhalb Library-Root, fehlt auf Disk,
    Podcast/Stream (entry type ≠ song).

- **Frontend (`reprise-gnome`):**
  - Neuer dreistufiger `adw::Dialog` in `preference_rhythmbox.rs` (Selection →
    Progress → Complete), ersetzt `push_import_page`.
  - Selection-State: Bibliotheks-Info-Banner, 5 SwitchRows mit beschreibenden
    Subtitles (Anzahlen aus Prescan), Warning-Zeile für übersprungene Einträge.
  - Progress-State: `adw::ProgressBar` mit Fraction-Update via Idle-Callback,
    live Kategorie-Zähler.
  - Complete-State: Zusammenfassungs-Tabelle, aufklappbare Skip-Details
    (`AdwExpanderRow`), „Undo import"-Link + „Done"-Button.
  - Neue/aktualisierte Strings in `strings_rhythmbox.rs`.

### Nicht enthalten

- Änderung der Datenbankschema-Migration — Rollback speichert alte Werte in
  einer temporären In-Memory-Struktur, nicht in einer DB-Tabelle.
- „Save list as file" für übersprungene Einträge (aus dem Mock; niedrige
  Priorität — wird als Follow-up zurückgestellt).
- „Review in Missing files"-Link (erfordert Navigations-Cross-Concern; Follow-up).
- Änderungen am First-Run-Wizard — der bleibt wie bisher.

## Architektur

### Backend: `prescan_rhythmdb()`

Neue Funktion neben `parse_rhythmdb()` in `rhythmbox_import.rs`. Nutzt
dieselbe XML-Parsing-Logik, zählt aber ALLE `<entry>`-Elemente (nicht nur
`type="song"`) und gleicht Pfade gegen die Datenbank ab.

```rust
pub struct RhythmboxPrescanResult {
    pub rhythmdb_path: PathBuf,
    pub total_entries: usize,            // alle entry-Elemente
    pub song_entries: usize,             // type="song"
    pub non_song_entries: usize,         // podcasts, streams, etc.
    pub rated_tracks: usize,             // songs mit rating > 0
    pub tracks_with_history: usize,      // songs mit play_count > 0 oder last_played > 0
    pub tracks_with_date_added: usize,   // songs mit first_seen > 0
    pub matched: usize,                  // songs deren Pfad in Reprise-DB existiert
    pub outside_library: usize,          // songs deren Pfad nicht unter library_root liegt
    pub missing_on_disk: usize,          // songs deren Pfad unter library_root liegt, aber Datei fehlt
    pub playlist_count: usize,           // statische Playlists
    pub playlist_track_count: usize,     // Gesamtzahl Tracks in allen Playlists
    pub last_modified: Option<std::time::SystemTime>,  // mtime der rhythmdb.xml
}

pub fn prescan_rhythmdb(
    rhythmdb_path: &Path,
    playlists_path: &Path,
    conn: &Connection,
    library_root: Option<&str>,
) -> Result<RhythmboxPrescanResult, RhythmboxImportError>;
```

Die Funktion ist read-only und kann gefahrlos vom UI-Thread (über
`gio::spawn_blocking`) aufgerufen werden, bevor der Nutzer „Import" klickt.

### Backend: Zusammengelegte Choices

```rust
pub struct RhythmboxImportChoices {
    pub ratings: bool,
    pub play_counts_and_last_played: bool,  // war: play_counts + last_played_at getrennt
    pub added_at: bool,
}
```

Intern setzt `merge_stats` bei `play_counts_and_last_played == true` sowohl
`play_count` als auch `last_played_at`. Das Ergebnis in
`RhythmboxImportSummary` behält `play_counts_raised` und
`last_played_imported` als getrennte Felder (für die Anzeige).

### Backend: Fortschritts-Callback

`merge_stats` erhält einen optionalen Callback:

```rust
pub fn merge_stats(
    conn: &mut Connection,
    tracks: &[RhythmboxTrackStats],
    choices: RhythmboxImportChoices,
    on_progress: Option<&dyn Fn(usize)>,  // tracks_processed so far
) -> Result<(RhythmboxImportSummary, RhythmboxRollback), RhythmboxImportError>;
```

Der Callback wird nach jedem Track aufgerufen. Das Frontend nutzt
`glib::idle_add_local` um die UI zu aktualisieren (Callback selbst läuft im
Blocking-Thread).

### Backend: Undo

```rust
pub struct RhythmboxRollbackEntry {
    pub path: String,
    pub rating: i32,
    pub play_count: i64,
    pub added_at: i64,
    pub last_played_at: Option<i64>,
}

pub struct RhythmboxRollback {
    pub entries: Vec<RhythmboxRollbackEntry>,
}

pub fn undo_rhythmbox_import(
    conn: &mut Connection,
    rollback: &RhythmboxRollback,
) -> Result<usize, RhythmboxImportError>;
```

`merge_stats` sammelt die aktuellen Werte BEVOR es sie überschreibt und gibt
sie als `RhythmboxRollback` zurück. `undo_rhythmbox_import` schreibt die
Originalwerte in einer Transaktion zurück. Der Rollback wird im Frontend
als `Rc<RefCell<Option<RhythmboxRollback>>>` gehalten — bei Dialog-Schließen
oder neuer Navigation wird er verworfen.

Playlists-Undo ist out of scope (komplex wegen Merge-Semantik). Der
Undo-Button stellt nur Track-Statistiken wieder her, Playlists bleiben
bestehen. Der Button-Text sagt klar „Undo import (statistics only)".

### Frontend: Dialog-Aufbau

`preference_rhythmbox.rs` wird umgebaut. Die Funktion `push_import_page`
entfällt. Stattdessen baut `open_rhythmbox_import` direkt einen
`adw::Dialog` mit drei Content-Zuständen:

```
┌──────────────────────────────────────────────┐
│  Cancel     Import from Rhythmbox    Import  │  ← HeaderBar (Selection)
├──────────────────────────────────────────────┤
│  ✓  Rhythmbox library found                  │
│     ~/.local/share/rhythmbox/rhythmdb.xml    │
│     1,603 entries · last used 3 days ago     │
│     1,685 match your library                 │
│                                              │
│  Choose what to copy into Reprise.           │
│  Rhythmbox and your audio files remain       │
│  unchanged — you can undo the whole          │
│  operation.                                  │
│                                              │
│  ┌────────────────────────────────────────┐  │
│  │ Ratings               542 rated   [■] │  │
│  │ Play counts & last played              │  │
│  │   1,412 tracks with history       [■] │  │
│  │ Date added                             │  │
│  │   original added-to-library …     [■] │  │
│  │ Playlists                              │  │
│  │   14 playlists · 630 tracks       [■] │  │
│  │ Column layout                          │  │
│  │   replaces your current table …   [□] │  │
│  └────────────────────────────────────────┘  │
│                                              │
│  ⚠ 918 entries point to files outside your   │
│    library folder — they will be skipped.    │
└──────────────────────────────────────────────┘
```

Intern ein `gtk4::Stack` mit drei benannten Children:
- `"selection"` — PreferencesPage mit Info-Banner + SwitchRows + Warning
- `"progress"` — Box mit ProgressBar + Zähler-Labels
- `"complete"` — Box mit Ergebnis-Tabelle + ExpanderRow + Buttons

Der Dialog nutzt `content_width(560)` und `content_height(-1)` (natural).

### Frontend: Prescan-Flow

1. `open_rhythmbox_import` zeigt den Dialog sofort mit einem Spinner /
   „Scanning Rhythmbox library…" Placeholder.
2. `gio::spawn_blocking` ruft `prescan_rhythmdb()` auf.
3. Im Idle-Callback werden die SwitchRow-Subtitles und das Info-Banner
   mit den Prescan-Ergebnissen gefüllt, der Stack wechselt zu `"selection"`.
4. „Import"-Button wird erst nach erfolgreichem Prescan sensitiv.

### Frontend: Import-Flow

1. User klickt „Import". Stack → `"progress"`.
2. `gio::spawn_blocking` ruft `merge_stats(…, Some(&on_progress))` auf.
   Der Callback sendet `glib::idle_add_local_once` je N Tracks (Batching
   alle 50 Tracks, um Idle-Flood zu vermeiden).
3. Parallel (sequenziell im selben Blocking-Thread): `merge_playlists`.
4. Ergebnis → Idle → Stack → `"complete"`.

### Frontend: Complete-State

- Checkmark-Icon (`emblem-ok-symbolic`, 48px, in Kreis).
- „Import complete" Heading.
- „1,685 of 1,603 Rhythmbox entries matched your library" — Subtitle.
- Tabelle als `adw::PreferencesGroup` mit `adw::ActionRow`s:
  - Ratings: N imported
  - Play counts: N raised
  - Date added · Last played: N · N restored
  - Playlists: N created
- `AdwExpanderRow` „N entries skipped" mit Breakdown-Rows (außerhalb
  Library / fehlt auf Disk / Podcasts & Radio).
- Footer: „Undo import" (`gtk4::Button` flat, destructive), „Done"
  (suggested-action).

### Strings

Neue/aktualisierte Konstanten in `strings_rhythmbox.rs`:

- `RHYTHMBOX_PRESCAN_SCANNING` — „Scanning Rhythmbox library…"
- `RHYTHMBOX_LIBRARY_FOUND` — „Rhythmbox library found"
- `RHYTHMBOX_IMPORT_BODY_RICH` — „Choose what to copy…can undo…"
- `RHYTHMBOX_PLAY_COUNTS_AND_LAST_PLAYED` — „Play counts & last played"
- `RHYTHMBOX_IMPORT_COMPLETE_HEADING` — „Import complete"
- `RHYTHMBOX_ENTRIES_SKIPPED` — „{count} entries skipped"
- `RHYTHMBOX_SKIP_OUTSIDE_LIBRARY` — „Files outside your library folder"
- `RHYTHMBOX_SKIP_MISSING_ON_DISK` — „Files no longer on disk"
- `RHYTHMBOX_SKIP_NON_SONG` — „Podcasts & radio streams"
- `RHYTHMBOX_UNDO_IMPORT` — „Undo import"
- `RHYTHMBOX_IMPORTING` — „Importing from Rhythmbox…"
- Formatierungs-Funktionen für die dynamischen Zahlen.

Bestehende Strings (`RHYTHMBOX_IMPORT_RATINGS`, etc.) bleiben erhalten
und werden als Titles der SwitchRows wiederverwendet.

## Kompatibilität

- `RhythmboxImportChoices` ändert sich (2 Felder → 1 zusammengelegtes).
  Alle Call-Sites in `preference_rhythmbox.rs` und `first_run.rs` werden
  angepasst. `first_run.rs` bleibt funktional unverändert (nutzt weiter
  die SwitchRow-basierte Minimalansicht, kein Prescan).
- `merge_stats()` Signatur ändert sich (neuer Callback + Rückgabewert).
  Bestehende Tests werden angepasst (`None` als Callback, Rollback
  ignoriert).

## Tests

- **Unit (reprise-core):** `prescan_rhythmdb` mit Fixture-XML + In-Memory-DB.
  Undo-Roundtrip: merge → undo → Werte stimmen mit Ausgangszustand überein.
  Progress-Callback zählt korrekt.
- **Unit (reprise-gnome):** `build_import_dialog` produziert die drei
  Stack-Children. SwitchRow-Defaults stimmen. Prescan-Ergebnis füllt Labels.
- **Smoke (headless):** `REPRISE_SMOKE_RHYTHMDB_IMPORT` öffnet den Dialog,
  führt den Import-Flow programmatisch durch und prüft die Tracing-Logs.
