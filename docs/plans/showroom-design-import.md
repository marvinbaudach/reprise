---
slug: showroom-design-import
worktree: /home/marvin/Projects/reprise-showroom-design-import
branch: feature/showroom-design-import
phase: coded
codex_session:
created: 2026-08-18
---

# Die Showroom-Seite wird die Design-Fassung

**Ziel.** Der Claude-Design-Entwurf „Reprise Showroom" ersetzt die heutige
Seite — nicht als Kopie des Monolithen, sondern als React-Komponenten im
bestehenden Aufbau: Scroll-Choreografie, wandernde Grundfarbe, Öl-Licht,
Mosaik mit Lightbox, eine neue Spectral-Seek-Sektion, ein neues CH.04 für CLI
und MCP, ein Tempo-Band und CH.05 als Bilanz.

**Herkunft.** Der Auftraggeber hat „ganze Seite übernehmen" gewählt
(18.08.2026). Der Visualizer aus
`docs/plans/showroom-plate-plays-the-visualizer.md` gehört **nicht** noch
einmal hierher: er ist gebaut, abgenommen und liegt im Vorgängerzweig.

---

## 1. Wo das Design liegt, und wie man es liest

Claude-Design-Projekt **`476e3050-d03c-4e6b-b376-448b2c137b03`**
(„Reprise Promotion-Seite"), Datei **`Reprise Showroom.dc.html`**.

- Nur über die `claude-design`-MCP-Werkzeuge erreichbar (`read_file` mit
  `offset`/`limit`). **Codex kommt nicht heran** — es ist auf den Worktree
  gesandboxt. Deshalb baut diese Seite die Hauptschleife, nicht Codex.
- `github.md` im selben Projekt führt eine Sektions-zu-Quelle-Tabelle und ein
  Sync-Protokoll. Das ist der Einstieg, nicht der Monolith.
- **Die Datei wird währenddessen bearbeitet.** Sie wuchs am 18.08. mitten in
  der Arbeit von 1569 auf 1592 Zeilen, der Etag wechselte. Zeilenbereiche
  unten sind Anhaltspunkte, keine Adressen — vor dem Zitieren neu lesen.

Zeilenbereiche, Stand Etag `1787057148553485` (1592 Zeilen):

| Bereich | Inhalt |
|---|---|
| 1–35 | Kopf, Grundstile, alle `@keyframes` |
| 36–120 | Wurzel, Grund/Öl/Grain, Fortschritt, Kopfzeile, **Hero** samt Seek-Streifen |
| 121–140 | Tempo-Band |
| 141–220 | CH.01 |
| 221–403 | CH.02 |
| 404–436 | CH.03 Kopf, Seek-Figur |
| 437–540 | Mosaik-Reihen |
| 541–575 | CH.04 |
| 576–639 | CH.05 |
| 640–671 | Fußzeile und Verfügbarkeit |
| 672–694 | Lightbox |
| 695–1592 | das Skript: Lebenszyklus, Reveal, Zähler, Grund, Tilt, Lightbox, Seek, Viz |

## 2. Zwei Befunde des Designs, beide entschieden

1. **Die Platte im Entwurf ist die verworfene Visualizer-Fassung.** `drawViz`
   zeichnet auf Referenzhöhe 300 px und skaliert, `impact` ist wieder das
   Mittel der ersten sieben Bänder, die Bänder kommen aus zwei Sinus, die Peaks
   fallen mit `dt·60`. Genau die vier Fehler, die die Messung des
   Visualizer-Plans widerlegt hat. **Es gilt die gebaute Fassung**
   (`showroom/src/visualizer/`), der Design-Code fliegt raus.
2. **Die Seek-Leiste würfelt ihre Eingabe.** Die Formung ist ein echter Port
   von `waveform.rs`/`spectral_colour.rs`, aber `buildTrack()` erzeugt Pegel
   und Centroid aus geseedetem Fraktal-Rauschen. Der Auftraggeber hat
   **messen** gewählt; das erledigt der Nebenzweig
   `docs/plans/showroom-seek-track-measured.md`, der
   `showroom/public/media/showroom/seek-track.bin` liefert (2004 B: `u32` LE
   Spieldauer in ms, dann 1000 Pegel, dann 1000 Centroid-Werte).

## 3. Was gebaut ist

**Stufe 1, Commit `d856bb5e26`** — das Gerüst:

| Datei | Zweck |
|---|---|
| `chrome/Backdrop.tsx` + `backdrop.css` | Grundfarbe, drei driftende Lichtblasen, Conic-Sweep, Grain |
| `chrome/ScrollProgress.tsx` | die 2-px-Spektrallinie am oberen Rand |
| `chrome/SiteHeader.tsx` + `chrome.css` | feste Kopfzeile, Marke, Alpha-Pille, Kapitelnavigation, `data-lifted` ab 60 px |
| `hooks/usePageChoreography.ts` | **ein** rAF-gedrosselter Tick: Reveal-Sweep, Grundfarbe, Fortschritt, Kopfzeile, aktiver Kapitellink, Öl-Parallaxe |
| `hooks/useReducedMotion.ts` | die Bewegungspräferenz, laufend beobachtet |
| `lib/reveal.ts` | Reveal-Pass mit den Konstanten des Entwurfs (0,88 · vh, 24 px, Stufung 70/20 ms) |
| `lib/counters.ts` | Zähler, 1250 ms, quartisches Ausklingen, Tausendertrenner erhalten |
| `chapters/TempoBand.tsx` | das Tempo-Band |
| `public/brand/` | `reprise-mark.svg`, `favicon.svg` aus `data/brand/` kopiert |

Nachweis: `npm run build` grün, `npm test` **16/16**, `npm run lint` grün.

## 4. Was noch fehlt

In dieser Reihenfolge, jede Stufe ein eigener Commit:

1. **Hero** (Design 36–120): Kopfzeile des Textblocks, Scroll-Hinweis, die
   beiden Aufnahmen als Kacheln, darunter der **Seek-Streifen** (Marke,
   `0:00`, Canvas, `−3:34`).
2. **Kachel und Lightbox**: `showcase/ShotTile.tsx` (Tilt 8°, Sheen an
   `--mx`/`--my`, Lade-Sweep, Beschreibung bei Hover), `showcase/Lightbox.tsx`
   (Pfeiltasten, Escape, Zoom 2,1 mit Ursprung am Klickpunkt, `overflow:
   hidden` auf `<html>`).
3. **CH.01 und CH.02** auf die Entwurfsfassung umbauen — beide Komponenten
   existieren, aber im alten Aufbau. Das Verhältnisband braucht
   `[data-ratio] > span` mit `data-w`, damit die Choreografie es füllt.
4. **CH.03**: die Seek-Sektion (Canvas, Modusschalter „Spectral fill" /
   „One colour + marks", Ablesezeile, drei Legendenkarten) plus die
   Mosaik-Reihen mit ihren Flex-Verhältnissen.
5. **CH.04** (CLI-Terminalkarte, MCP-Fähigkeitsliste) und **CH.05** (Bilanz
   als Tabelle) neu; Fußzeile und Verfügbarkeitsblock nachziehen.
6. **Tests** in `showroom/tests/`: je Sektion ein Vertrag im Stil der
   bestehenden (`page-contract`, `product-gallery`), und der Paritätswächter
   um `waveform.rs`/`spectral_colour.rs` erweitert — `SILENCE_RMS`,
   `PERCENTILE_LOW/HIGH`, `HEIGHT_GAMMA`, die beiden OKLCH-Endpunkte, der
   Schwellwert 26/255 und der Mindestabstand von 20 s.

Die Ports gehören nach `showroom/src/lib/waveform.ts` und
`showroom/src/lib/spectralColour.ts`, mit exportierten Konstanten — sonst kann
der Paritätstest sie nicht lesen.

## 5. Was dieser Plan nicht tut

- **Keine Messung der Codezahlen.** Die Fußzeile sagt ausdrücklich, daß die
  Zahlen eingetippt sind und die Messung noch aussteht. Das bleibt so, bis sie
  wirklich in CI läuft.
- **Kein Showroom-Merge-Gate.** Wie beim Visualizer: die Showroom-Tests laufen
  erst im Pages-Workflow beim Push auf `main`, also nach dem Merge.
- **Keine UX-Regel-IDs** — `check-ux-traceability.sh` kennt `showroom/` nicht.
