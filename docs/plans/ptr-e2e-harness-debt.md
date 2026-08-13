---
slug: ptr-e2e-harness-debt
worktree: /home/marvin/Projects/reprise-ptr-e2e-harness-debt
branch: feature/ptr-e2e-harness-debt
phase: planned
codex_session:
created: 2026-08-12
---
# Die ptr-e2e-Harness wieder ehrlich machen

**Goal:** Die 14 dauerhaft roten Checks von `scripts/ptr-e2e/run.sh` abarbeiten.
Sie sind keine gemeinsame Ursache: ein echter Produktfehler, mehrere verschwundene
`tracing`-Meldungen und drei veraltete Koordinaten. Ein Harness, der immer rot
ist, wird nicht gelesen — und deckt dann echte Regressionen zu.

**Baseline:** Lauf vom 12.08.2026, Log unter `/tmp/reprise-ptr-e2e/app.log`,
Screenshots unter `/tmp/reprise-ptr-e2e/*.png`. Das Onboarding-Banner ist bereits
erledigt (siehe `track-list-selection-anchor.HANDOFF.md`); die Ansicht ist
bannerfrei, Zeilen liegen bei y≈175/220/265/310/355 (45 px Abstand) im
1600x900-Fenster.

## Paket A — der echte Fehler: Shift+F10 gibt der Tastatur den Fokus nicht

**Das ist ein Barrierefreiheits-Defekt, kein Testproblem.** `Shift+F10` öffnet das
Kontextmenü der Track-Liste (`track_list_context_keys.rs:41-47`), ruft aber nur
`focus_guard.restore_on_popover_close(...)`. Es fehlt das Gegenstück
`focus_guard.bind_popover(&popover, &initial_focus)`, das
`table_columns/header_popover.rs:63-64` beim Spaltenmenü sehr wohl macht (dort
folgt `initial.grab_focus()` in `connect_map`).

Folge im echten Betrieb: Das Menü erscheint, aber `Pfeil runter` und `Enter` gehen
daran vorbei an die Liste. Der Beweis steht im Lauf-Log — nach
`track context menu opened from keyboard` (`app.log:265`) erzeugte die Tastenfolge
`Down; Down; Return` ein `activate track path=…/sine_05.flac`
(`track_list_activation.rs:76`), also GTKs Zeilenaktivierung, nicht die Menüaktion.
Wer das Menü nur mit der Tastatur bedient, kommt an seine Einträge nicht heran.

- [ ] Den Fokus beim Öffnen per Tastatur in den Popover binden, nach dem Muster
      von `header_popover.rs`.
- [ ] Regressionstest, dessen Name die zuständige Regel trägt (ACC-8-Umfeld:
      `scripts/check-input-parity.sh` verlangt für jeden Zeiger-Seam einen echten
      Tastatur-Partner). Der Test muss belegen, dass nach dem Öffnen ein Eintrag
      *im Popover* den Fokus hat — nicht, dass der Popover sichtbar ist.
- [ ] Prüfen, ob dieselbe Lücke andere per Tastatur geöffnete Popover betrifft.

Dieser eine Fehler verursacht als Kaskade die Checks 9, 10, 11, 13 und 14: Weil
`Enter` in der Liste landet statt im Menü, kippt danach die Auswahl (ein `Ctrl+A`
markierte alle fünf Zeilen), und alles Folgende misst Müll.

## Paket B — Meldungen, auf die das Harness wartet, die es nicht mehr gibt

Drei Zusicherungen können heute von keinem Codepfad erfüllt werden. Das ist die
gefährlichste Sorte roter Test: Sie sieht nach kaputtem Produkt aus, ist aber nur
fehlende Beobachtbarkeit.

- [ ] `column header visibility changed` — kein Treffer in `crates/`.
      `table_columns/registry.rs:392-399` (`set_visible`) loggt auf dem Erfolgspfad
      nichts. Entweder eine `tracing`-Zeile mit `column` und `visible` ergänzen
      oder das Harness auf die DB-Prüfung allein umstellen.
- [ ] `tracks added to queue` — existiert nur noch als UI-Toast-Vorlage
      (`strings.rs:579`). Der reale Pfad `track_list_queue_menu.rs:30-38`
      delegiert an `sidebar_dnd.rs:213` und `queue_insertion.rs:27`, das
      `items added to queue` mit `added`/`queue_len` loggt. Harness-Muster
      nachziehen.
- [ ] `sidebar refresh … up next changed` — die Queue-Änderung geht heute über
      `Sidebar::refresh_queue_count()` (`sidebar.rs:424-433`), das nur das Label
      setzt und gar nicht loggt. Entweder dort eine `tracing`-Zeile ergänzen oder
      das Muster auf `up next changed` verkürzen, das tatsächlich feuert.

Entscheidungsregel: Wo die Meldung echten Diagnosewert hat, die Meldung ergänzen.
Wo sie nur für den Test existierte, das Harness auf die DB-Zusicherung umstellen —
keine `tracing`-Zeilen einbauen, die nur ein Test liest.

## Paket C — drei veraltete Koordinaten

- [ ] **Rating-Flow neu schreiben.** `rating.rs:1-16`: Das Widget wurde von
      „Knopf öffnet Popover" auf ein **inline eingeblendetes Fünf-Sterne-Feld ohne
      Popover** umgebaut (`2b8aa721bc`). `scripts/ptr-e2e/rating.sh:19-22` klickt
      noch den Knopf und dann in einen Popover, den es nicht mehr gibt. Der Flow
      muss stattdessen über die Zelle fahren (`mousemove` ohne Klick, damit das
      Einblenden auslöst) und dann einen der Inline-Sterne treffen.
      `ROW0_RATING_BUTTON_*` und `ROW0_RATING_POPOVER_STAR2_*` aus `geometry.sh`
      entfernen — das Modell existiert nicht mehr.
- [ ] **`COLUMN_HEADER_Y=120`** trifft das Spaltenkopfband nicht. Im bannerfreien
      Fenster liegt die Kopfzeile bei y≈136 (Filterleiste darüber bei y≈87).
      Nachmessen und setzen; `header_popover.rs` gattert über `is_header_click`
      mit dynamisch gemessener Kopfhöhe, es gibt also keine feste Gegenprobe im
      Code.
- [ ] **`SIDEBAR_PLAYLIST_DELETE_Y=282`** trifft den **ersten** Menüeintrag.
      `sidebar_export.rs:44-54` ordnet `[Export playlist…, Delete playlist…]`; das
      Log beweist es (`sidebar_export: export playlist dialog dismissed`,
      `app.log:262`). Auf den zweiten Eintrag nachmessen —
      `03c-playlist-context-menu.png` zeigt den echten Abstand.

## Paket D — der Queue-Flow misst auf einem fahrenden Zug

Die Fixture-Tracks sind ~1,16 s lang. Während der Queue-Schritte läuft die
Wiedergabe weiter und schiebt `up_next` von 5 auf 1 herunter (`app.log:286-401`),
also zeigen `QUEUE_ROW0_*`/`QUEUE_ROW1_*` mitten im Zug auf andere Zeilen.

- [ ] Vor den Queue-Schritten die Wiedergabe anhalten, oder für dieses Segment
      eine längere Fixture verwenden. Erst nach Paket A angehen — vorher ist die
      Auswahl ohnehin verfälscht.

## Reihenfolge

A → B → C → D, und nach jedem Paket `scripts/ptr-e2e/run.sh` unter
`heavy-run medium` laufen lassen. Die Kaskaden lösen sich von selbst auf: A allein
sollte 6 Checks grün machen. Nicht mehrere Pakete gleichzeitig messen — sonst ist
wieder unklar, was was repariert hat.

**Nicht abschwächen.** Keine Zusicherung entschärfen, damit sie grün wird. Wenn
ein Check nicht erfüllbar ist, ist entweder das Produkt kaputt (Paket A) oder die
Beobachtbarkeit fehlt (Paket B) — beides wird behoben, nicht weggeschrieben.

---

## Stand 12.08.2026, 17:38 — Paket A ist NICHT erledigt

> **Überholt am 12.08.2026, 19:03.** Die Schlussfolgerung dieses Abschnitts ist
> falsch. Sie beruht auf einer Log-Zeile, die dem falschen Skriptschritt
> zugeordnet wurde. Siehe „Korrektur" ganz unten.

Worktree `/home/marvin/Projects/reprise-ptr-e2e-harness-debt`, Branch
`feature/ptr-e2e-harness-debt`, drei Commits:

- `21524015e8` Banner-Abweisung (aus dem NAV-17-Branch übernommen)
- `e1c7c9e2ab` Paket A: `bind_popover` in `track_list_context_keys.rs`
- `080b78e7ab` Paket B: die drei Meldungen

Gemessen: 14 → **13** rote Checks. Paket B hat gewirkt (der Sidebar-Zähler ist
grün). **Paket A hat im echten Lauf nichts geändert.**

Der Beweis, `/tmp/reprise-ptr-e2e/app.log:154-155`:

```
15:37:24.264  DEBUG track_list_context_keys: track context menu opened from keyboard
15:37:25.710  INFO  track_list_activation: activate track path=…/sine_05.flac
```

Also weiterhin: Menü auf, dann landet `Runter, Runter, Enter` in der Liste statt im
Popover. Zwischen Öffnen und Tastendruck liegen 1,45 s — an zu frühem Zugriff
liegt es nicht.

**Das eigentliche Problem: der Display-Test ist ein falsches Grün.** Der in
`e1c7c9e2ab` ergänzte Test behauptet, der Fokus liege nach dem Öffnen im Popover,
und er ist grün — während echte Eingabe das Gegenteil zeigt. Damit gehört er in
exakt die Fehlerklasse, für die `scripts/ptr-e2e/` überhaupt existiert: Der
Signal-Seam-Test greift den Handler direkt ab, echte Eingabe nimmt einen anderen
Weg.

Nächster Schritt ist **keine weitere blinde Reparatur**, sondern eine Diagnose:

- [ ] Warum divergieren Test und Wirklichkeit? Der Test ruft vermutlich
      programmatisch auf, was unter echter Eingabe nie passiert. Erst den Test so
      umbauen, dass er rot wird, solange die echte Eingabe scheitert.
- [ ] Prüfen, ob `bind_popover` seinen `grab_focus` überhaupt erreicht — greift
      `connect_map`, und behält der Popover den Fokus, oder holt ihn die
      `ColumnView` zurück? Ein Backtrace im Fokus-Handler ist verlässlicher als
      ein Reproduktionsversuch.
- [ ] Alternativhypothese: Der Popover ist gar nicht der Fokus-Empfänger, sondern
      GTK leitet Tasten weiter an das Widget, das den Grab hält.

Bis das geklärt ist, bleiben die Kaskaden-Checks (Edit tags, Queue-Adds,
Queue-Drag) rot — sie hängen alle an Paket A. Paket C (Koordinaten) und D
(fahrender Zug) sind davon unabhängig und noch offen; Paket C braucht jemanden,
der die Screenshots ansehen kann.

---

## Korrektur 12.08.2026, 19:03 — Paket A wirkt; die Harness zählt falsch

Diagnose aus demselben Lauf, diesmal mit den Screenshots statt nur mit dem Log.
**Es gibt keinen Fokus-Defekt mehr und kein falsches Grün.**

Die drei Belege:

1. `03-keyboard-context-menu.png` zeigt das offene Menü **mit dem Fokusring auf
   „Play next"**. `bind_popover` greift also unter echter Eingabe.
2. `04-keyboard-tag-editor.png` — aufgenommen *nach* `Runter, Runter, Enter` —
   zeigt das geöffnete Untermenü **„Add to playlist"**. Die Tastenfolge ist im
   Menü angekommen und hat dort exakt das getan, was sie tun muss.
3. Flow 3 belegt es ein zweites Mal: `Shift+F10, Runter, Enter` erzeugt
   `items added to queue` (`app.log:193`). Der Menüeintrag wurde per Tastatur
   ausgelöst.

**Die eigentliche Ursache ist ein Zählfehler in `run.sh`.** Das Menü lautet
`Play next, Add to queue, | Add to playlist ▸, Edit tags…, …`. Seit der Popover
seinen ersten Eintrag fokussiert, führen zwei `Runter` auf „Add to playlist",
nicht auf „Edit tags…" — dafür braucht es drei. Die Harness stammt aus der Zeit
davor und wurde nie nachgezogen.

**Die Log-Zeile war falsch zugeordnet.** Das `activate track` um 15:37:25.710
stammt nicht aus der Menünavigation, sondern aus dem übernächsten Schritt: Der
fehlgeschlagene „Edit tags"-Schritt lässt den Popover offen, `click_at 800 466`
schließt ihn, der Fokus fällt auf die Zeile zurück, `Ctrl+A` markiert alle fünf,
und das `Return` des Jahr-Schritts startet die Wiedergabe. Der Log beweist es
mit `queue set from view queue_len=5 start_index=0` direkt dahinter.

- [x] Off-by-one in `run.sh` behoben (drittes `Runter` plus Begründung).
- [ ] Neu messen. Erwartung: Flow 2 wird grün, und die Kaskade in Flow 3
      (`queue_len=1` / `queue_len=2`) löst sich, weil keine Mehrfachauswahl mehr
      entsteht.
- [ ] Der Display-Test aus `e1c7c9e2ab` bleibt, wie er ist — er hat nicht
      gelogen. Der Verdacht gegen ihn ist ausgeräumt.

Methodischer Punkt für das nächste Mal: Bei dieser Harness ist `app.log` allein
kein Zuordnungsbeweis. Die Schritte liegen Millisekunden auseinander, und eine
Zeile landet leicht beim falschen Schritt. Die Screenshots zwischen den Schritten
sind die eigentliche Zeitachse.

### Paket C ist ausgemessen — die Zahlen stehen jetzt hier

Aus denselben Screenshots, damit Codex nicht mehr auf Augen warten muss. Alle
Angaben fensterrelativ im 1600×900-Fenster ohne Banner.

| Konstante | alt | neu | Beleg |
|---|---|---|---|
| `COLUMN_HEADER_Y` | 120 | **136** | Kopfband „Title/Artist/…" liegt bei y≈136; Zeile 0 beginnt erst bei y≈153 |
| `SIDEBAR_PLAYLIST_DELETE_Y` | 282 | **305** | `03c`: „Export playlist…" y≈261, „Delete playlist…" y≈305, Raster 44 px. 282 fällt um 2 px in den Export-Eintrag |
| `ROW0_RATING_BUTTON_*`, `ROW0_RATING_POPOVER_STAR2_*` | — | **löschen** | kein Popover mehr |
| Inline-Sterne Zeile 0 | — | **x = 1520, 1536, 1552, 1568, 1584 bei y = 175** | pixelgemessen an `02-rating-chooser.png` (Teal-Läufe 1518–1523, 1534–1539, 1550–1555; Raster 16 px) |

### Und dabei fiel der echte Seam-Fehler auf: der Stern-Klick committet nicht

Der bisherige Flow klickt (1548, 170) — das ist mitten in Stern 3 des
Inline-Felds. Beobachtet:

- `02-rating-chooser.png` zeigt daraufhin drei gefüllte Sterne, aber
  `rating changed` (`rating_column.rs:158`) steht **nirgends** im Log.
- `02-after-star-click.png`, aufgenommen nachdem der Zeiger die Zelle verlassen
  hat: **kein einziges Teal-Pixel** in der Rating-Spalte. Die drei Sterne waren
  reine Hover-Vorschau, die Bewertung wurde nie übernommen.
- Grün ist das trotzdem: `rating.rs:363` bietet einen „Test-only seam", der die
  Sterntaste per `emit_clicked` drückt, und `click_sets_rating_to_star_value`
  läuft darüber.

Das ist genau die Fehlerklasse, die fälschlich `Shift+F10` angehängt wurde —
sie sitzt in Wahrheit hier. Noch nicht getrennt sind zwei Ursachen: Entweder
nimmt das Produkt einen echten Zeigerklick nicht an, oder die Presse trifft ein,
bevor das eingeblendete Sternfeld allokiert ist. Der neu geschriebene
Rating-Flow entscheidet das von selbst, wenn er in dieser Reihenfolge fährt:
`mousemove` auf die Zelle → kurz setzen lassen → Klick auf Stern 2 (x=1536).
Bleibt es rot, ist es das Produkt.

---

## Stand 12.08.2026, 19:45 — Lauf 14 → 8 rot, und die Suite läuft erstmals durch

Paket C und D sind implementiert (Codex, `762e22c279` und `79286372f7`), dazu
vier Korrekturen von Hand. Gemessener Lauf: **8 rote Checks, 12 grüne.**

### Was grün wurde

- **Flow 2, „keyboard context-menu navigation opened Edit tags"** — damit ist
  Paket A ein zweites Mal bestätigt, diesmal durch die Harness selbst.
- **Flow 1c, Playlist löschen** (vier Checks) — `SIDEBAR_PLAYLIST_DELETE_Y=305`
  trifft.
- **Flow 3, beide Queue-Adds plus Sidebar-Zähler** — die Kaskade ist aufgelöst,
  `queue_len=1` und `queue_len=2` stimmen jetzt exakt.
- **Paket D** — die Wiedergabe ist vor den Queue-Schritten nachweislich still.

### Vier Korrekturen von Hand, die dazugehören

1. Das dritte `Runter` in Flow 2 (der eigentliche Paket-A-Fix).
2. Codex' Paket D prüfte `applying state change.*state=Stopped`, also einen
   Zustands*wechsel*. Seit die Kaskade weg ist, spielt vor Flow 3 nichts mehr,
   ein ruhender Player feuert kein Ereignis, und der Check wäre grundlos rot
   geworden. Er prüft jetzt den Ergebniszustand über MPRIS `PlaybackStatus`.
3. Codex' DB-Zusicherung im Rating-Flow nannte eine Spalte `missing`, die es in
   `tracks` nicht gibt (nur `missing_since`).
4. **`assert_db_query_true` riss bei kaputter Abfrage die ganze Suite mit.**
   Unter `set -e` beendete der `sqlite3`-Fehler den Lauf mitten in Flow 1, und
   die Schlusszeile meldete trotzdem „1 failed check" für eine Suite, die nie
   gelaufen war. Eine fehlgeschlagene Abfrage macht jetzt diesen einen Check
   rot. Das war eine eigene Ehrlichkeitslücke der Harness, unabhängig vom
   Spaltenfehler.

### Die acht roten Checks zerfallen in drei Gruppen

**Gruppe 1 — Bewertung per Zeigerklick kommt nicht an (2 Checks).** Bestätigt
mit Hover, Setzzeit und Klick auf Stern 2: `rating changed` fehlt, und in der
Scratch-DB stehen alle fünf Tracks auf `rating = 0`. Die zwei gefüllten Sterne
im Screenshot sind Hover-Vorschau. Im Produkt ist die Verdrahtung sauber —
`build_star` hängt `connect_clicked` direkt an `handle_star_activated` —, der
Klick erreicht die Taste also gar nicht. Nächster Schritt ist ein `pick()`-Raster
über die Rating-Zelle statt weiterer Vermutungen; der Verdacht ist die
Zeilen-Maschinerie der `ColumnView` in der Capture-Phase, also genau das, was
das Modul-Kommentar in `rating.rs` für die Vorgängerversion beschreibt.

**Gruppe 2 — der Tag-Editor meldet sich, erscheint aber nicht (1 Check).** Neu
und vorher unsichtbar, weil Flow 2 nie so weit kam. `tag_edit_flow.rs:324` und
`tag_editor.rs:206` loggen beide „presented", und im Screenshot 400 ms später
ist **kein Dialog** zu sehen (`scrot` nimmt den ganzen Bildschirm auf, es kann
also nicht an einem fremden Fenster liegen). Danach fehlt jede Reaktion auf die
Jahr-Eingabe, und Flow 3 bedient die Track-Liste unmittelbar weiter — ein
modaler Dialog steht dort also nicht. Präsentiert wird auf `&window`, nicht auf
den sterbenden Popover; die Ursache ist damit offen. Ein AT-SPI-Baum in genau
diesem Zustand ist das nächste Werkzeug.

**Gruppe 3 — noch nicht nachgemessene Koordinaten (5 Checks).** Spaltenkopf-Menü
(3) und Queue-Drag (2). Beim Kopfmenü ist `COLUMN_HEADER_Y=136` jetzt richtig,
aber `HEADER_MENU_ARTIST_X/Y=560/208` zeigt vermutlich daneben — der
„Artist wieder sichtbar"-Check ist nur deshalb grün, weil nie etwas versteckt
wurde. `QUEUE_ROW0/1` steht noch auf 106/157. Beides ist aus
`03-column-header-menu.png` und `06-queue-before-reorder.png` ausmessbar.

---

## Stand 12.08.2026, 19:53 — Gruppe 3 erledigt: **3 rot, 17 grün**

Gemessen statt geraten, in drei Läufen von 8 auf 3.

- **Spaltenkopf-Menü:** Die Einträge liegen bei y = 260, 329, 398, 467, 536, die
  Schalter bei x = 695 (`HEADER_MENU_ARTIST` also 695/329 statt 560/208 — die
  alten Werte lagen im leeren Streifen über der ersten Zeile).
- **Queue-Zeilen:** y = 235 und 280. Die Queue-Ansicht trägt dasselbe
  Kopfband wie die Bibliothek **plus** einen „Play Next"-Abschnittskopf; die
  alten 106/157 stammen aus dem Layout ohne beide Bänder.
- **Die Artist-Wiederherstellung war kein Koordinatenfehler.** Ein zusätzlicher
  Screenshot des wieder geöffneten Menüs zeigte, dass es sich gar nicht öffnet:
  Das Sichtbarkeitsmenü bleibt über Umschaltungen hinweg offen (richtiges
  Verhalten), der zweite Rechtsklick klickt es also nur weg. Ein `Escape` davor
  löst beide Checks; ein zweites am Flow-Ende lässt keinen Popover in die
  Folge-Flows ragen. Die Vermutung „der Eintrag wandert nach unten" war falsch —
  eine nachgemessene Koordinate hätte hier nichts gerettet.

### Die verbleibenden drei sind beide Produktverdachte

1. `rating changed` fehlt und `rating` bleibt 0 (2 Checks).
2. Der Tag-Editor meldet „presented" und erscheint nicht, also kommt auch die
   Jahr-Zurückweisung nie (1 Check).

Damit ist das Ziel des Plans erreicht: **Jede rote Zeile der Harness ist wieder
ein echter Befund.** Keine Zusicherung wurde abgeschwächt. Die beiden Verdachte
brauchen je eine eigene Diagnose — Stern-Klick über ein `pick()`-Raster, der
Tag-Editor über einen AT-SPI-Baum im Moment des „presented".

---

## Stand 12.08.2026, 20:17 — beide Verdachte gemessen: einer war die Harness

Instrumentierung in `e42bfab919` (`chore(probe)`, rein additiv, **wieder zu
entfernen, sobald der Tag-Editor gefixt ist**): Capture-`GestureClick` plus
`pick()` auf der Rating-Zelle, `map`/`unmap`/`closed` an beiden Dialogen.

### Der Stern-Klick war kein Produktfehler

Die Probe schwieg zunächst vollständig — kein Druck erreichte die Zelle. Der
Screenshot erklärte warum: `click_window_from_right "$INFO_TOGGLE_FROM_RIGHT" 28`
löst mit `INFO_TOGGLE_FROM_RIGHT=222` zu x = 1378 auf, und dort sitzt der
**Hauptmenü-Knopf** (x ≈ 1373), nicht der Informations-Umschalter. Das geöffnete
Menü legte sein Autohide-Popover über die halbe Tabelle, und der Klick auf den
Stern daneben wurde als Wegklicken verschluckt, statt die Taste zu erreichen.
Die Zelle war die ganze Zeit in Ordnung: Der Hover blendete die fünf Sterne aus,
sie waren im Screenshot rechts neben dem Popover sichtbar.

Der Umschalt-Klick ist ersatzlos raus — die Informationsspalte ist in diesem
Fixture-Profil gar nicht offen, `01-initial-track-list.png` zeigt schon vor dem
Klick jede Spalte bis Rating. Danach protokolliert die Probe sauber Druck,
Loslassen, `rating star activation handler entered star=2` und
`rating changed new_rating=2`; **beide Rating-Checks sind grün**, inklusive der
Persistenz in der Datenbank.

Lehre: Ein „das Produkt nimmt den Klick nicht an" kann auch heißen, dass ein
früherer Schritt der Harness etwas offen gelassen hat. Der Screenshot direkt vor
dem Klick beantwortet das in Sekunden; das Codelesen davor hat zwei falsche
Mechanismen produziert.

### Der Tag-Editor ist ein echter Fehler — und schärfer als gedacht

Er wird **gemappt**, meldet `visible=true`, hat Root und Native, und nimmt
Tastatureingaben entgegen (im vorigen Lauf schloss ihn genau das `Return` des
Jahr-Schritts). Er wird nur **nie gezeichnet**: Der Vollbild-Screenshot 400 ms
nach dem Mappen unterscheidet sich um 1,5 % vom Ausgangsbild, ein Dialog wären
Größenordnungen mehr. Der Löschbestätigungs-Dialog malt im selben Lauf
einwandfrei (8,2 % Unterschied) — es liegt also nicht an der Umgebung.

Damit sind „nie gemappt", „falsches Elternteil" (`shared.window.upgrade()`) und
„Inhalt/Größe null" erledigt. Offen ist, warum ein gemappter, fokussierter
Dialog nicht malt.

**Er zieht die Kaskade nach sich.** Im letzten Lauf blieb er offen und schluckte
danach die Tastatur, weshalb Flow 3 fünf Checks verliert. Die Lauf-Bilanz von 6
rot hat deshalb nur **eine** Ursache — nicht sechs.

---

## Stand 12.08.2026, 21:05 — der „echte Fehler" war die Öffnungsanimation

Der Abschnitt oben („Der Tag-Editor ist ein echter Fehler") ist **widerlegt**.
Der Dialog malt. Er war 400 ms nach `present()` nur noch nicht fertig
eingeblendet.

Der Beweis lag die ganze Zeit in `/tmp/reprise-ptr-e2e/`: `04-keyboard-tag-editor.png`
zeigt den Dialog bei ~5–8 % Deckkraft und noch nicht auf Endgeometrie
(Kopfzeile y≈227 statt 175, linke Kante x≈572 statt 520) — „Cancel / Edit Tags /
Save", „Title / sine_01", „Year", die Rating-Sterne, alles als Geister lesbar.
`05-invalid-year-rejected.png`, gut eine Sekunde später, zeigt denselben Dialog
vollständig deckend. Die gemessenen 1,5 % Bildunterschied sind exakt das, was
ein Geist bei ~8 % Deckkraft erzeugt.

AdwDialog öffnet über eine Spring-Animation. Auf Debug-Build + llvmpipe + Xvfb
steht die nach 400 ms erst am Anfang. Fix in der Harness, nicht im Produkt:
`gtk-enable-animations=0` in der ohnehin generierten `settings.ini` des
Scratch-`XDG_CONFIG_HOME` (Commit `af842daddd`). Damit sitzt jeder Dialog beim
Mappen auf Endgeometrie und voller Deckkraft. Derselbe Screenshot an derselben
Skriptstelle zeigt den Dialog danach komplett.

Die angebliche Kaskade („der unsichtbare Dialog schluckt danach die Tastatur")
fällt damit ebenfalls: Timing, kein Zustandsfehler. Dass ihn in einem Lauf das
`Return` schloss und im nächsten nicht, passt zur laufenden Animation.

### Was die Suite danach zeigte

Die Bilanz stieg erst einmal — weil zum ersten Mal überhaupt alle Flows liefen.
Ein `sqlite3`-Syntaxfehler auf der gelöschten Spalte `missing` riss bis dahin
jeden Lauf in Flow 4 ab; die Flows 4b, 5 und 6 hatten **nie** stattgefunden.

| Lauf | FAIL | OK | Reichweite |
|---|---|---|---|
| 1 | 1 (auf stderr, von `heavy-run` verschluckt) | 19 | Abbruch Flow 4 |
| 2 | 1 | 19 | Abbruch Flow 4 |
| 3 | 28 | 25 | Flows 1–6 komplett |
| 4 | 19 | 29 | Compact-Einstieg repariert |
| 5 | 22 | 32 | Fehlklicks werden einzeln benannt |

Fünf Commits über `e42bfab919`: `1d1e5f3a67` (missing_since + `db_scalar_into`),
`af842daddd` (Animationen), `7bd0fa840f` (Zeiger bleibt im Fenster),
`8d35074760` (Compact über das Hauptmenü), `ac79a9a597` (nachgemessene
Koordinaten).

### Zwei Befunde, die bleiben

1. **Die Compact-Layouts existieren nur als Einstellung.** `CompactLayout
   { Cover, Pill, Card }` wird persistiert, aber kein Renderpfad unterscheidet
   sie, kein UI schreibt sie, und die Logzeile `compact layout changed`, auf die
   Flow 5 dreimal wartet, gibt es im Repo nicht. Flow 5 prüfte ein nie gebautes
   Feature. → eigener Plan: `ptr-e2e-compact-flow-respec.md`.
2. **Ein ungültiges Jahr wird als „No effective changes" erklärt.**
   `parse_number_field("0")` ist laut eigenem Unit-Test ein Fehler
   (`tag_editor_dirty_tests.rs:40`), aber der Fehlerpfad liegt in `do_save`
   (`tag_editor_save.rs:153`) — und Save ist deaktiviert, weil die Dirty-Logik
   keine wirksame Änderung sieht. Geschrieben wird nichts, die Begründung ist
   irreführend. UX-Befund, in diesem Strang nicht gefixt.

---

## Stand 13.08.2026, 00:30 — offener Produktbefund: ein `Next` erzeugt drei Starts

Beim Nachfahren auf der neueren `dev`-Basis fiel in Flow 4 auf: Bei
**eingefrorener Wiedergabe** (`mpris_call Pause`) erzeugte ein einziger
`mpris_call Next` drei Wiedergabestarts hintereinander.

Gemessen (`/tmp/reprise-ptr-e2e/app.log`, Lauf 12.08.2026 22:01):

```
02.456  applying state change state=Paused
02.768  playback started track_id=4 gapless=false from_up_next=true   <- Next #1 (X)
03.076  playback started track_id=5 gapless=false from_up_next=true   <- Next #2 (Y)
03.085  playback started track_id=2 gapless=true  from_up_next=false  <- 9 ms später, ungefragt
03.120  playback started track_id=1 gapless=false from_up_next=false  <- 35 ms später, ungefragt
```

Der Verdacht: `Next` auf einen bereits vorbereiteten Gapless-Nachfolger lässt
die Pipeline die vorbereitete Übergabe zusätzlich ausführen, und der
`StreamStart`-Pfad bucht sie als weiteren `AdvancedToNext`. Nicht in diesem
Strang untersucht, nicht von den `dev`-Commits verursacht (`fc778ddb10`,
`5ce6b3d8c2`, `9a7e51b006`, `4becab7a93` fassen den Playback-Pfad nicht an).

Flow 4 prüft deshalb, dass der Kontext bei B wieder aufsetzt, sobald die
manuelle Queue leer ist — im selben Fenster statt nach einem dritten `Next`.
Die frühere Formulierung („MPRIS Next resumed the unchanged context at B")
behauptete einen Mechanismus, den das Produkt so nicht hat.

**Ebenfalls in diesem Nachlauf repariert:** Flow 4 maß weiter auf dem fahrenden
Zug — die Queue lief autonom durch, bevor die Harness ihre Marker setzte, und
vier Checks lasen in ein leeres Fenster. Paket D hatte das nur für Flow 3
erledigt. Die Wiedergabe wird jetzt auch hier vor den Queue-Schritten
eingefroren, und Flow 4b stellt seine Vorbedingung („es spielt etwas") selbst
her, statt sie zu erben.
