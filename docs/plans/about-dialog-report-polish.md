# About-Dialog: Debug-Bericht und Legal-Sektionen nachziehen

Nachlauf zu `about-dialog-libadwaita-standard` (`phase: shipped`, `ea4ebb7846`).
Die visuelle Abnahme am 12.08.2026 fand drei Abweichungen zwischen dem
gelandeten Stand und dem Zielbild des Plans — Belege und Messmethode in
`docs/plans/about-dialog-libadwaita-standard.VISUAL.md`, Bilder unter
`.tmp/about-visual/`.

Der Dialog selbst bleibt libadwaita-Standard. Es wird **nicht** in den
Widget-Baum gegriffen; alle drei Punkte sind Inhalt, nicht Layout.

## Schichtregel (vor dem ersten Schnitt lesen)

Der Diagnose-Kern ist rein und liegt in `reprise-core` (Struktur + Renderer,
kein I/O). Die unreine Sammelseite liegt in `reprise-platform-linux`. Die
GNOME-Schicht speist nur den Ringpuffer und baut den Dialog —
`scripts/check-frontend-thinness.sh` verbietet dort Dateisystemzugriffe und
misst gegen eine Baseline. Beim vorigen Lauf hat genau dieser Schnitt einen
kompletten Durchgang gekostet.

## Aufgabe 1 — Die Warnzeilen brechen vierfach um

Gerendert wird heute das volle Rust-Modul als Ziel:

```
19:16:19 reprise::ui::track_list::track_list_menu_smoke: REPRISE_SMOKE_MENU_ACTION: unrecognized value; ignoring; value=$REDACTED
```

Im Dialog sind das vier umgebrochene Zeilen **pro Ereignis**; vier Warnungen
füllen zwei Drittel der sichtbaren Fläche, die vorgesehenen zehn wären rund
vierzig Zeilen. Das Zielformat des Plans war ein kurzes Ziel:

```
09:25:14 mtp: device detached mid-transfer, run 41 cancelled
```

**Zu tun:** beim Rendern nur das letzte `::`-Segment des Targets ausgeben
(`track_list_menu_smoke:`). Der Ringpuffer speichert weiter, was `tracing`
liefert — gekürzt wird in der Ausgabe, damit die Rohdaten nicht verlieren.

Randfälle, die Tests brauchen: Target ohne `::`, leeres Target, Target das auf
`::` endet. Bestehende Redaktions- und Reihenfolge-Tests müssen grün bleiben.

## Aufgabe 2 — `os manjaro unknown` auf Rolling-Distributionen

`/etc/os-release` hat auf Manjaro `ID=manjaro` und `BUILD_ID=rolling`, aber
**kein** `VERSION_ID`. Die Zeile liest sich dadurch für jeden Arch-, Manjaro-
oder Gentoo-Nutzer wie ein Fehler.

**Zu tun:** fehlt `VERSION_ID`, auf `BUILD_ID` ausweichen (`os manjaro rolling`).
Fehlt auch das, nur den Distributionsnamen ausgeben (`os manjaro · gnome 50.4 ·
x11`) — kein angehängtes `unknown` hinter dem Namen.

Das betrifft ausschließlich die Distributionsversion. Die übrige Regel des
Plans bleibt: ein Wert, der gar nicht ermittelbar ist, rendert weiterhin
`unknown`, und keine Zeile verschwindet.

Tests auf beiden Seiten: die Sammelseite liest die Ersatzfelder, der Renderer
lässt das Versions-Token weg, wenn keine Version da ist.

## Aufgabe 3 — Die Gewährleistungsformel steht siebenmal untereinander

Jede Legal-Sektion bekommt heute `copyright: None`, also rendert libadwaita
unter **jeder** Komponente denselben Satz „This application comes with
absolutely no warranty. See the GNU Lesser General Public Licence…". Siebenmal
dieselbe Aussage über *diese* App, obwohl die Sektion je eine Fremdbibliothek
ausweist.

**Zu tun:** die Fremdkomponenten wie SQLite behandeln — eine kurze eigene
Zeile je Sektion statt der Formel. SQLite zeigt heute sauber nur „Public
Domain", weil es mit einer benannten Lizenz statt eines Lizenztyps gebaut wird;
dieselbe Bauart auf die übrigen Sektionen anwenden, mit dem Lizenznamen als
Text (etwa „LGPL 2.1 or later", „MIT").

**Bewusster Tausch, den der Nutzer am Diff abnehmen soll:** die Sektionen
verlieren dabei den anklickbaren Lizenzlink. Die eigene Lizenz der App
(GPL 3.0 or later, oberster Block) bleibt unangetastet und behält ihren Link.
Wenn sich die Wiederholung ohne diesen Verlust auflösen lässt, ist das der
bessere Weg — dann begründen, warum.

## Nachweis

- `cargo test -p reprise-core` und `cargo test -p reprise-platform-linux`
- `cargo test -p reprise-gnome --bin reprise` (in diesem Crate greift `--lib`
  nicht, es liefe sonst kein einziger Test)
- `cargo clippy --workspace --all-targets -- -D warnings`
- `scripts/check-frontend-thinness.sh`

Display-Tests (`#[ignore]`, brauchen einen X-Server) laufen headless im Sandbox
nicht — sie gehören nicht in die Erfolgsmeldung. Was nicht gelaufen ist, wird
als nicht gelaufen berichtet.

Nicht committen: `.pipeline-*.md` (Auftrags- und Ergebnisdateien der Pipeline).
