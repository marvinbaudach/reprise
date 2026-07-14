# Rhythmbox-Statistikimport und Wiedergabenzähler-Spalte — Implementierungsplan

Design: `docs/superpowers/specs/2026-07-14-rhythmbox-stats-import-design.md`

## Globale Vorgaben

- Für jede Aufgabe zuerst einen fehlschlagenden Test ausführen, dann minimal
  implementieren und denselben Test grün sehen.
- Code, Kommentare, UI-Texte und Commits auf Englisch; interne Dokumente auf
  Deutsch; gettext vollständig Englisch/Deutsch.
- Niemals echte Musik-, Rhythmbox- oder Reprise-Daten in Tests/QA verwenden.
- `reprise-core` bleibt frei von GTK/libadwaita/GStreamer/zbus; jede wesentlich
  geänderte Datei bleibt unter 800 Zeilen.
- Vor jedem Commit: fmt, striktes Clippy, Workspace-Tests und Audit. Abschluss:
  Core-Purity, gettext, isolierte GTK-/Anwendungstests und adversarial review.

## Aufgabe 1 — Read-only RhythmDB-Parser und konservativer Statistik-Merge

**Dateien:** `crates/reprise-core/Cargo.toml`, `Cargo.lock`,
`crates/reprise-core/src/library/mod.rs`, neu
`crates/reprise-core/src/library/rhythmbox_import.rs`, Design und Plan.

**Schnittstellen:**

```rust
pub struct RhythmboxTrackStats {
    pub path: PathBuf,
    pub rating: Option<i32>,
    pub play_count: Option<i64>,
    pub added_at: Option<i64>,
    pub last_played_at: Option<i64>,
}
pub struct RhythmboxImportChoices {
    pub ratings: bool,
    pub play_counts: bool,
    pub added_at: bool,
    pub last_played_at: bool,
}
pub struct RhythmboxImportSummary {
    pub parsed: usize,
    pub matched: usize,
    pub ratings_imported: usize,
    pub play_counts_raised: usize,
    pub dates_imported: usize,
    pub last_played_imported: usize,
    pub skipped: usize,
}
pub fn parse_rhythmdb(path: &Path) -> Result<Vec<RhythmboxTrackStats>, RhythmboxImportError>;
pub fn merge_stats(
    conn: &mut Connection,
    tracks: &[RhythmboxTrackStats],
    choices: RhythmboxImportChoices,
) -> Result<RhythmboxImportSummary, RhythmboxImportError>;
```

1. RED: temporäre XML-/DB-Tests für dekodierte `file://`-Pfade, nur `song`,
   Bewertung/Count, ungültige Einträge, bestehende Reprise-Bewertung,
   `max`-Count und wiederholten Import anlegen und fehlschlagen sehen.
2. `quick-xml` ergänzen; streamenden Parser und transaktionalen Merge minimal
   implementieren. Defektes XML muss ohne DB-Änderung fehlschlagen.
3. Fokustests grün; vollständige Gates, Core-Purity, Dateigröße und Diff-Review.
4. Commit: `feat: import Rhythmbox ratings and play counts`.

Erwartung: mindestens 6 neue Core-Tests.

## Aufgabe 2 — Optionale sortierbare Wiedergabenzähler-Spalte

**Dateien:** `crates/reprise-core/src/queries/clauses.rs` und Querytests,
`crates/reprise-gnome/src/ui/column_layout.rs`,
`crates/reprise-gnome/src/ui/column_layout_editor.rs`,
`crates/reprise-gnome/src/ui/track_list_columns.rs`,
`crates/reprise-gnome/src/ui/strings.rs`, `po/de.po`, `po/reprise.pot`.

**Schnittstelle:** `ColumnId::PlayCount` serialisiert als `play-count`, sortiert
über `play_count`, Breite 90, rechtsbündige numerische Zelle, standardmäßig
ausgeblendet und nach Rating angeordnet.

1. RED: Tests für neue ID, Legacy-Normalisierung, Standard-unsichtbarkeit,
   Rhythmbox-Token, Sortier-Whitelist und Registry-Vollständigkeit ergänzen.
2. Fokustests ausführen und erwartete Compile-/Assertion-Fehler sehen.
3. Spaltenmodell, String, Zelle und SQL-Whitelist minimal implementieren.
4. Gettext aktualisieren; Fokustests plus isolierten GTK-Registrytest ausführen.
5. Vollständige Gates, Core-Purity, Dateigröße und Diff-Review.
6. Commit: `feat: add play-count library column`.

Erwartung: mindestens 5 neue/erweiterte Assertions.

## Aufgabe 3 — Statische Rhythmbox-Playlisten lesen und idempotent importieren

**Dateien:** `crates/reprise-core/src/library/rhythmbox_import.rs`.

**Schnittstellen:**

```rust
pub struct RhythmboxPlaylist { pub name: String, pub paths: Vec<PathBuf> }
pub struct RhythmboxPlaylistSummary {
    pub parsed: usize,
    pub imported: usize,
    pub tracks_added: usize,
    pub skipped_tracks: usize,
}
pub fn parse_playlists(path: &Path) -> Result<Vec<RhythmboxPlaylist>, RhythmboxImportError>;
pub fn merge_playlists(
    conn: &mut Connection,
    playlists: &[RhythmboxPlaylist],
) -> Result<RhythmboxPlaylistSummary, RhythmboxImportError>;
```

1. RED: temporäre XML-/DB-Tests verlangen nur statische Playlisten, erhaltene
   Reihenfolge, dekodierte Datei-URIs, Pfadmatching, gleichnamiges Merge,
   Duplikatfreiheit und Wiederholbarkeit.
2. Streamenden Parser mit `quick-xml` sowie Merge über `create_with_tracks` und
   `playlist_membership::add_unique_tracks` minimal implementieren.
3. Fokustests grün; vollständige Gates, Core-Purity, Dateigröße und Diff-Review.
4. Commit: `feat: import Rhythmbox playlists`.

Erwartung: mindestens 4 neue Core-Tests.

## Aufgabe 4 — Dauerhafter Import unter Einstellungen → Bibliothek

**Dateien:** neu `crates/reprise-gnome/src/ui/preference_rhythmbox.rs`,
`crates/reprise-gnome/src/ui/mod.rs`,
`crates/reprise-gnome/src/ui/preference_library.rs`,
`crates/reprise-gnome/src/ui/preferences.rs`,
`crates/reprise-gnome/src/ui/strings.rs`, `po/de.po`, `po/reprise.pot`,
`docs/agent-workflow/MANUAL-QA.md`.

**Schnittstellen:**

```rust
pub(super) fn add_rhythmbox_import_row(
    context: &Rc<PreferencesContext>,
    group: &adw::PreferencesGroup,
);
fn default_rhythmdb_path() -> PathBuf;
```

1. RED: Policy-/Displaytest verlangt eine immer sichtbare Bibliothekszeile und
   einen expliziten Dialog mit `Column layout`, `Ratings`, `Play counts` und
   `Playlists`; keine Aktion startet vor der Bestätigung.
2. Auswahloberfläche bauen. Statistik- und Playlistoptionen sind an,
   Spaltenlayout aus.
3. XML über `gio::spawn_blocking` lesen, danach Core-Merge und optional den
   bestehenden read-only GSettings-Spaltenimport sowie optional den statischen
   `playlists.xml`-Import ausführen; Trackliste und Sidebar neu laden.
4. Ergebnis nennt matched/imported/skipped; Fehler bleiben ohne Teil-UI-Zustand.
5. Isolierten GTK-Test und Scratch-Anwendungssmoke mit explizitem Fixture-Pfad
   ausführen; keine echten Nutzerdaten lesen.
6. Vollständige Gates, Core-Purity, gettext, Dateigröße und Whole-feature-Review.
7. Commit: `feat: expose Rhythmbox import in preferences`.

Erwartung: mindestens 1 Policytest, 1 Displaytest und 1 isolierter Smoke.

## Aufgabe 5 — Ursprüngliches Hinzufügedatum übernehmen

**Dateien:** `crates/reprise-core/src/library/rhythmbox_import.rs`,
`crates/reprise-gnome/src/ui/preference_rhythmbox.rs`,
`crates/reprise-gnome/src/ui/strings.rs`, `po/de.po`, `po/reprise.pot`, Design
und Manual QA.

1. RED: Parser- und Merge-Tests verlangen Rhythmbox' positives `first-seen`,
   die explizite Auswahloption, den älteren positiven Zeitstempel und
   idempotentes Verhalten; der GTK-Dialogtest verlangt fünf Optionen.
2. `RhythmboxTrackStats`, `RhythmboxImportChoices` und Summary um `added_at`
   erweitern. Der Merge setzt den älteren positiven Wert und macht einen Titel
   nie neuer.
3. „Date added“ standardmäßig ausgewählt im Importdialog ergänzen, Ergebnis-
   und Beschreibungstexte sowie vollständiges deutsches gettext aktualisieren.
4. Fokustests, isolierter GTK-Test und Scratch-App-Smoke mit `first-seen`
   ausführen; danach vollständige Gates, Core-Purity, Dateigröße und Review.
5. Commit: `feat: import Rhythmbox date added`.

## Aufgabe 6 — Letzte Wiedergabe übernehmen

**Dateien:** `crates/reprise-core/src/library/rhythmbox_import.rs`, neu
`crates/reprise-core/src/library/rhythmbox_playlist_import_tests.rs`,
`crates/reprise-gnome/src/ui/preference_rhythmbox.rs`,
`crates/reprise-gnome/src/ui/strings.rs`, `po/de.po`, `po/reprise.pot`, Design
und Manual QA.

1. RED: Parser- und Merge-Tests verlangen Rhythmbox' positives `last-played`,
   den neueren positiven Zeitstempel, idempotentes Verhalten und eine sechste
   explizite GTK-Auswahloption.
2. Bestehende Core-Tests in ein Geschwistermodul extrahieren, damit die
   Produktionsdatei und Testdatei jeweils unter 800 Zeilen bleiben.
3. `last_played_at` streamend parsen und transaktional mit `max` mergen; ein
   fehlender Reprise-Wert wird ergänzt, ein neuerer lokaler Wert bleibt.
4. „Last played“ standardmäßig ausgewählt im Importdialog ergänzen und
   Ergebnis-, Beschreibungs-, Manual-QA- sowie gettext-Texte aktualisieren.
5. Fokustests, isolierten GTK-Test und Scratch-App-Smoke ausführen; danach
   vollständige Gates, Core-Purity, Dateigröße und Review.
6. Commit: `feat: import Rhythmbox last played`.

## Aufgabe 7 — Importaktion nur bei gefundenen Rhythmbox-Daten zeigen

**Dateien:** `crates/reprise-gnome/src/ui/preference_rhythmbox.rs`, Design und
Manual QA.

1. RED: Ein Policytest verlangt, dass nur eine reguläre `rhythmdb.xml` als
   gefundene Rhythmbox-Daten gilt; ein fehlender Pfad und ein Verzeichnis gelten
   nicht als verfügbar.
2. Den bereits aufgelösten Standard- oder Smoke-Pfad vor dem Erzeugen der
   Preferences-Zeile prüfen und bei fehlender Datendatei früh zurückkehren.
3. Fokustest sowie isolierte Settings-Smokes mit und ohne Scratch-Fixture
   ausführen; danach vollständige Gates, Dateigröße und Review.
4. Commit: `fix: hide unavailable Rhythmbox import`.

## Abschluss

Ledger um alle sieben Commits ergänzen. Branch sauber und ungepusht lassen;
Integration nach `main` erfolgt erst nach separater Freigabe und Rebase auf den
dann aktuellen `main`.
