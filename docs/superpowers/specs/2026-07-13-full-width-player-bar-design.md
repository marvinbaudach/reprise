# Vollbreite Bibliotheks-Playerleiste — Design

## Ziel

Die Bibliotheks-Playerleiste entspricht wieder dem beschlossenen Hauptfenster-
Design: Sie läuft über die gesamte Fensterbreite unter beziehungsweise über
Seitenleiste, Bibliotheksinhalt und Informationspanel. Ihre Bedienelemente sind
in drei klar lesbare Bereiche gegliedert, statt Transport und sekundäre
Wiedergabeoptionen in einer einzigen Folge zu mischen.

## Visuelle Struktur

Die Leiste verwendet weiterhin native GTK-/Adwaita-Widgets und den bestehenden
Playerzustand. Von links nach rechts:

1. **Trackbereich:** 48-px-Cover, Titel und Interpret. Titel und Interpret
   ellipsieren, damit lange Metadaten die Mitte nicht verschieben.
2. **Mittelbereich:** Zurück, Play/Pause und Weiter als zusammengehörige
   Transportzeile; darunter Position, flexible Seek-Fläche und Gesamtdauer.
3. **Sekundärbereich:** Shuffle, Repeat und Lautstärke, rechtsbündig und vom
   Haupttransport getrennt.

Der Play/Pause-Knopf bleibt die primäre Aktion. Bestehende Tooltips,
Accessible Names, Tastaturfokus, Seek-Drag-Disziplin und Zustandsindikatoren
bleiben erhalten. Die Leiste fügt keine neue Playback-Funktion hinzu.

## Fenster-Topologie

Der gegenwärtige Fehler ist strukturell: Status- und Playerleiste sind als Bar
des mittleren `AdwToolbarView` eingehängt. Dieses ToolbarView ist nur das
Bibliotheks-Content-Pane; Seitenleiste und Informationspanel liegen außerhalb.
Darum kann die Leiste nie die volle Fensterbreite erhalten.

Ein neues fokussiertes Frontend-Modul `library_player_bar.rs` besitzt einen
vertikalen Bibliotheks-Root:

- als expandierendes Hauptkind den vollständigen `AdwNavigationSplitView`
  einschließlich Seitenleiste, Content und Informationspanel;
- als Geschwisterkind den bestehenden Status-/Playerleisten-Block.

`PlayerBarPosition::Bottom` hängt diesen Block nach dem Split-View ein,
`Top` davor. Der Positionswechsel ordnet nur diese Geschwister neu und bleibt
sofort wirksam. Das innere Content-`ToolbarView` behält Header, Scanfortschritt
und Trackliste, besitzt aber nicht länger die globale Playerleiste.

Die vollständige Bibliothekswurzel wird als generisches GTK-Widget an den
Library/Compact-Koordinator übergeben. Der Compact-Modus selbst, seine vier
Layouts und die getrennte Bibliotheksgeometrie ändern sich nicht.

## Modulgrenzen

- `library_player_bar.rs`: globaler Bibliotheks-Root, volle Breite und
  Top/Bottom-Umschaltung.
- `player_bar_layout.rs`: reine Widget-Komposition der drei Playerleisten-
  Zonen; keine Controller- oder Queue-Logik.
- `player_bar.rs`: bestehende Zustandsprojektion, Eingabecallbacks und
  Seek-Guards; verwendet nur die gebauten Widgets.
- `window.rs`: schlanke Composition-Root-Aufrufe. Die bereits kantennahe Datei
  erhält keine neue Layoutlogik.

## Responsives Verhalten

Der Mittelbereich darf horizontal schrumpfen; die Seek-Fläche expandiert und
schrumpft zuerst. Tracktexte ellipsieren. Buttons behalten native Zielgrößen.
Das vorhandene Mindestfenster von 600 × 400 bleibt gültig. Es gibt keinen
separaten mobilen Playerleistenmodus; bei engem Fenster übernimmt weiterhin
Adwaitas bestehende Navigation, während die globale Leiste sichtbar bleibt.

## Fehler- und Zustandsverhalten

Die Änderung ist ausschließlich kompositorisch. Track, Queue, Playbackstatus,
Position, Shuffle, Repeat, Lautstärke, MPRIS, Covergeneration und Scrobbling
laufen unverändert durch den einen `PlayerController`. Ein Positionswechsel
lädt oder pausiert keinen Track. Scheitert das Persistieren der Einstellung,
bleibt das bestehende Preferences-Verhalten unverändert.

## Tests und Verifikation

- Ein isolierter Displaytest beweist, dass Playerleiste und vollständiger
  Split-View Geschwister sind, die Leiste bei Top und Bottom die Root-Breite
  erhält und der Wechsel keine doppelte Parent-Beziehung erzeugt.
- Ein isolierter Displaytest beweist die drei Zonen: Cover/Metadaten links,
  Zurück/Play/Weiter plus Seek in der Mitte, Shuffle/Repeat/Lautstärke rechts;
  alle bisherigen Bedienelemente bleiben vorhanden und zugänglich.
- Bestehende Player-, Seek-, MPRIS-, Compact- und Preferences-Tests bleiben
  unverändert grün.
- Der vollständig isolierte Pointer-Harness liefert eine aktuelle breite
  Hauptfensteraufnahme und muss ohne GTK-/GLib-/Panic-/`RefCell`-Fehler enden.
- Vollständiger Releasechecker, gettext, Dateigrößen und Core-Purity bleiben
  verpflichtend.

## Manuelle native Prüfung

Headless nicht als erledigt behauptet werden dürfen: optische Balance der drei
Zonen auf dem echten GNOME-Desktop, lange Metadaten, 600-px-Mindestbreite,
HiDPI, Hell-/Dunkelmodus, Touch-Ziele sowie Top/Bottom-Umschaltung unter
Wayland mit geöffnetem und geschlossenem Informationspanel.

## Explizit nicht enthalten

- Keine Änderung an Compact Bar/Cover/Pill/Card.
- Keine neuen Playbackfunktionen oder neue Playerinstanz.
- Keine schwebende, transparente oder überlagernde Leiste.
- Kein frei konfigurierbarer Playerleisten-Editor.
- Keine Änderung an Sidebar-, Informationspanel- oder Now-Playing-Inhalten.
