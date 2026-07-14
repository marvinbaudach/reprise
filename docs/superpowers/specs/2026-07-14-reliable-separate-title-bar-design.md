# Zuverlässige separate Titelleiste — Design

## Ziel

Die zweite Fensterdekorationsoption muss unter GNOME Wayland sichtbar und
funktional von der integrierten Chromium-Leiste unterscheidbar sein. Statt nur
eine vom Compositor optional ignorierbare Server-Side Decoration anzufordern,
zeigt Reprise im bestehenden `system`-Modus eine separate native GTK-Titelleiste
mit App-Titel sowie Minimieren-/Maximieren-/Schließen-Controls.

## Ursache

Der bisherige Controller setzt `GdkToplevel:decorated=true` und wartet auf die
GTK-CSS-Klasse `ssd`. GNOME Wayland stellt für normale Wayland-Clients keine
serverseitige Titelleiste bereit. Der sichere Fallback lässt deshalb die
integrierten Controls sichtbar; optisch unterscheidet sich die Auswahl nicht
vom Chromium-Modus und kann ihr Produktversprechen nicht erfüllen.

## Verhalten

- `Chromium (CSD)` bleibt Standard: eine flache integrierte App-Leiste mit den
  nativen Fensterknöpfen der jeweiligen Library- oder Compact-Wurzel.
- `Separate title bar` blendet eine native `GtkHeaderBar` als oberste Leiste
  eines dauerhaften `AdwToolbarView` ein. Sie zeigt `Reprise` und die vom
  GTK-/Desktop-Setting bestimmten Fensterknöpfe.
- Die darunterliegende Library-Leiste bleibt als App-Toolbar mit aktuellem
  Quellentitel und Aktionen erhalten, verbirgt aber ihre Fensterknöpfe.
- Cover und Card behalten Menü und Aufbau, verbergen in diesem Modus jedoch
  ihren doppelten `Reprise`-Titel. Pill erhält die separate Titelleiste oberhalb
  seiner integrierten Zeile.
- Live-Umschaltung und Neustartpersistenz verwenden weiterhin die kompatiblen
  Core-Tokens `client` und `system`; es ist keine Datenbankmigration nötig.
- Der Wechsel zurück zu Chromium entfernt die separate Titelleiste vollständig,
  stellt die integrierten Fensterknöpfe und Compact-Titel wieder her und behält
  Playback, Layout und Geometriezustand.

## Architektur

`WindowDecorations` besitzt einen dauerhaften äußeren `AdwToolbarView`, eine
einmal erzeugte `GtkHeaderBar` als dessen oberste Leiste sowie getrennte Handles
für Library- und Compact-Header. Library und Compact tauschen nur den Inhalt
dieses Hosts; dadurch bleibt die separate Leiste bei jedem Layoutwechsel erhalten.
Der Controller projiziert nur den typisierten Modus und hält keine SQLite-Ausleihe.

`AdwApplicationWindow` unterstützt `GtkWindow:set_titlebar` nicht. Eine
`GtkHeaderBar` als Top Bar des äußeren `AdwToolbarView` liefert stattdessen die
native Drag-Fläche, Titelknöpfe, Fokus-/Backdrop-Stile und das Desktop-
`gtk-decoration-layout`. Beide Optionen bleiben ehrliche Client-Side Decoration;
der historische Token `system` bezeichnet aus Kompatibilitätsgründen nun die
separate native Leiste. Damit ist das Verhalten auf GNOME Wayland zuverlässig,
ohne Backendabfragen, private Protokolle oder vorgetäuschte SSD-Bestätigung.

Im Compact-Modus addiert die Geometrieprojektion die natürliche Höhe der
sichtbaren separaten Leiste. Ein Moduswechsel löst die aktuelle Layoutgeometrie
erneut aus, ohne View-, Playback- oder Persistenzzustand zu duplizieren.

## Tests und QA

- Reiner Projektionsvertrag für integrierte gegenüber separater Leiste.
- Isolierter Displaytest für Titelbar-An-/Abwesenheit, genau zwei verbleibende
  Compact-Header, Fensterknöpfe und Compact-Titel in beiden Modi.
- Live-Roundtrip nach `present()` und Startanwendung vor `present()`.
- Isolierter App-Smoke mit temporärer Datenbank und persistiertem `system`-Token.
- Vollständige Gates, gettext, Core-Purity, Dateigrößen und Release-Checker.
- Native GNOME-Prüfung für tatsächliche Abstände, Drag/Resize, HiDPI und
  Window-Manager-Buttonlayout bleibt manuell.

## Explizit nicht Teil

- Keine Änderung an Playback, Compact-Layouts oder Library-Inhalten.
- Keine Wayland-/X11-Erkennung und keine privaten Mutter-Protokolle.
- Keine Änderung der persistenten Tokens oder des CSD-Standards.
