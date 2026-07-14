# Manuelle „Als Nächstes“-Queue und GNOME-Compact-Redesign — Design

## Ziel

Reprise trennt den normalen Wiedergabekontext von der sichtbaren Queue:

- Der **Wiedergabekontext** bleibt die beim Start gewählte Bibliothek,
  Playlist, Smart Playlist oder gefilterte Ansicht. Er bestimmt, wo Reprise
  nach einem Titel normalerweise weiterspielt.
- **Queue / Als Nächstes** enthält ausschließlich Titel, die der Nutzer
  bewusst über „Zur Queue hinzufügen“ vorgemerkt hat. Der aktuelle Titel,
  der restliche Wiedergabekontext und bereits gespielte Titel erscheinen dort
  nicht.

Zusätzlich werden Cover, Pill und Card näher an die bereitgestellte
Vorlage gebracht, bleiben aber vollständig native, opake GTK4/libadwaita-
Oberflächen. Es gibt kein Blur, keine Transparenz, kein Always-on-top, kein
magnetisches Andocken und keine Hover-Vergrößerung des Fensters.

## Queue-Semantik

Ein Doppelklick in Bibliothek, Playlist oder Smart Playlist startet weiterhin
den gewählten Titel und merkt sich die vollständige aktuelle Ansicht als
Wiedergabekontext. Dieser Kontext bleibt intern und wird nicht als Queue
angezeigt.

„Zur Queue hinzufügen“ hängt Titel in stabiler Nutzerreihenfolge an „Als
Nächstes“ an. Duplikate sind erlaubt. Die sichtbare Queue und ihre Sidebar-
Zahl zeigen nur diese noch ausstehenden manuellen Einträge.

Beim natürlichen Titelende oder bei „Weiter“ gilt:

1. Repeat One wiederholt den aktuellen Titel nur bei natürlichem Ende.
2. Andernfalls wird zuerst der vorderste manuelle Queue-Eintrag verbraucht.
3. Ist „Als Nächstes“ leer, fährt Reprise im unveränderten
   Wiedergabekontext nach dessen Shuffle-/Repeat-Regeln fort.
4. Existiert weder Kontext noch manueller Eintrag, stoppt Reprise.

Der gestartete manuelle Titel verschwindet sofort aus „Als Nächstes“. „Zurück“
während eines manuell eingeschobenen Titels kehrt zum unveränderten aktuellen
Kontexttitel zurück; bereits verbrauchte manuelle Titel werden nicht erneut
eingereiht. Das ist bewusst eine kommende Liste und keine vollständige
Wiedergabechronik.

Ein Doppelklick auf einen Queue-Eintrag spielt diesen sofort. Der ausgewählte
Eintrag und alle davor liegenden Einträge gelten als verbraucht; spätere
Einträge bleiben in ihrer Reihenfolge ausstehend. Drag-and-drop ordnet nur die
manuelle Queue. Ein Queue-spezifischer Kontextmenüeintrag entfernt die
ausgewählten Einträge, ohne Musikdateien oder den Wiedergabekontext anzutasten.

Das Starten eines neuen Bibliotheks-/Playlist-Kontexts beendet einen eventuell
gerade eingeschobenen manuellen Titel, behält aber die noch ausstehenden
manuellen Einträge. Dadurch bleiben bewusst vorgemerkte Titel erhalten.

## Session und Kompatibilität

Die bestehende `Queue` in `reprise-core` bleibt der pure Wiedergabekontext für
Sortierung, Shuffle, Repeat und dessen aktuellen Index. Ein neuer kleiner
`UpNextQueue`-Typ besitzt ausschließlich die manuell ausstehenden IDs und ihre
Reihenfolge.

Der bestehende Sessionwert wird rückwärtskompatibel um `up_next` und
`current_up_next` ergänzt; fehlende Felder alter Sessions sind leer. Damit
wird eine bislang als vollständige Queue gespeicherte Bibliothek nach dem
Upgrade automatisch nur noch als interner Kontext restauriert, während die
sichtbare Queue leer ist. Ein beim Schließen laufender manueller Titel wird
als aktueller Titel geladen, bleibt beim Neustart aber wie bisher `Stopped`
und startet nie automatisch.

Vor der Wiederherstellung werden Kontext, manueller aktueller Titel und
ausstehende IDs gegen die Datenbank validiert. Fehlende IDs werden verworfen.
Die Session bleibt begrenzt; beschädigte neue Felder degradieren leer, nicht
zu einem Startabbruch.

## Controller-Architektur

`PlayerController` bleibt der einzige Player und erhält neben seinem
`RefCell<Queue>` genau eine `RefCell<UpNextQueue>` sowie den optionalen aktuell
laufenden manuellen Titel. Die Auswahl des nächsten Titels lebt in einem
fokussierten Geschwistermodul; `player_controller.rs` wächst wegen seines
Dateilimits nicht mit neuer Ablaufsteuerung.

Alle Transportpfade — natürlicher Abschluss, Buttons, Compact, Space, MPRIS
und Fehler-Skip — verwenden dieselbe Kandidatenauswahl. Queue-Badge,
Queue-Ansicht und Session werden nach jeder manuellen Mutation oder
Verbrauchsoperation benachrichtigt. Kein `RefCell`-Borrow überlebt einen GTK-,
Player-, MPRIS- oder Callback-Aufruf.

Shuffle verändert nur den Wiedergabekontext; manuell eingereihte Titel bleiben
in der vom Nutzer festgelegten Reihenfolge. Repeat All greift nach dem
Verbrauchen der manuellen Einträge wieder auf den Kontext. Repeat One lässt
ausstehende Einträge unberührt.

## Compact-Menü und Lautstärke

Alle drei Layouts behalten denselben sichtbaren Drei-Punkte-Knopf sowie
Rechtsklick, `Menu` und `Shift+F10` als Zugänge zum gemeinsamen nativen
`GtkPopoverMenu`. Es enthält:

- „Zur Bibliothek“;
- Layout: Cover, Pill, Card;
- Shuffle;
- Repeat Aus/Alle/Eins;
- Einstellungen.

„Zur Bibliothek“ existiert in Compact ausschließlich in diesem Menü. Die drei
sichtbaren Restore-Buttons entfallen. Hauptmenü und `Ctrl+M` bleiben als schnelle
Wege **in** beziehungsweise aus Compact erhalten; die Bibliotheks-Headerbar
dupliziert den Menüeintrag nicht mit einem eigenen Knopf.

Es gibt in Compact keinen sichtbaren Lautstärkeregler und keine Lautstärkezeile
im Kontextmenü. Vertikales Mausrad-/Touchpad-Scrollen auf freien Cover- oder
Metadatenflächen ändert die Lautstärke in Schritten von fünf Prozent und nutzt
denselben Controller-/MPRIS-Synchronisationspfad wie die Bibliotheksleiste.
Seek, Transport, Menü und Fensteraktionen sind keine Lautstärke-Scrollflächen;
Scrollen darüber darf weder Seek noch Lautstärke unbeabsichtigt verändern.
Die Lautstärke bleibt außerdem über MPRIS, Systemmedientasten und die volle
Bibliotheksleiste erreichbar.

## Layouts

### Cover (`Cover`)

Eine hochformatige Artwork-Ansicht mit großem quadratischem Cover. Titel,
Interpret und optional Album sind zentriert, darunter folgen Zeit/Seek und die
drei Haupttransportknöpfe. Shuffle, Repeat und Rückkehr liegen im Menü.

### Pill (`Pill`)

Eine einzige opake horizontale Zeile mit kleinem Cover, ellipsierten
Titel-/Interpret-Metadaten, Zurück/Play/Weiter, kompakter Zeit/Seek-Zeile und
Menü. Nur die freie Metadatenfläche ist ein `GtkWindowHandle`; Controls bleiben
außerhalb. Native `GtkWindowControls` bleiben für den bestehenden CSD-Modus
integriert und verschwinden beim gespeicherten Systemdekoration-Modus.

### Card (`Card`)

Links steht ein größeres Cover, rechts Titel, Interpret, optional Album/Jahr,
Zeit/Seek und die Haupttransportknöpfe. Sekundärfunktionen, Rückkehr und
Layoutwahl liegen im Menü. Die Card bleibt eine normale native Fensterfläche.

Alle Layouts verwenden Adwaita-Abstände, native Fokus-/Zielgrößen,
Ellipsierung, Tooltips und Accessible Names. Keine Bedeutung hängt nur von
Farbe ab. Die bestehende Client-/Systemdekorationseinstellung projiziert
weiterhin auf Bibliothek und alle Compact-Wurzeln.

## Fehlerfälle

- Ein Fehler beim Persistieren eines Layout-/Moduswechsels behält den bisher
  sichtbaren Zustand und zeigt den bestehenden Toast.
- Ein ungültiger oder gelöschter Queue-Titel wird verworfen und der nächste
  sichere Kandidat versucht; die Skip-Schleife bleibt begrenzt.
- Scroll-Ereignisse ohne endliche Richtung oder ohne registrierte freie
  Fläche sind No-ops.
- Eine fehlende Playback-Plattform deaktiviert Controls wie bisher; Queue-
  und Layoutansichten dürfen trotzdem nicht paniken.

## Tests und Verifikation

- Pure Core-Tests prüfen Append, Duplikate, Verbrauch, Prefix-Verbrauch,
  Entfernen, Reorder, Purge und Grenzen von `UpNextQueue`.
- Session-Tests prüfen alte JSON-Werte ohne neue Felder, Roundtrip,
  Größenbegrenzung und `Stopped`-Wiederherstellung eines manuellen Titels.
- Controller-Tests prüfen manuell-vor-Kontext, Resume, Repeat One, Shuffle-
  Stabilität, Previous, leeren Kontext und Fehler-Skip.
- Track-List-Tests prüfen Queue-Aktivierung, Queue-spezifisches Entfernen,
  Badge/Ansicht und DnD ausschließlich gegen „Als Nächstes“.
- Drei isolierte Displaytests prüfen neue Pflichtwidgets, fehlende sichtbare
  Restore-/Volume-Controls, natürliche Größe und Dekorationsprojektion.
- Ein isolierter App-Smoke beweist: Bibliothekskontext mit mehreren Titeln,
  sichtbare Queue zunächst 0, zwei manuelle Titel sichtbar, Abspielreihenfolge
  Kontext A → manuell X → manuell Y → Kontext B, Queue-Zahl 2 → 1 → 0.
- Der reale Pointerharness prüft Compact-Menü, kontextgebundene Rückkehr,
  Scroll-Lautstärke, Layoutwechsel und saubere GTK-/GLib-/Panic-/Borrow-Logs.
- Vollständige Gates, Rustdoc, Audit, Core-Purity, gettext,
  `scripts/check-release.sh`, optimierter Meson-Install und Dateigrößen gelten.

## Manuelle native GNOME-Prüfung

- Proportionen und visuelle Ruhe aller drei Layouts mit echten Covern;
- Mausrad und Touchpad auf freien gegenüber interaktiven Flächen;
- Pill-Drag und CSD/SSD-Umschaltung unter Wayland;
- Hell/Dunkel, HiDPI, Tastatur, Touch und lange deutsche/englische Metadaten;
- subjektive Queue-Nutzung mit einem längeren Bibliothekskontext.

## Explizit nicht Teil

- Kein Blur, keine Transparenz, kein Always-on-top, kein Docking und keine
  Hover-/Spring-Vergrößerung des Fensters.
- Kein frei konfigurierbarer Layouteditor, kein zweites Playerfenster und
  kein zweiter Playbackcontroller.
- Keine vollständige History-Ansicht; „Als Nächstes“ zeigt nur ausstehende
  manuelle Titel.
- Kein neuer Favoriten-/Ratingknopf in Compact.
- Keine Änderung oder Löschung von Musikdateien.
