# Minimalansicht und vollständige native Einstellungen — Design

## Ziel

Reprise erhält einen kompakten Fenstermodus sowie den im Master-Design 7b/7c
vorgesehenen nativen Einstellungsdialog. Jede sichtbare Einstellung wirkt wirklich
und wird gespeichert; es gibt keine Attrappen oder „Coming soon“-Schalter.

## Minimalansicht

Die Minimalansicht verwendet dieselbe `PlayerBar` und denselben
`PlayerController`. Beim Wechsel wird die Leiste kontrolliert aus dem normalen
Toolbar-Bereich in eine kompakte `ToolbarView` mit Wiederherstellen-Schaltfläche
umgehängt. Queue, Seek, Cover, MPRIS und Playback bleiben deshalb ein einziger
Zustandspfad. Der vorherige Fensterzustand wird beim Zurückwechseln restauriert;
die normale Session-Geometrie darf nicht durch Minimalmaße überschrieben werden.

## Einstellungen

Ein `AdwPreferencesDialog` enthält:

- Wiedergabe: echter 10-Band-Equalizer, Presets, ReplayGain Aus/Titel/Album.
- Darstellung: System/Hell/Dunkel.
- Layout: Playerleiste oben/unten, Sidebar und Statuszeile sichtbar,
  Listendichte Komfortabel/Standard/Kompakt, Spalteneditor öffnen.
- Bibliothek: aktueller Ordner, Ordner wählen/neu scannen, read-only
  Rhythmbox-Spaltenimport.
- Plugins: nur optionale Integrationen und Funktionen mit externen Diensten/APIs.
  Der Online-Coverabruf wirkt sofort; MPRIS nennt ehrlich den nötigen Neustart,
  solange dessen D-Bus-Lebenszyklus nicht sicher hot-reloadbar ist. Equalizer und
  ReplayGain sind feste Wiedergabefunktionen und erscheinen ausschließlich dort.

## Architektur

Typisierte Werte und Accessors leben in `reprise-core::library::settings`.
`minimal_view.rs` besitzt ausschließlich den Widget-Reparenting-Zustand.
`preferences.rs` baut Dialog/Rows und erhält kleine Callbacks vom Composition Root;
`window.rs` wächst nicht über 800 Zeilen.

Der plattformneutrale Playback-Vertrag erhält eine kleine `AudioEffects`-Konfiguration.
Linux setzt sie über einen `audio-filter`-Bin vor dem `playbin3`-Audioausgang um.
Fehlende GStreamer-Plugins ergeben einen nicht verfügbaren Schalter/Toast, niemals
einen Playback-Absturz. Pipeline-Rebuilds wenden die letzte Konfiguration erneut an.

## Sicherheit und Fehler

Alle DB-Borrows enden vor GTK-/Callback-Aufrufen. Persistenzfehler stellen den alten
Widgetzustand wieder her und zeigen einen Toast. Minimal-Reparenting prüft Eltern vor
`remove`. Keine Musikdatei und keine fremde GSettings-Quelle wird geschrieben.

## Tests

Reine Tests für sämtliche Setting-Fallbacks, Minimal-Transitionen, Dichte/Theme und
EQ-Presets; Plattformtests mit `fakesink` für Filteraufbau/Rebuild; isolierte
Display-/App-Smokes für Minimalwechsel und jede Preferences-Seite; vollständige
Release-Gates und native GNOME-Prüfliste.

## Nicht Teil

Schwebende/gläserne Playerleiste, Fremd-Plugin-Installation, mehrere
Bibliotheksordner, Crossfade, Geräte-Synchronisation und Onlinekonten.
