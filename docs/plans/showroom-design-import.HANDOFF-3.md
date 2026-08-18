# Handover — Showroom: der leere Hero ist geklärt, dazu Refaktorierung und Aufräumen

Stand: 18.08.2026, spät abends. Geschrieben für eine frische Sitzung nach `/clear`.

Diese Übergabe löst `showroom-design-import.HANDOFF-2.md` ab. Die ältere Datei
bleibt für die Vorgeschichte gültig (Design-Import, Review, die erste
Reparaturrunde) — der dort offene Anzeigefehler ist **erledigt** und dort auch
schon als solcher eingetragen.

## Wo die Arbeit steht

```
origin/dev
  └─ feature/showroom-plate-plays-the-visualizer   (Visualizer-Platte)
       └─ feature/showroom-seek-track              (gemessene Seek-Spur)
            └─ feature/showroom-design-import      (die Design-Fassung)  ← Spitze
```

Worktree der Spitze: `/home/marvin/Projects/reprise-showroom-design-import`,
HEAD `a561937407`, **23 Commits über `origin/dev`**. Nichts ist gepusht.

**Achtung: die gesamte Arbeit dieser Sitzung ist nicht committet.** 14 geänderte
und 2 neue Dateien liegen im Arbeitsbaum. Der Auftraggeber wurde zweimal
gefragt, ob und wie geschnitten committet werden soll (ein Commit gegen
`fix`/`perf` getrennt) — die Antwort steht aus. **Das ist der erste Punkt für
die nächste Sitzung.**

Gates auf dem Arbeitsbaum: `npm test` 42/42, `npm run lint` 0,
`npm run typecheck` 0, `npm run build` 0.

## Der gelöste Fehler: der Hintergrund malte über den Text

**Symptom** (über mehrere Sitzungen gejagt): Kopfzeile, Absätze und alles unter
dem Hero verschwanden kurz nach dem Seitenstart, stehen blieben Kopfleiste,
Navigation und die Produktkacheln. Überlebte Reloads, trat in Brave auf, nicht
im Prüfstand.

**Ursache:** `.backdrop-ground` war `position: fixed; z-index: 0` mit deckender
Hintergrundfarbe und liegt im selben Stapelkontext wie der Inhalt (`.page` ist
`position: relative; z-index: 1`). Nach CSS-Malordnung wird ein positioniertes
Element mit `z-index: 0` **nach** dem gesamten nicht positionierten Fließinhalt
gezeichnet. Es deckte damit jede Überschrift und jeden Absatz zu und ließ genau
das stehen, was selbst positioniert ist. Kein Compositor-Fehler, keine
Erweiterung, kein Brave — korrektes CSS mit falscher Ebenenwahl.

Warum es wie „verschwindet nach dem Start" aussah: im Dev-Betrieb wird
`backdrop.css` erst mit seiner Komponente nachgeladen.

**Fix:** alle drei festen Hintergrundebenen auf `z-index: -1`.
**Wächter:** `tests/backdrop-design.test.mjs` — jede `position: fixed`-Regel dort
muß einen negativen `z-index` tragen. Mutationsprobe bestanden.

Drei Verdächtige wurden auf dem Weg **widerlegt** und sollten nicht wieder
aufgerollt werden: Dark Reader (der Fehler tritt ohne die Erweiterung genauso
auf), die Hydration-Meldung (real, aber eine andere Sache — siehe unten), und
die Reveal-Mechanik (sie entschied bei allen Messungen korrekt: `opacity: 1`,
Kästen im Blickfeld, Schwelle plausibel).

## Was diese Sitzung sonst repariert hat

| Befund | Reparatur | Wächter |
|---|---|---|
| Der Dev-Server hydriert gegen ein leeres `#root` — React verwarf bei **jedem** Start den Baum und baute ihn neu | `entry-client.tsx`: `hydrateRoot` nur mit Prerender-Markup, sonst `createRoot` | — |
| Reveal kannte nur **einen** Nachzügler-Anlauf nach 400 ms; jede spätere Layout-Änderung ließ Inhalt unsichtbar | `usePageChoreography.ts`: `ResizeObserver` auf der Seite plus `document.fonts.ready` | — |
| Eine fehlschlagende Nebenwirkung im Sweep kippte die ganze Warteschlange — alles dahinter blieb für immer versteckt, auch beim Scrollen | `reveal.ts`: erst alle Eintretenden zeigen, dann jede Nebenwirkung einzeln abgesichert | `tests/reveal-sweep.test.mjs` (3) |
| Die Zahlen der Seite standen im Dev-Betrieb auf `0`: der doppelte Effektlauf las die schon genullte Zahl als Ziel | `counters.ts`: `prepareCounter` liest die Zahl genau einmal aus dem Markup | `tests/counters.test.mjs` (2) |

Beide neuen Testdateien haben eine bestandene Mutationsprobe.

## Refaktorierung: der Frame-Pfad

Gemessen über CDP, Median aus je drei Läufen. **Frame-Zeiten sind im Dev-Bau
rauschdominiert** (max 21 bis 267 ms bei identischem Code) — belastbar sind nur
die Renderer-Zähler.

| | Layouts | Style-Neuberechnungen |
|---|---|---|
| Ladepfad vorher | 172 | 396 |
| Ladepfad nachher | **53–86** | **173–213** |
| Lesedurchlauf vorher | 206 | 573 |
| Lesedurchlauf nachher | 206 | **392** |

- **`reveal.ts` — Lesen vor Schreiben.** `prepareReveals` und `sweepReveals`
  maßen ein Element, schrieben Stile, maßen das nächste; jeder Wechsel erzwingt
  ein neues Layout, beim Laden über alle 93 Elemente.
- **`reveal.ts` — `will-change` nur für das, was sich bewegt.**
- **`usePageChoreography.ts` — Navigationsziele einmal auflösen** statt pro Link
  pro Frame ein `getElementById`.
- **`usePageChoreography.ts` — Zeigerbewegung in den Frame**, `scrollHeight` je
  Frame nur einmal, ungültig gemacht vom `ResizeObserver`.

## Gestalterische Änderungen auf Zuruf

- **Favicon.** `public/brand/favicon.svg` lag längst im Repo, war nie verlinkt —
  daher der `favicon.ico`-404 in jeder Konsole. `index.html` verlinkt sie jetzt
  als `icon` und `apple-touch-icon`; Vite setzt beim Bauen `/reprise/` davor.
- **Hover der Bildkacheln gedrosselt.** Der Sheen ging auf `opacity: 1` und
  wusch den Screenshot aus. Neu: `--sheen-peak: 0.62` auf `.shot-tile__sheen`,
  ein Regler ohne Eingriff in die Verläufe. **Vom Auftraggeber noch nicht
  beurteilt** — falls immer noch zu hell, nur diesen Wert senken.
- **Der Modus-Umschalter ist raus.** „One colour + marks" wurde vollständig
  entfernt: der `fieldset`, der Zustand, die `marks`-Zeichenroute in
  `seekRenderer.ts` (samt `SeekMode`, `setMode`, `SINGLE_COLOUR`,
  `sectionBoundaries`-Import), die zugehörigen CSS-Regeln, das
  `data-seek-mode`-Attribut und die Legende „Marks — the sections". CH.03 zeigt
  jetzt zwei Legenden statt drei; die Tests sind entsprechend nachgezogen und
  sperren die Rückkehr (`assert.doesNotMatch(chapter, /seek-modes|…/)`).

## Offen

### Entscheidung: Qualitätsstufen für die Bilder

**Lazy-Loading ist da** — die zwei Hero-Bilder bewusst `eager`, alle Mosaik-
Kacheln `lazy`, dazu `decoding="async"` und feste `width`/`height`.
**`srcset`/`sizes` gibt es nicht.** Gemessen an der laufenden Seite:

| | Wert |
|---|---|
| Alle 11 Bilder auf der Platte | 924 KB, größtes 117 KB |
| Dekodiert im Speicher | **96,2 MB** |
| Überabtastung `android-visualizer.webp` | **5,0×** (1080 px Datei auf 215 px Fläche) |
| Überabtastung `gnome-library.webp` | 2,7× |

Die Übertragung ist unkritisch, der Dekodier- und Speicheraufwand nicht. Der
Gewinn läge also bei Speicher und Dekodierzeit, der Preis wären rund zwanzig
zusätzliche Binärdateien im Repo. `magick`/`convert` steht auf dem Rechner zur
Verfügung, `sharp` und `cwebp` nicht. **Der Auftraggeber hat danach gefragt,
aber noch nicht entschieden.**

### Aus HANDOFF-2 übernommen, weiterhin offen

- **Vier kleine Review-Befunde:** unabbrechbare rAF-Schleife in `counters.ts`,
  ungetrackter `setTimeout` in `reveal.ts`, Ref-Zuweisung im Render-Körper von
  `MeasuredSeekTrack.tsx`, kein `AbortController` auf dem geteilten Seek-`fetch`,
  `u32::try_from`-Panik in `waveform.rs`.
- **Die Sprossen brechen auf dem Handy:** bei 390 px ist `.rungs__rung` 560 px
  breit in einem 347 px breiten Container. Braucht eine Gestaltungsentscheidung.
- **`isHero()` in `reveal.ts` sucht `#hero`, der Hero heißt `#rp-top`.** Der
  Zweig ist tot. Beim Refaktorieren bewusst stehengelassen, weil ihn zu
  reparieren das Verhalten sichtbar ändern würde (die Hero-Texte bekämen eine
  Einblendung, die sie heute nicht haben). Das ist eine Gestaltungsfrage, keine
  Aufräumarbeit.

## Fallen dieser Sitzung

- **`Page.captureScreenshot` erwartet Dokument-Koordinaten, nicht
  Fensterkoordinaten.** `getBoundingClientRect()` liefert letztere. Ohne
  `+ scrollX/scrollY` im `clip` bekommt man ein gleichmäßig graues Bild und hält
  es für einen leeren Seitenbereich — das hat hier zweimal eine falsche Spur
  erzeugt.
- **`Input.dispatchMouseEvent` löst kein `:hover` aus** (nachgemessen:
  `matches(':hover')` bleibt `false`). Für Hover-Zustände `DOM.enable` +
  `CSS.enable` + `CSS.forcePseudoState` nehmen.
- **Die erste Messung nach einer Quelländerung ist wertlos.** Vite übersetzt
  kalt neu; der Ladepfad zeigte 208 Layouts statt 53–86. Immer einen Aufwärmlauf
  verwerfen.
- **`curl` auf `localhost` scheitert mit `000`, solange die Bash-Sandbox aktiv
  ist.** Das ist kein Netzproblem — `dangerouslyDisableSandbox` löst es. Die
  gegenteilige Notiz in HANDOFF-2 war falsch.
- **Frame-Zeiten im Dev-Bau nicht als Beleg verwenden.** Nur `LayoutCount`,
  `RecalcStyleCount` und deren Dauern sind reproduzierbar.

## Methode, die den Fehler gefunden hat

Eine **eigene Brave-Instanz mit Debug-Port und Wegwerf-Profil** — damit war der
Fehler ohne den Auftraggeber reproduzierbar, und das leere Profil hat
Erweiterungen als Ursache ausgeschlossen:

```
brave --remote-debugging-port=9333 --user-data-dir=$SCRATCH/prof \
      --no-first-run --no-default-browser-check --window-size=1376,860 URL
```

Dann `http://127.0.0.1:9333/json/list` nach dem Ziel fragen und dessen
`webSocketDebuggerUrl` mit Nodes globalem `WebSocket` ansprechen. Gebraucht
wurden `Runtime.evaluate`, `Page.reload`, `Page.captureScreenshot`,
`Runtime.consoleAPICalled`, `Performance.getMetrics`,
`Emulation.setDeviceMetricsOverride` und `CSS.forcePseudoState`. Kein
Playwright, keine zusätzliche Abhängigkeit.

Den Ausschlag gab eine **Bisektion**: die drei Hintergrundebenen nacheinander
per `style.display = 'none'` abschalten und nach jedem Schritt den Hero-Kopf
abgreifen. Schritt 3 — Grundfarbe aus — brachte den Text zurück.

Zwischenzeitlich lief eine Dev-Instrumentierung in `usePageChoreography.ts`
(`REVEAL#…`- und `PAINT-KETTE`-Zeilen in der Konsole), über die der Auftraggeber
per Screenshot die Zahlen seiner eigenen Seite liefern konnte. Sie ist wieder
ausgebaut. Falls ein ähnliches Bild wiederkommt: dieser Weg funktioniert, weil
er dem Auftraggeber nur einen Reload und einen Screenshot abverlangt.

## Betrieb

- **Ein Dev-Server läuft** aus diesem Worktree auf **Port 5199**
  (`http://localhost:5199/reprise/`). Zum Beenden den Port suchen
  (`ss -ltn | grep 5199`) und den Prozeß killen — nicht per `pkill -f vite`,
  das trifft die eigene Kommandozeile mit.
- Die Meß-Skripte lagen im Scratchpad der Sitzung und sind weg. Die Methode oben
  reicht, um sie in wenigen Minuten neu zu schreiben.
- **Kein Wake-Lock genommen.** Für unbeaufsichtigte Läufe:
  `wake-lock acquire showroom-design-import "…"`.
