# Fensterdekorationsmodus — Designspezifikation

> Der SSD-Teil dieser Spezifikation ist durch
> `2026-07-14-reliable-separate-title-bar-design.md` abgelöst. Persistente Tokens
> und Chromium-CSD-Standard bleiben bestehen; `system` zeigt nun eine separate
> native GTK-Leiste im dauerhaften Fensterinhalt.

## Ziel

Reprise verwendet standardmäßig die bereits entworfene flache, Chromium-artige
Client-Side Decoration (CSD): Die eigene Headerbar ist zugleich Drag-Fläche und
trägt die nativen Fensterknöpfe. Unter **Einstellungen → Darstellung** kann der
Nutzer live auf eine Systemtitelleiste mit Systemrahmen (Server-Side Decoration,
SSD) umschalten. Die Wahl wird dauerhaft gespeichert und beim nächsten Start vor
dem ersten Präsentieren des Fensters angewendet.

GTK übermittelt den Dekorationswunsch an das Fenstersystem. Ein Window Manager
oder Wayland-Compositor darf SSD ablehnen und auf CSD zurückfallen. Reprise zeigt
in diesem Fall keine doppelten eigenen Fensterknöpfe und bleibt bedienbar.

## Umfang

### Enthalten

- typisierte persistente Einstellung `ui.window_decoration_mode` im reinen Core;
- `Client` als toleranter Standard für neue, fehlende und unbekannte Werte;
- ein fokussierter GTK-Controller für Fensterrahmen und interne Fensterknöpfe;
- sofortige Umschaltung zwischen eigener flacher CSD und angeforderter SSD;
- konsistente Behandlung von Library-Header sowie Cover-, Pill- und
  Card-Kompaktlayout;
- eine Auswahlzeile auf der Seite Darstellung mit verständlicher Erklärung;
- vollständige deutsche Übersetzung, isolierte Displaytests und native
  GNOME-/Wayland-Prüfpunkte.

### Nicht enthalten

- eigene gezeichnete Titelleistenknöpfe oder nicht-native Rahmen;
- compositor- oder X11-spezifische FFI beziehungsweise private Protokolle;
- automatische Erkennung, ob der Compositor den SSD-Wunsch tatsächlich erfüllt;
- Änderungen an Headerinhalt, Seitenleiste, Trackliste oder Playerleiste;
- eine globale Manipulation der Prozessvariable `GTK_CSD`;
- Änderungen an Musikdateien, Netzwerk, Scrobbling oder realer Datenbank.

## Architektur

### Core-Einstellung

`reprise_core::library::settings::WindowDecorationMode` besitzt die Varianten
`Client` und `System`. Getter und Setter verwenden die stabilen Tokens `client`
und `system`. Der Getter fällt bei fehlendem, unlesbarem oder unbekanntem Wert
auf `Client` zurück. Der Core bleibt frei von GTK, libadwaita, GStreamer und
zbus.

### GTK-Controller

Ein neues Modul `ui/window_decorations.rs` besitzt nur geklonte Widget-Handles:
das `AdwApplicationWindow`, die Library-`AdwHeaderBar`, alle kompakten
`AdwHeaderBar`s und die Pill-`GtkWindowControls`. Es kapselt die Projektion des
persistierten Modus:

- `Client`: GTKs eigener dekorierbarer CSD-/Resize-Rahmen bleibt aktiv, der
  realisierte `GdkToplevel` fordert keine Desktop-Dekoration an, interne
  Header-Titelknöpfe und Pill-WindowControls sind sichtbar;
- `System`: der realisierte `GdkToplevel` fordert Desktop-Dekorationen an,
  interne Header-Titelknöpfe und Pill-WindowControls sind verborgen.

Der GTK-`decorated`-Zustand bleibt in beiden Modi aktiv, damit CSD-Schatten und
Resize-Ränder nicht verloren gehen. Der gezielte GDK-Hinweis signalisiert im
CSD-Modus, dass Reprise den Rahmen selbst zeichnet, und fordert im Systemmodus
Dekorationen vom Desktop an. Die App-Toolbar bleibt
in beiden Modi als flache Inhalts- und Drag-Leiste erhalten; im Systemmodus ist
sie keine zweite Fensterbedienung.

Der Controller sammelt kompakte Dekorationswidgets beim Fensteraufbau durch eine
kleine, rein lokale Widgetbaum-Suche. Es entsteht kein zweiter Player- oder
View-State. `PreferencesContext` hält einen `Rc` auf diesen Controller, liest
den Startwert ohne GTK-Aufruf unter einer SQLite-Ausleihe und wendet ihn erst
nach dem Ende der Ausleihe an.

### Einstellungsseite und Texte

Weil `preferences.rs` und `strings.rs` randvoll sind, liegt die neue
Darstellungszeile in `preference_window_decorations.rs`; ihre gettext-markierten
Texte liegen in einem kleinen Geschwistermodul. `po/POTFILES.in` nimmt dieses
Modul auf. Eine `AdwComboRow` bietet „Chromium (CSD)“ und „System
title bar“ an und erklärt, dass die Systemunterstützung vom Desktop abhängt.

## Datenfluss und Fehlerverhalten

Beim Start: Datenbank lesen → Moduswert kopieren → Controller anwenden → Fenster
präsentieren. In den Einstellungen: Auswahl ändern → Einstellung speichern → nur
bei erfolgreichem Speichern live anwenden. Schlägt das Speichern fehl, bleibt
der sichtbare Modus unverändert; es gibt keine teilweise Änderung.

Der Window Manager darf die angeforderte Dekoration ignorieren. Das ist kein
App-Fehler: Reprise behält seine Toolbar, vermeidet doppelte App-Knöpfe und
überlässt den tatsächlichen Außenrahmen dem GTK-/Desktop-Fallback.

## Barrierefreiheit und Verhalten

- Beide Optionen besitzen ausgeschriebene, übersetzbare Bezeichnungen.
- Die Auswahl ist per Tastatur erreichbar und wird sofort wirksam.
- Bestehende Tooltips und Accessible Labels der Headeraktionen bleiben gleich.
- CSD bleibt der Standard und bewahrt die freigegebene flache Gestaltung.
- Keine `RefCell`- oder SQLite-Ausleihe reicht über einen GTK-Aufruf.

## Tests und Verifikation

- RED/GREEN-Coretests für Default, beide Tokens, Roundtrip und unbekannten Wert;
- RED/GREEN-Unit-Tests für stabile Combo-Indizes;
- isolierter RED/GREEN-Displaytest, der `decorated`, Titelknöpfe aller Header und
  Pill-WindowControls in beiden Modi prüft;
- vollständige Gates, Core-Purity, gettext-Vollständigkeit und Dateigrößen;
- vollständig isolierter App-Smoke mit privatem XDG-Daten-/Cachepfad, eigener
  D-Bus-Session, Xvfb, X11, leerem Wayland-Display und `fakesink`;
- manuelle native GNOME-/Wayland-Prüfung von Live-Umschaltung, echter
  Compositor-Reaktion, Rahmen, Drag-Verhalten, Neustart und allen Compact-Layouts.

## Explizit nicht tun

- keine produktiven App-Läufe auf dem echten Desktop;
- keine echte Musikbibliothek, reale Reprise-Datenbank oder Nutzerkonten öffnen;
- keinen SSD-Erfolg vortäuschen, wenn der Compositor nur einen Fallback liefert;
- keine Headeraktionen entfernen und keine parallelen Fensterzustände einführen;
- keine privaten GTK-Interna oder Backend-spezifischen Dekorations-Hacks nutzen.
