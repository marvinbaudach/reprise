# GUI-C: Browse-Bar und Rhythmbox-Spaltenimport — Design

## Ziel

Reprise erhält eine kompakte Bibliotheksnavigation nach Genre, Interpret und
Album sowie einen expliziten, rein lesenden Import des sichtbaren
Rhythmbox-Spaltenlayouts. Beide Funktionen bleiben native GTK4/libadwaita-
Oberfläche; alle SQL- und Zustandsregeln liegen im dependency-reinen Core.

## Umfang

### Browse-Bar

Oberhalb der Trackliste steht in der Quelle **Library** eine horizontale Bar
mit drei Dropdowns: Genre, Artist, Album. Jedes beginnt mit `All genres`,
`All artists` beziehungsweise `All albums`.

Die Auswahl ist kaskadierend:

- Genre bestimmt die angebotenen Artists und Albums.
- Artist bestimmt zusätzlich die angebotenen Albums.
- Ein Genre-Wechsel setzt Artist und Album zurück; ein Artist-Wechsel setzt
  Album zurück. Dadurch kann kein unsichtbarer, widersprüchlicher Zustand
  bestehen bleiben.
- Leere Tagwerte werden als `Unknown genre`, `Unknown artist` und
  `Unknown album` angeboten, intern aber weiterhin als exakter leerer Wert
  behandelt.

Facetten sind exakte, gebundene SQL-Vergleiche. Die bestehende freie Suche
wird mit den Facetten per `AND` kombiniert. Sortierung, Fensterung,
Aktivierungsqueue und Statuszeile beziehen sich immer auf dieselbe gefilterte
Menge. Beim Wechsel aus Library wird die Bar verborgen; die Auswahl bleibt
für die Rückkehr in derselben Sitzung erhalten und beeinflusst keine andere
Quelle.

Die Bar ist Bestandteil des TrackList-Widgets, nicht von `window.rs`. So bleibt
die bereits grenznahe Composition Root unter 800 Zeilen. Bei schmalem Fenster
dürfen die Dropdowns gleichmäßig schrumpfen; abgeschnittene Werte erhalten
Tooltips über GTKs normale Dropdowndarstellung. Ein zusätzlicher großer
Rhythmbox-Browser-Pane mit drei permanenten Listen ist ausdrücklich nicht Teil
dieser Etappe.

### Spaltenlayout

Reprise kennt danach diese Spalten-IDs:

`cover`, `title`, `track-number`, `artist`, `album`, `genre`, `year`,
`duration`, `rating`.

Cover und Title sind feste Reprise-Leitspalten und bleiben sichtbar. Das
Standardlayout entspricht dem bisherigen UI; die neuen Spalten Track number
und Genre sind standardmäßig verborgen.

Der Hauptmenüpunkt `Import Rhythmbox column layout` liest explizit und nur:

- Schema: `org.gnome.rhythmbox.sources`
- Schlüssel: `visible-columns` (`as`)

Zuordnung:

| Rhythmbox | Reprise |
|---|---|
| `track-number` | `track-number` |
| `artist` | `artist` |
| `album` | `album` |
| `genre` | `genre` |
| `duration` | `duration` |
| `date` | `year` |
| `rating` | `rating` |

`title` bleibt unabhängig von Rhythmbox sichtbar. Unbekannte Werte wie
`post-time` werden ignoriert, Duplikate werden stabil entfernt. Bekannte
importierte Spalten werden nach Cover/Title in Rhythmbox-Reihenfolge sichtbar;
nicht importierte Reprise-Spalten bleiben verborgen und werden in stabiler
Default-Reihenfolge hinten angefügt. Das Ergebnis wird unter einem eigenen
Reprise-Setting gespeichert und sofort angewandt.

Fehlt das Schema oder kann der Schlüssel nicht gelesen werden, bleibt das
bisherige Layout unverändert und ein Toast erklärt den Fehler. Reprise schreibt
niemals in dconf/GSettings von Rhythmbox. Der Import ist jederzeit wiederholbar
und überschreibt dann bewusst nur Reprises gespeichertes Spaltenlayout.

## Architektur

### Core

`queries::BrowseFilter` ist ein kleiner unveränderlicher Wert mit optionalem
Genre/Artist/Album. Library-Window, Count, IDs und Stats erhalten denselben
Filter. Ein gemeinsamer Clause-Builder erzeugt ausschließlich gebundene
Parameter; kein Tagwert gelangt in SQL-Text.

`queries::query_browse_values` liefert sortierte `(value, count)`-Facetten.
Artist wird durch Genre begrenzt, Album durch Genre+Artist. Leere Werte bleiben
als leere Strings typisiert; nur das Frontend erzeugt die `Unknown …`-Labels.

Das Core-Settings-Modul speichert/liest den kanonischen Layout-String. Die
Rhythmbox-Token-Zuordnung und GSettings-Abfrage bleiben Linux/GNOME-Frontend-
Belang und gelangen nicht in `reprise-core`.

### GTK-Frontend

`ui/browse_bar.rs` besitzt Dropdownmodelle, Selektionen und
Generation/Update-Guard gegen Callback-Reentry beim Neubefüllen. Es meldet
einen plain `BrowseFilter` an TrackList; DB-Abfragen bleiben kurz und synchron
(nur DISTINCT/GROUP BY über lokale SQLite-Daten, keine Datei- oder Netzarbeit).

`ui/column_layout.rs` definiert die pure Token-Zuordnung, Serialisierung und
GTK-Anwendung. Es hält die `ColumnViewColumn`-Handles in einem Registry-Wert,
statt Widgets später über fragile Downcasts/Titelstrings wiederzusuchen.

`ui/primary_menu.rs` installiert die Import-Aktion. `window.rs` verschiebt den
bestehenden `primary_menu::install`-Aufruf lediglich hinter die TrackList-
Konstruktion; netto wächst die Datei nicht.

## Fehlerbehandlung

- SQL-Fehler beim Facettenladen: Warnung, betroffener Dropdown fällt auf
  `All …` zurück; die Trackliste bleibt benutzbar.
- Gespeichertes Layout beschädigt/unbekannt: kanonischer Default, Warnung,
  kein Panic.
- Rhythmbox-Schema fehlt oder GSettings-Lesen scheitert: kein Layoutwechsel,
  erklärender Toast.
- Persistieren des Imports scheitert: aktuelles GTK-Layout bleibt unverändert;
  Fehler-Toast, damit der Zustand nicht fälschlich als dauerhaft erscheint.

## Tests und Verifikation

- Pure Core-Tests für kombinierte exakte Filter, SQL-Injection-resistente
  Bindung, Count/IDs/Window-Konsistenz und kaskadierende Facetten inklusive
  leerer Werte.
- Frontend-Unit-Tests für Rhythmbox-Mapping, unbekannte/duplizierte Tokens,
  stabile Reihenfolge und Layout-Serialisierung/Fallback.
- Voll isolierter Browse-Smoke wählt Genre+Artist, kombiniert eine Suche und
  prüft geloggte Row-IDs/Count.
- Voll isolierter Import-Smoke verwendet eine Environment-Fixture statt der
  echten Benutzer-dconf-Daten, prüft sichtbare Reihenfolge und gespeichertes
  Reprise-Setting.
- Manuell: reale Dropdowndarstellung/Keyboard-Navigation, schmale Breite,
  Spaltenreihenfolge und echter lesender Rhythmbox-Import.

## Explizit nicht Teil dieser Etappe

- Schreiben oder Zurücksetzen von Rhythmbox-GSettings.
- Import weiterer Rhythmbox-Einstellungen (Browser-Ansicht, Fenstergröße,
  Sortierung, Plugins, Playlists oder Datenbank).
- Freie Spaltenkonfiguration per Drag-and-drop/Preferences-Dialog.
- Persistenz der Browse-Auswahl über App-Neustarts; das gehört zur
  Session-Restore-Etappe GUI-D.
- Neue Core-Abhängigkeiten zu GTK, GIO, GSettings, GStreamer oder zbus.

## Nicht verhandelbare Regeln

Core bleibt GTK/GStreamer/zbus-frei. Alle UI-Texte, Logs, Code und Kommentare
sind Englisch. Tests und Smokes verwenden ausschließlich temporäre DBs und
Fixtures; die echte Musikbibliothek und die echte Reprise-DB werden nie
berührt. Der Rhythmbox-Import ist read-only. Jede bearbeitete Datei endet unter
800 Zeilen.
