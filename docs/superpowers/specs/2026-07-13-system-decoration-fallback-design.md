# Systemdekorations-Fallback — Designspezifikation

## Ziel

Die Einstellung „System title bar“ darf Reprise niemals ohne sichtbare
Fensterbedienung zurücklassen. Reprise fordert weiterhin Dekorationen vom
Desktop an. Die eigenen Titelknöpfe werden aber erst verborgen, wenn GTK den
tatsächlichen serverseitig dekorierten Zustand über die dokumentierte
CSS-Klasse `ssd` meldet.

## Ursache

`GdkToplevel:decorated=true` ist nur ein Wunsch an den Desktop. Der bisherige
Controller behandelte diesen Wunsch sofort als Erfolg und blendete alle
`AdwHeaderBar`-Titelknöpfe sowie die expliziten `GtkWindowControls` aus.
`AdwApplicationWindow` besitzt keinen separaten GTK-Titelbereich. Lehnt der
Compositor SSD ab oder unterstützt sie nicht, bleibt deshalb keine sichtbare
Fensterbedienung übrig.

## Verhalten

- `Client`: Reprise meldet eigene CSD und zeigt alle eigenen Fensterknöpfe.
- `System` ohne bestätigte `ssd`-Klasse: Reprise fordert SSD an, behält aber die
  eigenen Controls als sicheren Fallback.
- `System` mit bestätigter `ssd`-Klasse: Reprise verbirgt die eigenen Controls,
  damit keine doppelten Fensterknöpfe erscheinen.
- Wenn GTK den `ssd`-Zustand später setzt oder entfernt, wird die Projektion
  sofort aktualisiert.

Die persistierte Einstellung und ihre Tokens bleiben kompatibel. Es gibt keine
Backend-, Desktop- oder Umgebungsvariablen-Erkennung und keine private FFI.

## Tests

- Reiner RED/GREEN-Test für Client, unbestätigten System-Fallback und
  bestätigten SSD-Zustand.
- Isolierter Displaytest für Library- und alle Compact-Controls: Systemmodus
  behält Controls ohne `ssd`, simuliertes `ssd` verbirgt sie, Verlust von `ssd`
  stellt sie wieder her.
- Vollständige Projekt-Gates, Dateigrößen und isolierter App-Smoke.

## Explizit nicht tun

- keine echte Systemtitelleiste vortäuschen, wenn der Desktop keine liefert;
- kein Wechsel von `AdwApplicationWindow` zu einer zweiten Fensterklasse;
- keine Desktopnamen, Wayland-Protokolle oder X11-Sonderpfade abfragen;
- keine echte Nutzerdatenbank, Musikdateien oder Live-Desktop-Session öffnen.
