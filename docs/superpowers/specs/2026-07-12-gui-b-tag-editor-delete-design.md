# GUI-B — Batch-Tag-Editor und sicheres Löschen

**Datum:** 2026-07-12  
**Status:** Aus dem freigegebenen Master-Design und der ausdrücklichen Nutzeranweisung
„bis zum bitteren Schluss“ autonom fortgeführt.

## Ziel

Reprise erhält einen Tag-Editor für einen oder mehrere ausgewählte Titel sowie
zwei klar getrennte Löschpfade:

1. **Aus Bibliothek entfernen** — nur Datenbank/Queue/Playlists, niemals die Datei.
2. **In den Papierkorb verschieben** — nur nach expliziter Bestätigung; erfolgreich
   verschobene Dateien werden anschließend aus der Datenbank entfernt.

Der wichtigste Datenintegritätsgrundsatz ist unverhandelbar: Bei einer
Mehrfachauswahl werden ausschließlich Felder geschrieben, die der Nutzer im
Dialog tatsächlich geändert hat. Unterschiedliche Ausgangswerte werden als
„(multiple values)“ angezeigt und niemals stillschweigend vereinheitlicht.

## Umfang

Bearbeitbare klassische Metadaten:

- Title
- Artist
- Album
- Album artist
- Year
- Track number
- Genre

Bewertungen, Wiedergabezähler, Cover, MusicBrainz-IDs und ReplayGain-Tags bleiben
unangetastet. Änderungen an Musikdateien geschehen ausschließlich nach „Apply“.

## Architektur

### Core: Patch-Modell

`reprise_core::library::tag_edit` definiert ein explizites Patch-Modell:

```rust
pub struct TagPatch {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<Option<u32>>,
    pub track_no: Option<Option<u32>>,
    pub genre: Option<String>,
}
```

- äußeres `None`: Feld nicht angefasst → vorhandenen Wert exakt erhalten;
- `Some(value)`: String setzen (leerer String löscht das Feld);
- Zahlen: `Some(None)` löscht, `Some(Some(n))` setzt.

`MixedValue<T>` beschreibt die Auswahlansicht (`Uniform(T)` oder `Mixed`). Die
Zusammenfassung kommt aus den DB-Zeilen der ausgewählten IDs; Dateizugriffe sind
dafür nicht nötig.

### Core: Datei schreiben und DB synchronisieren

Die bestehende Datei wird mit lofty gelesen. Nur Patch-Felder werden am
bestehenden Tag geändert; Bilder und unbekannte Items bleiben erhalten. Fehlt ein
Tag, wird der primäre Tag-Typ der Datei angelegt. Nach erfolgreichem Speichern
wird genau diese Datei über den Scanner erneut eingelesen, wodurch DB-Metadaten,
mtime/size/device/inode und Importfehler konsistent aktualisiert werden, während
Rating/Playcount erhalten bleiben.

Ein Batch ist absichtlich **nicht dateisystem-atomar** — mehrere Dateien können
nicht gemeinsam transaktional geschrieben werden. Das Ergebnis ist deshalb pro
Track typisiert: Erfolge werden sichtbar, Fehler werden gesammelt und gemeldet;
ein Fehler verhindert nicht die übrigen explizit angeforderten Änderungen.

Die gesamte Arbeit läuft auf einem dedizierten Worker mit eigener DB-Verbindung,
nie auf dem GTK-Main-Thread.

### Core: Entfernen

Ein neuer transaktionaler DB-Primitiv entfernt beliebige Track-IDs und kompaktiert
alle betroffenen Playlist-Positionen in derselben Transaktion. Er ist die
allgemeine Form des bestehenden Missing-only-Primitivs; dessen `missing = 1`-
Schutz bleibt für den alten Pfad erhalten.

Papierkorb-Operationen verwenden das plattformübergreifende `trash`-Crate. Jede
Datei wird einzeln verschoben. Nur erfolgreiche IDs gelangen danach in den
DB-Remove-Batch. Es gibt keinen permanenten Delete-Fallback.

### GTK-Frontend

Ein `AdwDialog` zeigt sieben `AdwEntryRow`s. Bei gemischten Werten bleibt das
Eingabefeld leer und der Hinweis lautet „(multiple values)“. Dirty-Flags werden
erst nach der Initialisierung verbunden. Der Apply-Button ist ohne Änderung
deaktiviert.

Das Track-Kontextmenü erhält:

- „Edit tags…“
- „Remove from library…“
- „Move to Trash…“

`Delete` öffnet denselben Bestätigungsdialog. Der Dialog nennt die Anzahl und
beschreibt unmissverständlich, ob Dateien unangetastet bleiben oder in den
Papierkorb wandern. Nach Erfolg werden Liste, Sidebar und Queue aktualisiert.

## Fehlerbehandlung

- Unlesbare/nicht beschreibbare Datei: pro Datei Fehler, übriger Batch läuft.
- Ungültiges Jahr/Track-Nr.: Dialog bleibt offen, Inline-Fehler; kein Write.
- Papierkorbfehler: Datei und DB-Zeile bleiben bestehen; Sammeltoast nennt Fehlerzahl.
- DB-Fehler nach erfolgreichem Papierkorb: Datei ist bereits im Papierkorb; Fehler
  wird deutlich gemeldet und ein Rescan bereinigt die DB später. Kein Panic.
- Aktuell abgespielte/queued entfernte IDs werden aus der Queue entfernt; ein
  bereits von GStreamer geöffnetes File darf zu Ende spielen, wird aber nicht erneut
  angesteuert.

## Sicherheit

- Tests arbeiten ausschließlich auf Kopien von `tests/fixtures/sine.flac` in
  `tempfile::tempdir()`.
- Kein Test benutzt den echten Desktop-Papierkorb; Trash wird über eine injizierte
  Funktion getestet.
- Kein automatischer Dateischreibzugriff, kein permanentes Löschen.
- Alle Smokes verwenden vollständig isolierte XDG-/D-Bus-/Xvfb-Umgebungen.

## Tests

- Mixed/Uniform-Zusammenfassung und Patch-Semantik.
- Einzelfile: nur geänderte Felder; unberührte Felder und Cover/Custom Item bleiben.
- Zahl setzen/löschen; leerer String löscht.
- Batch-Teilerfolg; DB folgt nur erfolgreichen Writes; Rating/Playcount bleiben.
- Entfernen kompaktiert Playlistpositionen und gibt exakt entfernte IDs zurück.
- Trash injiziert: nur Erfolge werden DB-seitig entfernt; kein echter Trash im Test.
- GTK-Smokes: Edit-Aktion ohne Änderung schreibt nichts; DB-only Remove lässt
  Fixture bestehen; Trash-Smoke wird aus Sicherheitsgründen nur mit einer
  eigens erzeugten Scratch-Datei gefahren.

## Explizit nicht in GUI-B

- Rating in Dateitags schreiben.
- Cover austauschen oder entfernen.
- Undo über App-Neustarts hinweg.
- Permanentes Löschen ohne Papierkorb.
- Automatische Tag-Korrektur oder Online-Metadaten.

