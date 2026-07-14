# Rhythmbox-Statistikimport und Wiedergabenzähler-Spalte — Designspezifikation

## Ziel

Reprise bietet unter **Einstellungen → Bibliothek** erneut einen bewusst
ausgelösten, read-only Rhythmbox-Import an. Neben dem bestehenden Spaltenlayout
kann der Nutzer Bewertungen, Wiedergabezähler, das ursprüngliche
Hinzufügedatum und die letzte Wiedergabe aus `rhythmdb.xml` sowie statische Playlisten aus
`playlists.xml` übernehmen. Die Tracktabelle erhält außerdem eine optionale
Spalte „Wiedergaben“.

## Umfang

- eine dauerhafte Aktion „Aus Rhythmbox importieren…“ in den
  Bibliothekseinstellungen;
- explizite Auswahl von Spaltenlayout, Bewertungen, Wiedergabezählern,
  Hinzufügedatum, letzter Wiedergabe und statischen Playlisten;
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
- Rhythmbox' positives `first-seen` wird als `added_at` übernommen, wenn es
  älter als Reprises positiver Wert ist; bei einem fehlenden Reprise-Wert wird
  der gültige Rhythmbox-Wert verwendet. Ein Import macht einen Titel niemals
  künstlich neuer.
- Rhythmbox' positives `last-played` wird als `last_played_at` übernommen,
  wenn es neuer als Reprises Wert ist. Der Import setzt eine aktuellere lokale
  Wiedergabe niemals zurück.
- Nicht gefundene gültige Pfade werden in der Zusammenfassung gezählt.
  Nicht-Datei-URIs, ungültige Werte und Nicht-Song-Einträge werden sicher
  ignoriert.
- Ein XML-/Lesefehler tritt vor der SQLite-Transaktion auf; es entstehen keine
  Teiländerungen.

Statische Playlisten behalten ihre Reihenfolge. Einträge werden ebenfalls über
dekodierte `file://`-Pfade gematcht. Eine gleichnamige Reprise-Playlist wird
ergänzt; vorhandene oder innerhalb der Rhythmbox-Playlist wiederholte Titel
werden nicht erneut eingefügt. Eine neue Playlist wird nur angelegt, wenn
mindestens ein Track gematcht wurde. Smart-/automatische Rhythmbox-Playlisten
werden nicht in eine andere Semantik übersetzt und daher übersprungen.

## Architektur

`reprise-core::library::rhythmbox_import` enthält den plattformneutralen,
streamenden XML-Parser und die transaktionale Merge-Logik. Der Parser liefert
plain-data `RhythmboxTrackStats`; die Merge-Funktion konsumiert diese zusammen
mit `RhythmboxImportChoices` und liefert eine `RhythmboxImportSummary`.
`quick-xml` dekodiert XML sicher, `url` wandelt ausschließlich `file://`-URIs
in lokale Pfade um.

Der gleiche Core-Baustein liest `playlists.xml` als geordnete
`RhythmboxPlaylist`-Werte. Der Import verwendet die bestehenden atomaren
Playlist-Helfer: neue Namen werden mit Tracks erstellt, vorhandene Namen über
den duplicate-sicheren Membership-Pfad ergänzt.

`preference_rhythmbox.rs` baut die Adwaita-Auswahl, liest die XML-Datei in
`gio::spawn_blocking`, führt anschließend den kurzen SQLite-Merge auf dem
Main-Thread aus und zeigt eine Ergebnis- oder Fehlermeldung. Der bekannte Pfad
ist `$XDG_DATA_HOME/rhythmbox/rhythmdb.xml`; `playlists.xml` liegt daneben. Ein
isolierter Smoke-Hook darf beide Pfade explizit überschreiben.

`column_layout.rs` erweitert das persistente Spaltenmodell um `PlayCount`.
Bestehende gespeicherte Layouts werden durch die vorhandene Normalisierung
verlustfrei um die neue, ausgeblendete Spalte ergänzt. Die SQL-Sortier-
Whitelist akzeptiert `play_count`.

## Fehlerbehandlung

- Fehlende oder unlesbare `rhythmdb.xml`: verständlicher Dialog, keine Änderung.
- Eine fehlende `playlists.xml` verhindert nur den ausgewählten Playlistimport;
  Statistikimport und Spaltenlayout bleiben nutzbar.
- Defekte `rhythmdb.xml`: kompletter Abbruch vor dem Statistik-Merge.
- Defekte `playlists.xml`: Warnung; ein bereits erfolgreicher Statistikimport
  bleibt erhalten.
- Einzelne defekte Einträge: sicher überspringen.
- SQLite-Fehler: Transaktion zurückrollen und Fehlerdialog zeigen.
- Ein fehlendes Rhythmbox-GSettings-Schema verhindert nur den optionalen
  Spaltenlayoutteil, nicht den Statistikimport aus XML.

## Tests und Verifikation

- Core-RED/GREEN-Tests mit ausschließlich temporärer XML- und SQLite-Fixture:
  URI-Dekodierung, Songfilter, Konfliktregeln, Wiederholbarkeit und atomarer
  Fehlerpfad;
- Core-Tests für statische Playlist-Reihenfolge, URI-Dekodierung,
  Smart-Playlist-Ausschluss, gleichnamiges Merge und Wiederholbarkeit;
- Query- und Spaltenlayouttests für `play_count`, Legacy-Layoutmigration,
  Standard-unsichtbarkeit und Rhythmbox-Tokenmapping;
- isolierter GTK-Test für die dauerhafte Preferences-Aktion und ihre vier
  Auswahlmöglichkeiten;
- isolierter Anwendungssmoke mit Scratch-XDG, Scratch-DB und Scratch-
  `rhythmdb.xml`;
- vollständige fmt-, clippy-, Workspace-Test-, Audit-, Core-Purity-, gettext-
  und Dateigrößen-Gates.

## Explizit nicht Teil dieser Änderung

- kein Import von Audio-Tags wie Titel, Interpret, Album oder Genre;
- kein Import von Smart-Playlists oder Rhythmbox-internen IDs;
- kein automatischer Start, keine Hintergrundüberwachung und kein Schreiben
  nach Rhythmbox;
- kein Zugriff auf echte Rhythmbox-, Reprise- oder Musikdaten während QA;
- keine Addition von Wiedergabezählern und kein Überschreiben vorhandener
  Reprise-Bewertungen.
