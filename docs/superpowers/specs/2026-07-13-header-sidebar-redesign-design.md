# Header- und Sidebar-Redesign — Designspezifikation

## Ziel

Das Bibliotheksfenster folgt wieder sichtbar der freigegebenen GTK4-Vorlage
`docs/design/2026-07-11-designmock-gtk4.pdf`, Seite 7a. Eine einzige flache
Headerbar spannt über Seitenleiste, Titelliste und optionales Informationspanel.
Der aktuelle Ansichtstitel bleibt geometrisch in der Fensterbreite zentriert;
Suche und kompakte Aktionen stehen rechts. Die linke Navigation wird schmaler,
erhält eindeutige symbolische Icons und übernimmt die ruhigere vertikale
Gliederung der Vorlage.

Die Vorlage bleibt entsprechend der bereits freigegebenen Hauptspezifikation
eine grobe Richtung. Native Adwaita-Zustände, adaptive Navigation und bestehende
Funktionen haben Vorrang vor pixelgenauer Nachbildung.

## Umfang

### Enthalten

- eine fensterbreite `AdwHeaderBar` oberhalb des vollständigen
  `AdwNavigationSplitView`, einschließlich geöffnetem Informationspanel;
- `AdwCenteringPolicy::Strict` für den dynamischen Ansichtstitel;
- Suchfeld rechts neben dem Titelbereich mit begrenzter, mockupnaher Breite;
- bestehende Scan-, Import-, Compact-, Informations- und Menüaktionen als
  kompakte, zugängliche Headeraktionen;
- einen dauerhaft erreichbaren Headerknopf, der die vollständige linke
  Sidebar-Spalte ein- und ausklappt;
- eine Sidebarbreite von ungefähr 220–280 px statt der heute sehr breiten
  Standardaufteilung;
- Icons für Bibliothek, Warteschlange, manuelle und intelligente Playlists
  sowie Problemquellen;
- einheitliche Iconspalte, Abstände, Ellipsierung, Zähler und Abschnittszeilen;
- unveränderte Auswahl-, Aktivierungs-, DnD-, Kontextmenü-, Zähler- und
  Wiederherstellungslogik;
- isolierte Display-/Pointer-Prüfung und ein Eintrag in der manuellen QA-Liste.

### Nicht enthalten

- Änderungen an Track-Tabelle, Browse-Leiste, Informationskarten oder
  Playerleisten-Inhalt;
- eigene CSS-Farbwerte, Glasoptik oder nicht-native Fensterdekoration;
- neue Navigationseinträge wie das im frühen Mockup skizzierte Radar;
- Änderungen an Datenbank, Musikdateien, Scrobbling oder Netzwerkverhalten;
- eine pixelgenaue Kopie der PDF-Vorlage.

## Architektur

### Fensterrahmen

Ein neues fokussiertes UI-Modul baut den Library-Rahmen aus einer äußeren
`AdwToolbarView`: die Headerbar ist deren einzige Top-Bar, der bestehende
Navigation-Split ist der Content. Der bisherige innere `ToolbarView` behält
Scanfortschritt, Titelliste, Status und Playerbar, enthält aber keine Headerbar
mehr. Damit ist die Headerbreite unabhängig von Sidebar und Info-Panel.

`MinimalView` speichert seinen Library-Root als allgemeines `GtkWidget` statt
als konkreten `AdwNavigationSplitView`. Die adaptive Navigation selbst bleibt
weiterhin über die separat gehaltene Split-View-Referenz verdrahtet.

### Sidebar-Präsentation

Die reine Navigations- und Datenlogik bleibt in `sidebar.rs`. Ein neues
Geschwistermodul enthält Präsentationszuordnung und Widgetbau. Das hält die
bereits randvolle Datei unter 800 Zeilen und trennt quellenspezifische Icons
von Auswahl-, Callback- und Datenbanklogik.

Intelligente Playlists erhalten Icons anhand ihrer stabilen Sortierfelder:
`last_played_at`, `rating` und `added_at`. Unbekannte oder künftig
benutzerdefinierte Regeln fallen auf ein neutrales Playlist-Symbol zurück.

## Verhalten und Barrierefreiheit

- Alle Icon-Buttons behalten Tooltip und Accessible Label.
- Icons ergänzen Text, ersetzen ihn in der Sidebar nicht.
- Zähler bleiben numerisch und rechtsbündig; lange Namen werden ellipsiert.
- Die native `navigation-sidebar`-Auswahl bleibt alleinige Hervorhebung.
- Der Sidebar-Knopf bleibt sichtbar, solange die Sidebar in den
  Layouteinstellungen aktiviert ist. Bei breitem Fenster entfernt er die
  vollständige linke Spalte und gibt den Platz der Tracktabelle zurück; beim
  Wiedereinblenden stellt er die geteilte Ansicht wieder her.
- Im schmalen adaptiven Layout wechselt derselbe Knopf zwischen Sidebar- und
  Content-Seite. Eine Quellenauswahl führt weiterhin automatisch zum Content.
- Keine `RefCell`-Ausleihe reicht über einen GTK-Aufruf oder Callback.

## Tests und Verifikation

- RED/GREEN-Displaytest für die äußere Toolbar-Hierarchie, flachen Stil,
  strikte Titelzentrierung und rechte Suche;
- RED/GREEN-Unit-Tests für alle Sidebar-Iconzuordnungen und den unbekannten
  Smart-Playlist-Fallback;
- Displaytest für Iconspalte, Zähler und Sidebarbreiten;
- vollständige Projekt-Gates, Dateigrößenprüfung und isolierter Pointer-/
  Screenshot-Lauf mit privatem XDG-Daten-/Cachepfad, eigener D-Bus-Session,
  Xvfb, X11, leerem Wayland-Display und `fakesink`;
- manuelle Prüfung unter nativem GNOME/Wayland für tatsächliche Abstände,
  Icon-Theme, HiDPI und schmale Fenster.

## Fehlerverhalten

Das Redesign führt keine neue I/O ein. Fehlt ein Symbol im aktiven Icon-Theme,
bleibt der Text weiterhin verständlich. Bestehende Fehlerpfade für Scan,
Import, Wiedergabe und Navigation werden nicht verändert.

## Explizit nicht tun

- keine produktiven App-Läufe auf Desktop, echter Datenbank oder Musikordner;
- keine zusätzlichen Farbanbieter oder globale CSS-Provider installieren;
- keine Headeraktion entfernen, nur um die Vorlage optisch zu treffen;
- keine Sidebar-Zähler oder Problemquellen ausblenden;
- keine Arbeit aus dem parallelen Playerbar-Branch kopieren oder überschreiben.
