# Vereinfachte Compact-Fensterleiste — Design

## Ziel

Die Fensterleiste der Compact-Ansicht zeigt ausschließlich den App-Namen
`Reprise`. Die zusätzliche zweite Zeile mit dem gewählten Layout (`Cover`,
`Pill` oder `Card`) entfällt, weil das aktive Layout bereits durch die
sichtbare Fensterkomposition und im Layout-Menü eindeutig erkennbar ist.

## Umfang

- Alle Compact-Layouts mit eigener `AdwHeaderBar` verwenden ein
  `AdwWindowTitle` mit Titel `Reprise` und leerem Untertitel.
- Das integrierte Pill-Layout bleibt unverändert; es besitzt bewusst keine
  separate HeaderBar.
- Layout-Auswahl, aktive Radio-Markierung, Persistenz, Fenstermaße,
  Dekorationsmodus und Menüaktionen bleiben unverändert.
- Die Layout-Namen und Übersetzungen bleiben bestehen, weil sie weiterhin im
  Compact-Menü benötigt werden.

## Tests und QA

- Die drei bestehenden isolierten Layout-Vertragstests prüfen zusätzlich: Eine
  separate HeaderBar enthält genau einen `AdwWindowTitle`, dessen Titel
  `Reprise` und dessen Untertitel leer ist; das Pill-Layout enthält keinen.
- Vollständige Gates, Release-Check und isolierter Compact-App-Smoke.
- Native GNOME-Abstände und vertikale Zentrierung bleiben ein manueller
  Sichtcheck.

## Explizit nicht Teil dieser Änderung

- Keine Änderung an Layout-Menü, Layout-Namen, Speicherung oder Wechselpfaden.
- Keine Entfernung der Fensterleiste oder ihrer Schließen/Minimieren-/Menü-
  Aktionen.
- Keine Änderung an Metadaten, Cover, Transport oder Fenstergeometrie.
