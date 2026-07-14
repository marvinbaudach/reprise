# Rhythmbox-Statistikimport und Wiedergabenzähler-Spalte — Designspezifikation

## Ziel

Reprise bietet unter **Einstellungen → Bibliothek** erneut einen bewusst
ausgelösten, read-only Rhythmbox-Import an. Neben dem bestehenden Spaltenlayout
kann der Nutzer Bewertungen und Wiedergabezähler aus `rhythmdb.xml` übernehmen.
Die Tracktabelle erhält außerdem eine optionale Spalte „Wiedergaben“.

## Umfang

- eine dauerhafte Aktion „Aus Rhythmbox importieren…“ in den
  Bibliothekseinstellungen;
- explizite Auswahl von Spaltenlayout, Bewertungen und Wiedergabezählern;
- standardmäßig ausgewählte Statistikoptionen, ohne automatischen Import;
- exaktes Matching lokaler Titel über den dekodierten `file://`-Pfad;
- eine optionale, standardmäßig ausgeblendete und sortierbare Spalte
  „Wiedergaben“;
- vollständige englische/deutsche gettext-Texte und isolierte Tests.

## Daten- und Konfliktregeln

Rhythmbox bleibt vollständig read-only. Reprise liest ausschließlich die vom
Nutzer bestätigte `rhythmdb.xml` und schreibt nur in die eigene SQLite-Datenbank.
Audiodateien und deren Tags werden nicht verändert.

Der Import ist wiederholbar und konservativ:

- Rhythmbox-Bewertungen von 1 bis 5 werden nur übernommen, wenn Reprises
  Bewertung noch 0 ist. Vorhandene Reprise-Bewertungen gewinnen.
- Der Wiedergabezähler wird auf `max(Reprise, Rhythmbox)` gesetzt und daher nie
  verringert oder bei wiederholtem Import addiert.
- Nicht gefundene Pfade, Nicht-Datei-URIs, ungültige Werte und Nicht-Song-
  Einträge werden gezählt und übersprungen.
- Ein XML-/Lesefehler tritt vor der SQLite-Transaktion auf; es entstehen keine
  Teiländerungen.

## Architektur

`reprise-core::library::rhythmbox_import` enthält den plattformneutralen,
streamenden XML-Parser und die transaktionale Merge-Logik. Der Parser liefert
plain-data `RhythmboxTrackStats`; die Merge-Funktion konsumiert diese zusammen
mit `RhythmboxImportChoices` und liefert eine `RhythmboxImportSummary`.
`quick-xml` dekodiert XML sicher, `url` wandelt ausschließlich `file://`-URIs
in lokale Pfade um.

`preference_rhythmbox.rs` baut die Adwaita-Auswahl, liest die XML-Datei in
`gio::spawn_blocking`, führt anschließend den kurzen SQLite-Merge auf dem
Main-Thread aus und zeigt eine Ergebnis- oder Fehlermeldung. Der bekannte Pfad
ist `$XDG_DATA_HOME/rhythmbox/rhythmdb.xml`; ein isolierter Smoke-Hook darf ihn
explizit überschreiben.

`column_layout.rs` erweitert das persistente Spaltenmodell um `PlayCount`.
Bestehende gespeicherte Layouts werden durch die vorhandene Normalisierung
verlustfrei um die neue, ausgeblendete Spalte ergänzt. Die SQL-Sortier-
Whitelist akzeptiert `play_count`.

## Fehlerbehandlung

- Fehlende oder unlesbare `rhythmdb.xml`: verständlicher Dialog, keine Änderung.
- Defektes XML: kompletter Abbruch vor dem DB-Merge.
- Einzelne defekte Einträge: überspringen und in der Zusammenfassung melden.
- SQLite-Fehler: Transaktion zurückrollen und Fehlerdialog zeigen.
- Ein fehlendes Rhythmbox-GSettings-Schema verhindert nur den optionalen
  Spaltenlayoutteil, nicht den Statistikimport aus XML.

## Tests und Verifikation

- Core-RED/GREEN-Tests mit ausschließlich temporärer XML- und SQLite-Fixture:
  URI-Dekodierung, Songfilter, Konfliktregeln, Wiederholbarkeit und atomarer
  Fehlerpfad;
- Query- und Spaltenlayouttests für `play_count`, Legacy-Layoutmigration,
  Standard-unsichtbarkeit und Rhythmbox-Tokenmapping;
- isolierter GTK-Test für die dauerhafte Preferences-Aktion und ihre drei
  Auswahlmöglichkeiten;
- isolierter Anwendungssmoke mit Scratch-XDG, Scratch-DB und Scratch-
  `rhythmdb.xml`;
- vollständige fmt-, clippy-, Workspace-Test-, Audit-, Core-Purity-, gettext-
  und Dateigrößen-Gates.

## Explizit nicht Teil dieser Änderung

- kein Import von Audio-Tags wie Titel, Interpret, Album oder Genre;
- kein Import von Playlists, Last-played-Zeit oder Rhythmbox-internen IDs;
- kein automatischer Start, keine Hintergrundüberwachung und kein Schreiben
  nach Rhythmbox;
- kein Zugriff auf echte Rhythmbox-, Reprise- oder Musikdaten während QA;
- keine Addition von Wiedergabezählern und kein Überschreiben vorhandener
  Reprise-Bewertungen.
