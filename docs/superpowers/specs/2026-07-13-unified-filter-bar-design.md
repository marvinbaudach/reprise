# Einheitliche Filterleiste — Design

## Ziel

Die drei dauerhaft sichtbaren Genre-, Interpret- und Album-Suchfelder werden durch die kompakte
Filterdarstellung des Referenzdesigns ersetzt. Oberhalb der Tracktabelle zeigt Reprise aktive
Bibliotheksfilter als entfernbare Chips, bietet einen einzigen Einstieg `Add filter` und stellt
Trefferzahl sowie `Reset` in derselben Leiste dar. Die globale Freitextsuche im Header bleibt
unverändert.

## Umfang

Die Leiste ist ausschließlich in `Library` sichtbar und enthält:

- die zurückhaltende Abschnittsbeschriftung `Filters`;
- aktive Chips in stabiler Reihenfolge Genre, Artist, Album;
- einen kompakten `Add filter`-Button;
- rechts die aktuelle Trefferzahl und `Reset`;
- ein zweistufiges Popover: zuerst Filtertyp, danach genau ein Suchfeld mit den möglichen Werten.

Ein Chip zeigt den Typ und den exakten Wert, beispielsweise `Artist: Brand of Sacrifice`. Leere
Tagwerte verwenden weiterhin `Unknown genre`, `Unknown artist` oder `Unknown album`. Ein Klick auf
den Schließen-Teil entfernt den Filter. Wegen der bestehenden Kaskade entfernt Genre zugleich
Artist und Album; Artist entfernt zugleich Album. `Reset` löscht alle Browse-Filter in einem Schritt.

Bereits aktive Filtertypen werden im Add-Popover nicht noch einmal angeboten. Wer einen Wert ändern
will, entfernt dessen Chip und fügt den Filter neu hinzu. Die Wertsuche arbeitet lokal und
case-insensitiv als Teilstringsuche. Die Ergebnismenge stammt weiter aus den bestehenden
kaskadierenden Core-Abfragen; es entstehen keine neuen SQL-Semantiken.

Bei schmaler Breite dürfen Chips umbrechen. Trefferzahl und Reset bleiben kompakt; die Tabelle wird
nicht durch drei expandierende Eingaben zusammengedrückt. Die Leiste zeigt bei aktiver Einschränkung
`N of M tracks`, sonst nur `M tracks`. Dabei zählen globale Suche und Browse-Filter gemeinsam zur
sichtbaren Einschränkung.

## Architektur

`ui/browse_bar.rs` bleibt Besitzer der Leiste und des `BrowseFilter`. Pure Hilfsfunktionen erzeugen
Chip-Spezifikationen, bestimmen noch verfügbare Facetten, entfernen Filter kaskadierend und filtern
Popover-Werte. Die GTK-Schicht projiziert diese Werte in ein `FlowBox`, ein `MenuButton`-Popover und
die Trefferanzeige.

Das Popover lädt Werte erst nach Auswahl einer Facette über das bestehende
`queries::query_browse_values`. Die Wahl eines Werts läuft durch denselben `apply_selection`-Pfad wie
der vorhandene isolierte Browse-Smoke. Callback-Werte werden vor GTK-Reentry aus `RefCell`s geklont.

`track_list::reload` meldet der Browse-Bar den sichtbaren Count. Nur wenn nötig wird zusätzlich der
ungefilterte Library-Count abgefragt; ein Fehler blendet die Trefferanzeige aus und lässt Tabelle und
Filter bedienbar. Die bestehende Statuszeile behält ihre Dauerangabe und bleibt unabhängig.

Da `strings.rs` an der Dateigrößengrenze liegt, wandern die bestehenden Browse-Texte zusammen mit den
neuen Filtertexten in `ui/browse_filter_strings.rs`. Die Datei wird in gettexts `POTFILES.in`
aufgenommen; deutsche Übersetzungen werden vollständig ergänzt.

## Fehlerbehandlung

- Schlägt eine Facettenabfrage fehl, bleibt das Popover leer und der Fehler wird geloggt.
- Ein Wert, der zwischen Öffnen und Auswahl verschwindet, wird nicht angewandt.
- Schlägt die ungefilterte Count-Abfrage fehl, wird nur die Trefferbeschriftung verborgen.
- Session-Restore mit leeren oder nicht mehr angebotenen Werten bewahrt den exakten Filter wie bisher.

## Tests und Verifikation

- Pure Tests für Chip-Reihenfolge, Unknown-Darstellung, verfügbare Facetten und kaskadierendes Entfernen.
- Pure Test für case-insensitive Teilstringsuche im einzigen Wertsuchfeld.
- GTK-Displaytest für Filtertyp-Auswahl, Chip-Projektion, Reset und zugängliche Bezeichnungen.
- Bestehender isolierter Browse-Smoke für Genre/Artist/Album plus Freitextsuche; zusätzlich werden
  Chip-Zustand und Trefferzahl geloggt.
- Vollständige Gates, Release-Checker, Core-Purity und Dateigrößenprüfung.
- Manuell offen bleibt ausschließlich die visuelle Beurteilung von Abständen, Umbruch und Popover bei
  realen schmalen Fensterbreiten.

## Explizit nicht Teil dieser Änderung

- Neue Filtertypen wie Rating, Year, Language oder Duration.
- Mehrere Werte derselben Facette oder OR-Verknüpfungen.
- Persistenz außerhalb des bestehenden Session-Restore.
- Änderungen an globaler Suche, Core-SQL oder Bibliotheksdateien.
- Ein dauerhaft sichtbares Suchfeld pro Facette.

## Nicht verhandelbare Regeln

Core bleibt GTK/GStreamer/zbus-frei. Code, Logs, Commit-Texte und englische UI-Quelltexte bleiben
Englisch; deutsche gettext-Übersetzungen werden gepflegt. Tests verwenden nur temporäre Datenbanken
und vollständig isolierte Displays. Jede erstellte oder wesentlich bearbeitete Datei bleibt unter
800 Zeilen.
