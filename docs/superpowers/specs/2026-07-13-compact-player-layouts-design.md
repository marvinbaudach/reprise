# Auswählbare Kompaktplayer-Layouts — Design

## Ziel

Reprise besitzt zwei klar getrennte, schnell umschaltbare Fensteransichten:

1. Die **Bibliotheksansicht** bleibt das vollständige Arbeitsfenster für Suche,
   Browse-Filter, Queue, Playlists, Tagpflege und Einstellungen.
2. Die **Kompaktansicht** ist ein dauerhaft nutzbarer kleiner Player. Innerhalb
   dieser Ansicht wählt der Nutzer eines von vier kuratierten Layouts:
   **Leiste**, **Cover**, **Pill** oder **Card**.

Die Varianten bedienen unterschiedliche Hörgewohnheiten, ohne vier unabhängige
Player zu erzeugen. Playback, Queue, MPRIS, Coverauflösung und Sessionzustand
bleiben ein gemeinsamer Zustandspfad.

## Begriffe und persistente Werte

Der Code verwendet zwei typisierte Werte in
`reprise_core::library::settings`:

- `WindowViewMode::{Library, Compact}` unter `ui.window_view_mode`, gespeichert
  als `library` oder `compact`; unbekannte Werte fallen auf `Library` zurück.
- `CompactLayout::{Bar, Cover, Pill, Card}` unter `ui.compact_layout`,
  gespeichert als `bar`, `cover`, `pill` oder `card`; unbekannte Werte fallen
  auf `Bar` zurück.

„Leiste“ ist die deutsche UI-Bezeichnung für `Bar`. Der gesamte Modus heißt
„Kompaktansicht“; dadurch kollidiert kein Variantenname mit dem Modusnamen.

Beide Werte werden sofort nach einer erfolgreichen Nutzeraktion persistiert.
Scheitert das Speichern eines Ansichtswechsels, kehrt das Fenster in den
vorherigen Modus zurück und zeigt einen Toast; der sichtbare und der gespeicherte
Zustand dürfen nicht auseinanderlaufen.
Beim nächsten Start stellt Reprise den letzten Fenstermodus und das letzte
Kompaktlayout wieder her. Die bestehende Sessionqueue wird weiterhin ohne
Autoplay als `Stopped` restauriert. Bei einem echten Erststart hat der
Einrichtungsdialog Vorrang: Vor abgeschlossenem Onboarding startet Reprise
immer in der Bibliotheksansicht.

## Schneller Wechsel

- `Ctrl+M` schaltet jederzeit zwischen Bibliotheks- und Kompaktansicht um.
- Die Bibliotheks-Headerbar zeigt keinen zusätzlichen Kompaktknopf; der Einstieg
  bleibt im Hauptmenü und über `Ctrl+M` erreichbar.
- Das Hauptmenü bietet „Kompaktansicht“ und das Kontextmenü der Kompaktlayouts
  „Zur Bibliothek“ als textliche Einträge derselben Umschaltaktion.
- Der Wechsel geschieht im bestehenden `AdwApplicationWindow`; es wird kein
  zweites Fenster und kein zweiter Player erzeugt.
- Track, Queue, Playbackstatus, Position, Lautstärke, Shuffle und Repeat ändern
  sich beim Ansichts- oder Layoutwechsel nicht. Laufende Wiedergabe wird nicht
  angehalten, neu geladen oder zurückgespult.

Die volle Bibliotheksgeometrie bleibt getrennt vom Kompaktmodus gespeichert.
Jedes Kompaktlayout setzt seine kuratierte natürliche Größe; eine gespeicherte
Bibliotheksgröße wird dadurch nie überschrieben. Eine Fensterposition wird
nicht vorgetäuscht: Unter Wayland entscheidet der Window Manager über die
Platzierung.

## Gemeinsames Kompaktmenü

Alle vier Layouts verwenden dasselbe `gio::MenuModel`, denselben daraus
gebauten `GtkPopoverMenu` und dieselben fenstergebundenen Actions. Der Popover
besitzt für die Lautstärke eine native benutzerdefinierte Zeile mit Regler:

- „Zur Bibliothek“
- „Layout“ als Radio-Untermenü mit Leiste, Cover, Pill und Card
- Shuffle an/aus
- Repeat Aus/Alle/Eins
- Lautstärke über diese native Popover-Zeile
- Einstellungen

Ein sichtbarer `open-menu-symbolic`-Knopf öffnet das Menü. Zusätzlich öffnet
Rechtsklick auf eine freie, nicht interaktive Fläche dasselbe Menü. `Menu` und
`Shift+F10` bieten den Tastaturpfad. Rechtsklick auf Seek-, Lautstärke- oder
Transportelemente wird nicht abgefangen.

Der Layout-Eintrag ist eine stateful String-Action. Das aktive Layout ist als
Radioauswahl erkennbar. Ein Wechsel schließt das Menü, komponiert die Oberfläche
sofort neu, passt die natürliche Fenstergröße an und persistiert erst dann den
neuen Wert. Scheitert die Persistenz, stellt Reprise das vorige Layout wieder
her und zeigt einen Toast.

## Gemeinsame Informations- und Bediengarantie

Jedes Layout zeigt immer:

- Cover oder den vorhandenen Platzhalter;
- Titel und Interpret mit Ellipsierung;
- Zurück, Play/Pause und Weiter;
- aktuellen Fortschritt und Gesamtdauer;
- eine bedienbare Seek-Fläche;
- den Menüknopf und „Zur Bibliothek“.

Shuffle, Repeat und Lautstärke bleiben in jedem Layout erreichbar. Wo der Raum
nicht für direkte Knöpfe reicht, liegen sie im gemeinsamen Menü. Tooltips,
Accessible Names, Tastaturfokus und mindestens die nativen Adwaita-Zielgrößen
bleiben erhalten. Keine Variante trägt Bedeutung ausschließlich durch Farbe.

## Layout Leiste (`Bar`)

Leiste ist der Standard und die direkte Weiterentwicklung der bestehenden
Minimalansicht. Sie ist ein horizontales Alltagslayout mit normaler schmaler
Headerbar.

- Links: 64-px-Cover, Titel und Interpret.
- Mitte: Shuffle, Zurück, Play/Pause, Weiter und Repeat; darunter Seek mit
  Zeitangaben.
- Rechts: Lautstärke, Layoutmenü und „Zur Bibliothek“.
- Alle Playbackfunktionen sind ohne Menü erreichbar.
- Zielgröße ungefähr 600 × 135 logische Pixel einschließlich Headerbar; die
  endgültige Mindestgröße folgt der gemessenen nativen Widgetanforderung.

## Layout Cover (`Cover`)

Cover ist ein ruhiges, hochformatiges Artwork-Layout mit normaler Headerbar.

- Großes quadratisches Cover mit möglichst 1:1 dargestelltem Artwork.
- Darunter Titel, Interpret und Album.
- Seek und Zeitangaben bilden eine eigene Zeile.
- Zurück, Play/Pause und Weiter sind zentriert sichtbar.
- Shuffle, Repeat und Lautstärke liegen im gemeinsamen Menü; aktive
  Shuffle-/Repeat-Zustände werden zusätzlich kompakt neben den Hauptcontrols
  angezeigt.
- Zielgröße ungefähr 360 × 500 logische Pixel.

## Layout Pill (`Pill`)

Pill ist die flachste Variante und bleibt ein opakes, normales GNOME-Fenster.
Es verwendet weder Transparenz noch Blur, Always-on-top oder Desktop-Dock-
Semantik.

- Eine einzige horizontale Zeile: 50-px-Cover, Titel/Interpret, Haupttransport,
  schmale Seek-Fläche, Menü und „Zur Bibliothek“.
- Die freie Metadatenfläche liegt in einem `GtkWindowHandle` und ist damit die
  native Drag-Fläche. Buttons und Slider gehören ausdrücklich nicht dazu.
- Fensteraktionen werden in die Zeile integriert; es gibt keine zweite
  Headerbar, die die charakteristische geringe Höhe wieder aufhebt.
- Shuffle, Repeat und Lautstärke liegen im gemeinsamen Menü.
- Zielgröße ungefähr 620 × 82 logische Pixel.

## Layout Card (`Card`)

Card ist die informationsreichere Kompaktvariante mit normaler Headerbar.

- Links ein 132-px-Cover.
- Rechts Titel, Interpret, Album und optional Jahr; fehlende Werte erzeugen
  keine leeren Platzhalterzeilen.
- Seek und Zeitangaben liegen unter den Metadaten.
- Zurück, Play/Pause und Weiter sind direkt sichtbar.
- Shuffle- und Repeatstatus sind direkt sichtbar und bedienbar; ein direkter
  `GtkScaleButton` hält auch die Lautstärke ohne Menü erreichbar.
- Zielgröße ungefähr 440 × 240 logische Pixel.

## Architektur

`minimal_view.rs` wird zu einem kleinen Zustandskoordinator für
`WindowViewMode`, `CompactLayout`, Geometrieschutz und Root-Wechsel. Layout- und
Menüaufbau werden in neue fokussierte Geschwistermodule extrahiert; die bereits
790 Zeilen große `window.rs` erhält nur schlanke Composition-Root-Aufrufe.

Der bestehende `PlayerController`, seine Queue und der Linux-Player bleiben
einzig. Die vollständige `PlayerBar` bleibt die Bibliotheksoberfläche. Eine neue
`CompactPlayer`-Oberfläche wird in die bereits vorhandenen zentralen `sync_*`-
Methoden aufgenommen, die heute `PlayerBar` und `NowPlayingView` gemeinsam
aktualisieren. Titel, Status, Position, Transport, Shuffle und Repeat werden so
von jedem bestehenden Controllerereignis an alle drei Oberflächen projiziert;
es entsteht kein zweiter Zustandsweg. Kompakte Nutzerabsichten rufen dieselben
Controller-Methoden wie Bar und Now-Playing auf. Eine versteckte Oberfläche darf
keine eigene Queue, keinen eigenen Positionstimer und keine zweite Coverpipeline
starten. Der eine vorhandene `CoverLoader` bedient ein eigenes
generation-geschütztes Kompakt-Coverziel.

Layoutwechsel innerhalb der Kompaktansicht verwenden einen `gtk::Stack` mit
vier klar getrennten Layoutwurzeln. Nur das aktive Kind ist sichtbar;
asynchrone Coverergebnisse bleiben über die bereits vorhandene Generation an
den aktuellen Track gebunden, nicht an eine Layoutinstanz. Layout-Callbacks
halten nur schwache Referenzen. Kein `RefCell`- oder SQLite-Borrow überlebt
einen GTK-, Action- oder Persistenzcallback.

## Start, Schließen und Fehlerfälle

Beim Fensteraufbau werden Sessionqueue und Bibliothekszustand wie bisher zuerst
restauriert. Danach wird bei abgeschlossenem Onboarding der persistierte
Fenstermodus angewendet, bevor das Fenster sichtbar wird. Ein restaurierter
Track ist geladen und bedienbar, bleibt aber gestoppt.

Schließen in der Kompaktansicht darf nicht zuerst künstlich in die
Bibliotheksansicht wechseln. Die Session speichert weiterhin die zuletzt
bekannte volle Bibliotheksgeometrie; der separate Moduswert bleibt `compact`.
Fehler beim Lesen persistierter Werte führen zu Leiste beziehungsweise
Bibliotheksansicht und einem Warnlog, nie zu einem Startabbruch.

Wenn ein Layout wegen einer unerwarteten GTK-Ressource nicht aufgebaut werden
kann, fällt die Kompaktansicht für diesen Lauf auf Leiste zurück. Playback läuft
weiter. Es gibt keine automatische Änderung der gespeicherten Auswahl ohne eine
erfolgreiche Nutzeraktion.

## Tests und Verifikation

- Core-Tests beweisen Roundtrip und sichere Fallbacks beider typisierten Werte.
- Reine Zustandsmaschinentests beweisen Toggle, Layoutwechsel, Erststart-
  Vorrang, Persistenzfehler-Rollback und getrennte Geometrie.
- Controller-/Projektionsregressionen beweisen, dass beide Oberflächen denselben
  Track-, Playback-, Seek-, Volume-, Shuffle- und Repeatstatus erhalten und ein
  Layoutwechsel keine Playeroperation auslöst.
- Je ein isolierter Displaytest prüft die Pflichtwidgets, Radioauswahl,
  Accessible Names und natürliche Größe jedes Layouts.
- Ein vollständig isolierter App-Smoke startet mit Fixture-Musik, spielt,
  wechselt Bibliothek → alle vier Kompaktlayouts → Bibliothek und beweist
  unveränderte Track-ID, Playing-Zustand und monotone Position.
- Ein Zwei-Start-Smoke schließt in Card, startet erneut in Card mit restauriertem
  Track und beweist `Stopped` ohne Autoplay.
- Der reale Xvfb-Pointerharness prüft Hauptmenü, `Ctrl+M`, sichtbaren
  Kompaktmenüknopf, Rechtsklick, Radioauswahl und Rückkehrknopf sowie saubere
  GTK-/GLib-/Panic-/`RefCell`-Logs.
- Vollständige Gates, Rustdoc, Core-Purity, gettext, Releasechecker und
  Dateigrößenregel bleiben verpflichtend.

## Manuelle native GNOME-Prüfung

Headless nicht als erledigt behauptet werden dürfen:

- tatsächliche Proportionen, Ellipsierung und Iconwirkung aller vier Varianten;
- Pill-Drag-Fläche und integrierte Fensteraktionen unter Wayland;
- Window-Manager-Platzierung und Größenwechsel;
- Touch-Ziele, HiDPI sowie Hell-/Dunkelmodus;
- subjektiv ruhige Coverdarstellung mit echten quadratischen und nicht
  quadratischen Bildern.

## Explizit nicht Teil

- Kein frei zusammenstellbarer Layouteditor und keine beliebig verschiebbaren
  Playerbausteine.
- Kein zweites Playerfenster, kein zweiter Playbackcontroller und keine
  unabhängige Kompaktqueue.
- Kein Always-on-top, Blur, Transparenz, schwebendes Overlay oder Trayplayer.
- Keine neuen Playbackfunktionen wie Crossfade, Lyrics oder Visualizer.
- Keine globale systemweite Tastenkombination; `Ctrl+M` gilt im aktiven
  Reprise-Fenster, Mediensteuerung bleibt MPRIS.
- Keine erzwungene Fensterposition unter Wayland.
