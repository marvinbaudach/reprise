# Stabiler Wechsel von Compact zur Bibliothek — Design

## Ziel

Beim Verlassen der Compact-Ansicht darf kein sichtbar aufgeblasener Compact-Inhalt als
Zwischenzustand erscheinen. Die vollständige Bibliothekswurzel muss bereits im persistenten
Fenster-Host liegen, bevor GTK die Mindest- und Zielgröße des Bibliotheksfensters anfordert.

## Verhalten und Architektur

- `MinimalView` behält den vorhandenen Ein-Fenster- und Ein-Player-Pfad bei.
- Beim Wechsel zu Compact bleibt die bestehende Reihenfolge erhalten: alte große Constraints
  lösen, Compact-Wurzel einsetzen, danach Compact-Maße anwenden.
- Beim Rückweg wird die Reihenfolge gespiegelt: zuerst Bibliothekswurzel einsetzen, danach das
  Fenster wieder resizable machen und Bibliotheks-Constraints, gespeicherte Größe sowie optional
  Maximierung anwenden.
- Persistenz, ausgewähltes Layout, gespeicherte Bibliotheksgeometrie, separate Titelleiste und
  Wiedergabezustand ändern sich nicht.
- Es wird keine zeitbasierte Animation und kein zweiter Widget-/Player-Baum eingeführt.

## Tests und QA

- Ein isolierter GTK-Regressionsvertrag beobachtet die erste Größenänderung beim Restore und
  verlangt, dass zu diesem Zeitpunkt bereits die Bibliothekswurzel im Content-Host liegt.
- Bestehende Transition-, Compact-, Dekorations- und Workspace-Tests bleiben grün.
- Ein vollständig isolierter Anwendungssmoke übt Compact → Bibliothek aus.
- Das tatsächliche Frame-Pacing unter GNOME/Wayland bleibt ein manueller Sichtcheck.

## Explizit nicht Teil

- Keine neue Crossfade-/Slide-Animation.
- Keine Änderung an Layouts, Fensterdekorationen, Controls oder Shortcuts.
- Kein Zugriff auf echte Musik, Datenbank, Cache oder Desktop-Session.
