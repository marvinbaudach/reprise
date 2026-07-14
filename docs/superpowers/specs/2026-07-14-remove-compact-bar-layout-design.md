# Compact-Layout ohne Bar — Design

## Ziel

Die Layoutvariante `Bar` wird vollständig aus dem Compact Player entfernt. Die
Layoutauswahl zeigt nur noch `Cover`, `Pill` und `Card`. `Card` wird zum
Standard, weil es Metadaten und Transportsteuerung in einer klaren,
fenstergerechten Komposition vereint.

## Verhalten und Kompatibilität

- Das Layoutmenü enthält genau Cover, Pill und Card.
- Ein neuer oder ungültiger gespeicherter Layoutwert öffnet Card.
- Der historische persistierte Wert `bar` wird beim Lesen als Card behandelt.
  Damit starten bestehende Profile ohne leere Ansicht oder Auswahlfehler.
- Beim nächsten expliziten Layoutwechsel wird nur noch einer der drei gültigen
  Werte gespeichert; eine Datenbankmigration ist dafür nicht notwendig.
- Cover- und Pill-Aufbau, Wiedergabestatus, Queue, Kontextmenü und Rückkehr zur
  Bibliothek bleiben unverändert.

## Architektur

`reprise-core::library::settings::CompactLayout` enthält nur noch `Cover`,
`Pill` und `Card`. Der zentrale Settings-Leser übernimmt die
Rückwärtskompatibilität für `bar`; dadurch müssen GTK-Aufbau, Menü und
Smoke-Hooks keinen entfernten Layouttyp kennen.

Die GTK-Layoutfactory entfernt den Bar-Root und dessen Metriken. Der
Compact-Stack wird aus genau drei Roots aufgebaut und startet mit Card. Das
Layoutmenü leitet seine Einträge weiterhin aus derselben typisierten Liste ab.

## Tests und QA

- Core-Regressions für Card als Standard, die drei gültigen Roundtrips, die
  Legacy-Abbildung `bar` nach Card und Card als Fallback bei ungültigen Werten.
- Pure UI-Regressions für exakt drei Tokens und die Ablehnung von `bar` im
  aktuellen UI-Vertrag.
- GTK-Verträge für die drei verbleibenden Layouts und Card als initial sichtbare
  Ansicht.
- Vollständige Projekt-Gates, Core-Purity, Release-Checker und Dateigrößen.
- Reale Größenwirkung, Abstände und Fensterdekoration bleiben ein manueller
  Sichtcheck unter GNOME.

## Explizit nicht Teil dieser Änderung

- Keine Neugestaltung von Cover, Pill oder Card.
- Keine Änderung an Library Player Bar oder deren Position.
- Keine Datenbankmigration und keine Änderung an Musikdateien.
