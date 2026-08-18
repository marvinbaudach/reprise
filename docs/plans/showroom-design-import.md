---
slug: showroom-design-import
worktree: /home/marvin/Projects/reprise-showroom-design-import
branch: feature/showroom-design-import
phase: reviewed
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

Der Entwurf liegt seit dem 18.08.2026 **im Repository**:
`docs/design/reprise-showroom.design.html`, 1666 Zeilen, mitsamt
`docs/design/README.md` (Leseführer, Zeilentabelle, Zuordnung der Bild-UUIDs
auf die vorhandenen Assets).

Der Auftraggeber hat den Browser-Export der Design-Datei bereitgestellt; die
Quelle steckte darin als JSON-String im `<script type="__bundler/template">`
und wurde ausgepackt. Damit fällt die Beschränkung des ursprünglichen Plans:

- **Codex kommt jetzt heran.** Der Sandkasten reicht bis zum Worktree, und die
  Vorlage liegt im Worktree. Der Bau der restlichen Stufen geht deshalb an
  Codex, nicht mehr an die Hauptschleife.
- **Die Vorlage bewegt sich nicht mehr unter der Lesung.** Sie ist getrackt;
  ein neuer Export ersetzt sie und `git diff` zeigt, was sich geändert hat.
  Zeilenbereiche sind wieder Adressen, keine Anhaltspunkte.

Die MCP-Datei `Reprise Showroom.dc.html` im Projekt
`476e3050-d03c-4e6b-b376-448b2c137b03` bleibt die Bearbeitungsfassung — sie
ist die Quelle, das Repo hält die Kopie. `github.md` im selben Projekt führt
die Sektions-zu-Quelle-Tabelle und das Sync-Protokoll.

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

**Drei Abweichungen vom Entwurf, gefunden am 18.08. beim Abgleich gegen die
lokale Vorlage** — sie gehören in die nächste Stufe zurückgeholt:

| `showroom/src/components/chrome/backdrop.css` | Entwurf | gebaut |
|---|---|---|
| Drift-Dauern der drei Blasen | 42 s / 57 s / 71 s | 68 s / 94 s / 116 s |
| Dauer des Conic-Sweeps | 150 s | 220 s |
| Rahmen der Öl-Schicht | `inset: -12%` | `inset: 0` |

Dazu fehlt der Öl-Schicht `opacity: var(--oil, 0.55)` und
`transition: transform 1600ms cubic-bezier(0.16, 1, 0.3, 1)`. Das dritte Paar
ist nicht nur Geschmack: die Choreografie verschiebt `#backdrop-oil` per
`translate3d`, und bei `inset: 0` kann dabei die Kante der Schicht ins Bild
wandern.

## 4. Was noch fehlt

Die Reihenfolge folgt jetzt der Abhängigkeit vom Nebenzweig: alles, was die
gemessene Seek-Spur (`showroom/public/media/showroom/seek-track.bin`) braucht,
kommt **hinter** den Rebase auf `feature/showroom-seek-track`. Jede Stufe ein
eigener Commit.

### Vor dem Rebase — ohne die Seek-Spur baubar

1. **Hero** (Vorlage 140–190), **ohne** den Seek-Streifen: Auszeichnung,
   Überschrift, die beiden Fließtexte, der Scroll-Hinweis mit der pulsenden
   Linie (`rpCue`), und rechts die beiden Aufnahmen als Kacheln — die GNOME-
   Kachel groß, die Android-Kachel als überlappendes Rechteck bei
   `right: -5%; bottom: -6%; width: 24%`. Die Visualizer-Platte sitzt schon
   auf der Android-Kachel (`showroom/src/visualizer/`) und bleibt, wie sie
   gebaut ist — der `drawViz` des Entwurfs fliegt raus (§2).
2. **Kachel und Lightbox**: `showcase/ShotTile.tsx` (Tilt 8°, Sheen an
   `--mx`/`--my`, Lade-Sweep `rpSweep`, Beschreibung bei Hover über
   `grid-template-rows: 0fr → 1fr`), `showcase/Lightbox.tsx` (Pfeiltasten,
   Escape, Zoom 2,1 mit Ursprung am Klickpunkt, `overflow: hidden` auf
   `<html>`, `rpLbIn`/`rpFade`).
3. **Rückholung der drei Abweichungen** aus §3 in `backdrop.css`.
4. **CH.01 und CH.02** auf die Entwurfsfassung umbauen (Vorlage 210–316 und
   317–434) — beide Komponenten existieren, aber im alten Aufbau. Das
   Verhältnisband braucht `[data-ratio] > span` mit `data-w`, damit die
   Choreografie es füllt.
5. **Mosaik-Reihen** aus CH.03 (Vorlage im Bereich 435–599, der Teil ohne
   Canvas) mit ihren Flex-Verhältnissen, auf `ShotTile` aufgesetzt.
6. **CH.04** (CLI-Terminalkarte, MCP-Fähigkeitsliste, Vorlage 600–644),
   **CH.05** samt Verfügbarkeitsblock (645–701) und die **Fußzeile**
   (702–725).

### Nach dem Rebase — die gemessene Seek-Spur liegt vor

7. **Die Ports** nach `showroom/src/lib/waveform.ts` und
   `showroom/src/lib/spectralColour.ts`, mit exportierten Konstanten — sonst
   kann der Paritätstest sie nicht lesen.
8. **Der Seek-Streifen im Hero** (Vorlage 179–187: Marke, `0:00`, Canvas,
   `−3:34`) und **die Seek-Sektion CH.03** (Canvas, Modusschalter „Spectral
   fill" / „One colour + marks", Ablesezeile, drei Legendenkarten). Beide
   lesen `seek-track.bin` (2004 B: `u32` LE Spieldauer in ms, 1000 Pegel,
   1000 Centroid-Werte). `buildTrack()` des Entwurfs fliegt raus (§2).
9. **Tests** in `showroom/tests/`: je Sektion ein Vertrag im Stil der
   bestehenden (`page-contract`, `product-gallery`), und der Paritätswächter
   um `waveform.rs`/`spectral_colour.rs` erweitert — `SILENCE_RMS`,
   `PERCENTILE_LOW/HIGH`, `HEIGHT_GAMMA`, die beiden OKLCH-Endpunkte, der
   Schwellwert 26/255 und der Mindestabstand von 20 s.

## 5. Was dieser Plan nicht tut

- **Keine Messung der Codezahlen.** Die Fußzeile sagt ausdrücklich, daß die
  Zahlen eingetippt sind und die Messung noch aussteht. Das bleibt so, bis sie
  wirklich in CI läuft.
- **Kein Showroom-Merge-Gate.** Wie beim Visualizer: die Showroom-Tests laufen
  erst im Pages-Workflow beim Push auf `main`, also nach dem Merge.
- **Keine UX-Regel-IDs** — `check-ux-traceability.sh` kennt `showroom/` nicht.
