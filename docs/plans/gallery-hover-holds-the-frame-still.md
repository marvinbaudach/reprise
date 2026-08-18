---
slug: gallery-hover-holds-the-frame-still
worktree: /home/marvin/Projects/reprise-gallery-hover-holds-the-frame-still
branch: feature/gallery-hover-holds-the-frame-still
phase: planned
codex_session:
created: 2026-08-18
---

# Der Rahmen hält still, nur das Bild bewegt sich

> Zeilennummern gegen `origin/dev` @ `73bfa05186` erhoben (18.08.2026). Der
> lokale Hauptcheckout steht auf detached HEAD `be5f014d3b` und enthält
> `showroom/` **nicht** — gearbeitet wird im Worktree oben, geschnitten von
> `origin/dev`.

**Ziel.** Die elf Screenshot-Platten des Showrooms reagieren heute auf den
Zeiger mit einem verfolgenden Lichtkegel, einer 3D-Kippung und einer
aufklappenden Bildunterschrift. Danach hält der Rahmen still und ist
beschnitten: nur das Bild darin fährt heran. Die Platte hebt sich, der Schatten
vertieft sich, der Rand wärmt zum Akzent, und ein Lupenzeichen kündigt den
längst vorhandenen Klick zur Vergrößerung an.

**Herkunft.** Neu. Die zwei bestehenden Showroom-Pläne
(`showroom-design-import.md`, `showroom-plate-plays-the-visualizer.md`, beide
`phase: shipped`) berühren die Hover-Behandlung nicht.

---

## 1. Was heute dasteht

Alle Hover-Wirkung sitzt in **einer** Komponente, `ShotTile`, benutzt von der
Galerie (`ProductGallery.tsx:58`) und den Hero-Platten
(`HeroProduct.tsx:38,46`). Elf Platten: 2 im Hero, 9 in der Galerie.

**`showroom/src/components/showcase/ShotTile.tsx`**

| Zeile | Was |
|-------|-----|
| 12 | `const TILT_DEGREES = 8` |
| 14–20 | `resetTile()` — räumt vier Inline-Stile ab |
| 55–57 | `useEffect`, der bei `reducedMotion` `resetTile` ruft |
| 59–75 | `handlePointerMove` — der ganze Zeigerpfad |
| 63–67 | rechnet `--mx`/`--my` und schreibt sie als Inline-Custom-Properties |
| 69–74 | Inline-`transition`, `transform` (Kippung + `scale(1.014)`), `boxShadow`, `borderColor` |
| 89–90 | `onPointerMove` / `onPointerLeave` am Button |
| 101 | `<span class="shot-tile__sheen" data-sheen>` — der Lichtkegel |
| 102–113 | die Bildunterschrift samt `description-wrap` |

**`showroom/src/components/showcase/shot-tile.css`**

| Zeile | Was |
|-------|-----|
| 2–3 | `--mx: 50%; --my: 50%` |
| 21 | `transform-style: preserve-3d` — nur für die Kippung da |
| 31–38 | `.shot-tile > .product-shot` — Ladeeinblender |
| 45–58 | `.shot-tile__sweep` — **Ladeschimmer, nicht Lichtkegel** |
| 66–99 | `.shot-tile__sheen` + `--sheen-peak` + die Hover-Regel |
| 158–180 | `description-wrap` `0fr`, Hover → `1fr` |
| 191–207 | der `prefers-reduced-motion`-Block |

### 1.1 Drei Prämissen des Auftrags stimmen so nicht

**Die Zeile fließt nicht um.** `.shot-tile__caption` ist
`position: absolute; inset: auto 0 0` (`shot-tile.css:117-118`), und
`scale(1.014)` ist ein Transform. Die Höhe der Platte hat sich **nie**
geändert. Die Beschreibung verschwindet also, weil sie den Screenshot zudeckt
— nicht wegen Reflow. Der Höhentest bleibt trotzdem: als Regressionssperre,
nicht als Beleg für einen heutigen Fehler.

**Die Lightbox liest die Beschreibung nicht aus dem DOM**, sondern
`capture.description` aus dem Datenmodul (`Lightbox.tsx:189-190`). Die Kopie in
der Platte erreicht niemanden: unsichtbar bei `0fr`, und für Screenreader vom
`aria-label` des Buttons überschrieben (`ShotTile.tsx:86`). Sie fällt deshalb
**ganz** weg (Entscheidung 18.08.2026).

**Das Bild allein zu skalieren zerreißt die Hero-Telefonplatte.**
`.hero-product__visualizer` ist `position: absolute` mit Prozentgeometrie
(`showcase.css:26-34`), aufgelöst gegen den Button, und das `<canvas>` ist ein
**Geschwister** des `<img>`, kein Kind. Ein Zoom nur auf dem `<img>` ließe den
laufenden Visualizer stehen, während der Screenshot darunter wächst — das
Quadrat rutschte sichtbar aus dem Telefondisplay. Deshalb §2.2.

### 1.2 Was der Prüfstand kann — und was nicht

Alle 21 Tests unter `showroom/tests/` sind **statische Analyse**: Regex über
`dist/index.html`, über das gebaute CSS und über `.tsx`-Quelltext, der als Text
gelesen wird. Kein jsdom, kein Browser, keine Layout-Engine. jsdom hülfe auch
nicht — es rechnet kein Layout, `offsetHeight` ist dort immer 0.

Der Höhentest wird deshalb ein **CSS-Vertrag** (§4.1), ergänzt um **eine**
echte Messung in headless Chrome, die nach dem Codex-Lauf gefahren und in §7
eingetragen wird.

### 1.3 Die Suite läuft im PR-Gate gar nicht

`ci.yml:89` fährt für den Showroom nur `check-project-quality.sh --showroom` =
`npm ci` + `lint` + `lint-contract.test.mjs` (`check-project-quality.sh:47-53`).
Die vollständige Suite läuft ausschließlich in `pages.yml:57-59`, und die feuert
nur bei `push` auf `main`. **Neue Tests wären im genannten Gate tote Fracht.**
Verdrahtung: §5.2.

---

## 2. Der Zielzustand

Alles wird **reines CSS**. Danach läuft für die Platten kein JavaScript mehr auf
Zeigerbewegung — nicht gedrosselt, nicht per rAF, gar nicht.

### 2.1 Der Zustand steckt in vier Tokens

Hover steht in einer Media-Query, `:focus-visible` nicht — zusammenfassen lassen
sie sich also nicht. Beide setzen deshalb **dieselben vier Custom Properties**;
die Wirkung ist genau einmal definiert:

```css
.shot-tile {
  --plate-lift: 0px;
  --shot-zoom: 1;
  --cue-opacity: 0;
  --cue-rise: 5px;

  transform: translate3d(0, var(--plate-lift), 0);
}

.shot-tile__picture { transform: scale(var(--shot-zoom)); }

.shot-tile__zoom {
  opacity: var(--cue-opacity);
  transform: translateY(var(--cue-rise));
}
```

Der Übergang hängt an `transform`/`opacity`, nicht an der Custom Property:
`var()` wird eingesetzt, der berechnete Wert ändert sich, **den** interpoliert
der Übergang. Kein `@property` nötig.

`--mx`/`--my` verschwinden (§2.6); diese vier ersetzen keinen Zeigerpfad.

### 2.2 Ein Bildwickel, damit der Visualizer mitfährt

Neues Element in `ShotTile`, das Bild **und** die Kinder umschließt:

```tsx
<span className="shot-tile__picture">
  <ProductShot … />
  {children}          {/* VisualizerPlate der Hero-Telefonplatte */}
</span>
```

```css
.shot-tile__picture {
  position: relative;
  display: block;
  transform-origin: center;
  transition: transform 900ms cubic-bezier(0.16, 1, 0.3, 1);
}
```

`.product-shot` ist bereits `display: block; width: 100%; height: auto`
(`showcase.css:1-6`) — der Wickel bekommt damit exakt den Kasten des Bildes,
kein Zeilenabstand kommt dazu, und `.hero-product__visualizer` behält seine
Prozentgeometrie unverändert. Wickel und Canvas skalieren um denselben
Ursprung, das Quadrat bleibt im Telefondisplay stehen.

`display: block` ist Pflicht: ein Inline-Wickel erzeugte eine Zeilenbox mit
Unterlänge, der Kasten wäre ein paar Pixel höher als das Bild, und genau diese
Differenz verschöbe die Prozentgeometrie des Visualizers.

Der Wickel steht **vor** `.shot-tile__sweep`, damit der Ladeschimmer weiter
darüber liegt. Bildunterschrift und Lupenzeichen bleiben **außerhalb** — sie
sollen nicht mitskalieren.

Die bestehenden Selektoren `.shot-tile > .product-shot` und
`.shot-tile[data-loading="true"] > .product-shot` (`shot-tile.css:31-38`) ziehen
auf den Wickel um.

### 2.3 Die drei Bewegungen

```css
.shot-tile {
  overflow: hidden;                 /* steht schon da (Zeile 10) */
  transition:
    transform 620ms cubic-bezier(0.16, 1, 0.3, 1),
    box-shadow 620ms ease,
    border-color 620ms ease;
}
```

Der aktive Zustand — einmal formuliert, zweimal aufgerufen:

```css
@media (hover: hover) {
  .shot-tile:hover {
    --plate-lift: -6px;
    --shot-zoom: 1.045;
    --cue-opacity: 1;
    --cue-rise: 0px;
    border-color: oklch(46% 0.03 195);
    box-shadow:
      0 40px 90px -46px oklch(3% 0.02 269 / 0.98),
      0 0 40px -18px oklch(80% 0.12 190 / 0.35);
  }
}

.shot-tile:focus-visible {
  /* exakt dieselben sechs Deklarationen, in derselben Reihenfolge */
}
```

Rand- und Schattenwerte sind die, die der Inline-Pfad heute schon schreibt
(`ShotTile.tsx:72-74`) — was im JavaScript stand, steht danach im Stylesheet.

`@media (hover: hover)` ohne `pointer: fine`: reine Touch-Geräte bekommen keinen
Hover-Zustand, also kann keiner kleben bleiben; Trackpad und Stift behalten ihn.
`:focus-visible` steht außerhalb — Tastaturbedienung gibt es auf jedem Gerät.

### 2.4 Das Lupenzeichen

Neues Kind in `ShotTile`, nach dem Bildwickel, vor der Bildunterschrift:

```tsx
<span className="shot-tile__zoom" data-zoom="" aria-hidden="true">
  <svg viewBox="0 0 256 256" fill="currentColor" focusable="false">
    <path d="M216,48V96a8,8,0,0,1-16,0V67.31l-50.34,50.35a8,8,0,0,1-11.32-11.32L188.69,56H160a8,8,0,0,1,0-16h48A8,8,0,0,1,216,48ZM106.34,138.34,56,188.69V160a8,8,0,0,0-16,0v48a8,8,0,0,0,8,8H96a8,8,0,0,0,0-16H67.31l50.35-50.34a8,8,0,0,0-11.32-11.32Z" />
  </svg>
</span>
```

Phosphor `ArrowsOutSimple`, Gewicht *regular*, wörtlich aus
`phosphor-icons/core@main/assets/regular/arrows-out-simple.svg` (18.08.2026 aus
der Quelle geholt, nicht nachgezeichnet).

```css
.shot-tile__zoom {
  position: absolute;
  top: 10px;
  right: 10px;
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border-radius: 50%;
  background: oklch(14% 0.014 269 / 0.72);
  color: oklch(96% 0.004 269);
  pointer-events: none;
  transition:
    opacity 320ms ease,
    transform 320ms cubic-bezier(0.16, 1, 0.3, 1);
}

.shot-tile__zoom > svg { width: 15px; height: 15px; }

/* Telefonplatten sind 110–190 px breit — 30 px wären dort ein Viertel davon. */
.shot-tile--phone .shot-tile__zoom {
  top: 8px;
  right: 8px;
  width: 24px;
  height: 24px;
}

.shot-tile--phone .shot-tile__zoom > svg { width: 12px; height: 12px; }
```

`aria-hidden` + `pointer-events: none`: Dekoration über einem Button, der
bereits `aria-label="Open screenshot: …"` trägt. Trefferfläche und
Fokusreihenfolge bleiben unverändert, es kommt kein fokussierbares Element dazu.

### 2.5 Reduzierte Bewegung

Der bestehende Block (`shot-tile.css:191-207`) wird zu:

```css
@media (prefers-reduced-motion: reduce) {
  .shot-tile,
  .shot-tile__picture,
  .shot-tile__zoom {
    transform: none !important;
    transition: none;
  }

  .shot-tile__sweep { display: none; }   /* bleibt: Endlosanimation */
  .shot-tile__picture > .product-shot { transition: none; }
}
```

`transform: none !important` neutralisiert Hub, Bildzoom und das Aufsteigen des
Zeichens auf einen Schlag. Die Deckkraft bleibt an `--cue-opacity` gebunden und
springt **sofort** auf 1 — genau die Ausnahme, die der Auftrag zulässt.

Der bisherige `.shot-tile__sheen { display: none }` entfällt mit dem Element.

### 2.6 Was ersatzlos verschwindet

`ShotTile.tsx`: `TILT_DEGREES`, `resetTile`, `handlePointerMove`, der
`reducedMotion`-`useEffect`, `onPointerMove`, `onPointerLeave`, `buttonRef`, der
Import `type PointerEvent as ReactPointerEvent`, das
`<span class="shot-tile__sheen" data-sheen>` **und** der ganze
`description-wrap`/`description`-Block (§1.1).

`shot-tile.css`: `--mx`, `--my`, `transform-style: preserve-3d`, der komplette
`.shot-tile__sheen`-Block samt `--sheen-peak`, sowie alle
`description-wrap`/`description`-Regeln inklusive `grid-template-rows`.

**`.shot-tile__sweep` bleibt.** Das ist der Ladeschimmer für
`data-loading="true"`, liegt vier Zeilen neben `sheen` und heißt fast gleich.
Wer ihn mitnimmt, nimmt elf Platten den Ladeindikator.

### 2.7 Die `reducedMotion`-Kette wird kürzer

`ShotTile` braucht die Eigenschaft nicht mehr; damit wird sie auch in
`ProductGallery` und `HeroProduct` unbenutzt und fällt dort weg.

`Hero.tsx` und `ChapterThree.tsx` **behalten** sie — sie reichen sie an
`HeroSeekTrack` (`Hero.tsx:47`) bzw. `SpectralSeekTrack` (`ChapterThree.tsx:39`)
weiter. Nur die Weitergabe an `HeroProduct` (`Hero.tsx:45`) und `ProductGallery`
(`ChapterThree.tsx:42`) entfällt.

`VisualizerPlate` liest die Bewegungspräferenz selbst über `matchMedia`
(`VisualizerPlate.tsx:30`) und ist nicht betroffen.

---

## 3. Die neuen UX-Regeln

Das Regelbuch kennt heute **keinen** Showroom (`grep -c -i showroom
docs/ux-rules.md` → 0) und nur `[core]`, `[gtk]`, `[e2e]`, `[manual]`.

Neuer Abschnitt am Dateiende, nach `## AI. GNOME platform conformance`:

```markdown
## AJ. Showroom (public site)

- **SHOW-1** [active] [web] — Eine Screenshot-Platte hält ihren Rahmen still.
  Beim Zeigen bewegt sich nur das Bild darin; keine Hover- oder Fokusregel
  verändert eine layoutwirksame Eigenschaft der Platte.
- **SHOW-2** [active] [web] — Kein zeigergeführter Lichtkegel: keine
  Overlay-Ebene mit cursorabhängigem Gradienten, kein `pointermove`-Handler.
- **SHOW-3** [active] [web] — Zeigen und Tastaturfokus erzeugen denselben
  Zustand: dieselbe Hebung, derselbe Bildzoom, dasselbe Lupenzeichen.
- **SHOW-4** [active] [web] — Bei `prefers-reduced-motion: reduce` gibt es keine
  Transform-Übergänge an einer Platte; der Hinweis darf sofort erscheinen.
- **SHOW-5** [active] [web] — Ohne Hover-Fähigkeit gibt es keinen
  Hover-Zustand, damit nach einem Tap keiner kleben bleibt.
```

Legende in den Prozessregeln (`docs/ux-rules.md:23-31`) um
*„`[web]` (Showroom-Suite, `showroom/tests/`)"* ergänzen, und der
Traceability-Absatz (Zeile 33) um die dritte Namensform:
*„Showroom: `test('show-1 …')`"*.

### 3.1 `check-ux-traceability.sh` muss `.mjs` sehen

Das Skript sammelt heute aus zwei Quellen: Rust-`fn` nach `#[test]` (Zeile
51–52) und `scripts/cua-e2e` (Zeile 54–58). Eine `[active]`-Regel mit nur einem
`.mjs`-Test fiele mit *„has no rule-named test"* durch. Dritte Quelle:

```bash
# Showroom: the rule ID leads the test name, so only test() lines count.
web_refs=$(grep -rhE "^test\('(${prefixes})-[0-9]+[a-z]?-" showroom/tests 2>/dev/null \
  | grep -oE "(${prefixes})-[0-9]+[a-z]?" | sort -u || true)
```

und in Zeile 79: `for ref in $snake_refs $kebab_refs $web_refs`.

Der Anker `^test\('` ist die Absicherung, kein Schönheitsfehler: `prefixes` wird
aus dem Dokument abgeleitet und enthält das einbuchstabige `p`. Ein freier Lauf
über `showroom/tests` bliebe an jedem `p-2-…` in einem Selektor-Regex hängen und
meldete *„test references unknown rule"*.

Zeile 40 zusätzlich: `\[(core|gtk|e2e|manual)\]` → `\[(core|gtk|e2e|manual|web)\]`.
Funktional nötig ist es nicht — ein unbekanntes Level fällt in den richtigen
Zweig —, aber ein Level, das das Skript nicht liest, ist eine Falle für die
nächste Änderung.

---

## 4. Die Tests

Neue Datei `showroom/tests/gallery-hover.test.mjs`, mit dem lokalen
`builtCss()`/`prerenderedPage()`-Muster der Nachbardateien — es gibt kein
geteiltes Hilfsmodul, und dieser Plan führt keins ein. Testnamen führen die
Regel-ID: `test('show-1 …')`.

**4.1 `show-1` — Höhe in Ruhe und beim Zeigen identisch.** Als CSS-Vertrag:

1. Kein `:hover`- oder `:focus-visible`-Block, dessen Selektor `.shot-tile`
   enthält, deklariert `height`, `min-height`, `max-height`, `padding`,
   `margin`, `border-width`, `font-size`, `line-height`, `grid-template-rows`,
   `aspect-ratio`, `inset`, `top` oder `bottom`.
2. `grid-template-rows` kommt in keiner `.shot-tile`-Regel mehr vor (die
   Bildunterschrift ist weg).
3. Der Bildzoom sitzt auf `.shot-tile__picture` und ausschließlich als
   `transform`.

Die Messung liefert §7, nicht dieser Test.

**4.2 `show-2` — kein Lichtkegel, kein `pointermove`.**
`dist/index.html` ohne `data-sheen`/`shot-tile__sheen`; gebautes CSS ohne
`--mx`, `--my`, `--sheen-peak` und ohne `radial-gradient`, der `var(--m`
enthält; `ShotTile.tsx` als Text ohne `onPointerMove`, `pointermove`,
`setProperty('--m` und `addEventListener`.

**4.3 `show-3` — Zeigen und Fokus sind derselbe Zustand.** Beide
Deklarationsblöcke aus dem gebauten CSS ziehen, an `;` zerlegen, leere Einträge
werfen, sortieren, `assert.deepEqual`. Vorher `assert.ok` auf beide Treffer und
`assert.ok(entries.length >= 6)` — zwei nicht gefundene Blöcke dürfen nicht als
„gleich" durchgehen. Ein Zusammenziehen zur Selektorliste ist ausgeschlossen:
einer steht in einer Media-Query, der andere nicht.

**4.4 `show-4` — reduzierte Bewegung kennt keinen Transform-Übergang.** Im Block
hinter `prefers-reduced-motion:reduce` tragen `.shot-tile`,
`.shot-tile__picture` und `.shot-tile__zoom` `transition:none` und
`transform:none`. Muster wie `shot-tile-lightbox.test.mjs:38-41`, das mit
`[\s\S]*?` über die minifizierte Blockgrenze greift.

**4.5 `show-5` — Touch bekommt keinen Hover-Zustand.** Der
`.shot-tile:hover`-Block steht innerhalb von `@media (hover:hover)`,
`.shot-tile:focus-visible` außerhalb — geprüft über die Position beider
Selektoren relativ zur Media-Query-Grenze im gebauten CSS.

### 4.6 Bestehende Tests, die nachziehen müssen

`showroom/tests/shot-tile-lightbox.test.mjs` behauptet den alten Zustand:

| Behauptung | Was daraus wird |
|-----------|-----------------|
| `data-sheen=""` im HTML | entfällt |
| `data-dwrap=""` im HTML | entfällt (Beschreibung ist weg) |
| `const TILT_DEGREES = 8` | entfällt |
| `setProperty('--mx'` | entfällt |
| `perspective(1200px)` | entfällt |
| `radial-gradient(… var(--mx) …)` | entfällt |
| `grid-template-rows` | entfällt |
| reduced-motion → `.shot-tile__sheen … display:none` | umgehängt auf `.shot-tile__zoom` / `transition:none` |
| `doesNotMatch(source, /requestAnimationFrame/)` | bleibt |
| `data-sweep=""`, Plattenzahl 11 | bleibt unverändert |

Tests 2–4 derselben Datei (Lightbox, `showcase.ts`) bleiben unberührt.

Gegenzulesen, aber voraussichtlich unberührt: `product-gallery.test.mjs`
(zählt Buttons und Bilder), `page-vitals.test.mjs` (zählt `data-loading` und
`<img class="product-shot">`), `hero-design.test.mjs`
(`.hero-product__visualizer`-Geometrie). Das neue `<span>` ist kein `<img>` und
kein `<button>`; der Bildwickel ändert keine der gezählten Zahlen.

---

## 5. Nebenarbeiten im selben Commit

**5.1 Phosphor-Lizenz.** Das Repository führt jede fremde Quelle explizit
(`LICENSES/CAVA-MIT.txt` + Abschnitt in `LICENSING.md:50`). Ein wörtlich
übernommener Icon-Pfad ist derselbe Fall: `LICENSES/PHOSPHOR-MIT.txt` anlegen
und in `LICENSING.md` einen Abschnitt „Third-party icon note — Phosphor (MIT)"
ergänzen, der benennt, welches Icon in welcher Datei steckt.

**5.2 Die Suite ins Gate hängen.** `scripts/check-project-quality.sh`,
Showroom-Zweig (Zeile 47–53), bekommt als letzte Zeile `npm --prefix showroom test`.
Damit läuft die vollständige Suite in `ci.yml:89` (jeder PR) und in
`check-merge-readiness.sh:57`. `lint-contract.test.mjs` bleibt davor stehen: es
kommt ohne Build aus und soll weiter zuerst und schnell scheitern dürfen.

**5.3 Text der Spectral-Seek-Karte halbieren.**
`showroom/src/components/seek/MeasuredSeekTrack.tsx` — unabhängig vom
Hover-Umbau, reitet auf Wunsch in diesem Branch mit (eigener Commit).

- `seek-card__note` (Zeile 325–342): auf *„Move across the measured track to
  inspect its values. The bars are shaped by the same functions the apps use —
  bars.rs, waveform.rs and spectral_colour.rs — with only the band values
  standing in for live PCM."* Alle drei Quell-Links bleiben.
- Legende „Height — the body" (Zeile 347–352): auf *„Every bar is the RMS of its
  slice, mapped through the track's own p10–p95 window and smoothed against
  flicker. A compressed master still shows verse against chorus instead of one
  loud wall."*
- Legende „Colour — the frequency" (Zeile 355–359): auf *„The tint is the
  spectral centroid: coral is low and weighty, teal high and airy."*

Beide `<article data-seek-legend>` samt Überschriften, der Farbverlauf
(`seek-legends__axis`) und die Endmarken bleiben — die pinnt
`spectral-seek-section.test.mjs:32-33`.

---

## 6. Reihenfolge

1. `shot-tile.css` umbauen (§2.1–2.5), `sheen`- und Beschreibungsblöcke löschen.
2. `ShotTile.tsx`: Zeigerpfad raus, Bildwickel und Lupenzeichen rein (§2.2–2.4, §2.6).
3. `reducedMotion`-Kette kürzen (§2.7).
4. `docs/ux-rules.md`: Abschnitt AJ, Legende, Traceability-Absatz (§3).
5. `scripts/check-ux-traceability.sh`: dritte Quelle + `web`-Ebene (§3.1).
6. `showroom/tests/gallery-hover.test.mjs` schreiben (§4.1–4.5).
7. `shot-tile-lightbox.test.mjs` nachziehen (§4.6).
8. Lizenz (§5.1), Gate-Verdrahtung (§5.2), Textkürzung (§5.3, eigener Commit).
9. Grün fahren: `npm --prefix showroom run lint`, `… run typecheck`,
   `npm --prefix showroom test`, `scripts/check-ux-traceability.sh`.

Schritte 4–7 gehören in **einen** Commit: Regel-IDs und die nach ihnen benannten
Tests müssen zusammen landen, sonst ist das Traceability-Gate in genau einem
Commit rot — das fordert das Regelbuch selbst (Zeile 13).

---

## 7. Abnahme

**Gate.** `MERGE_READINESS_BASE_REF=origin/dev scripts/check-merge-readiness.sh --no-fetch`
grün vor dem PR.

**Suite.** `npm --prefix showroom test` grün.

**Echte Messung** (nach dem Codex-Lauf): headless Chrome über den
Remote-Debugging-Port auf den Vite-Preview, pro Platte
`getBoundingClientRect().height` in Ruhe und mit erzwungenem Hover.

| Platte | Ruhe | Hover |
|--------|------|-------|
| _wird nach dem Lauf eingetragen_ | | |

**Sichtprüfung.** Je ein Screenshot in Ruhe und mit Hover, damit Hub, Bildzoom,
Lupenzeichen und der weiterhin sitzende Visualizer einmal ein Mensch gesehen hat
— der Prüfstand sieht nur Text.

---

## 8. Fallen

- **`sweep` ist nicht `sheen`** — vier Zeilen daneben, fast gleicher Name; wer
  ihn mitnimmt, nimmt elf Platten den Ladeindikator (§2.6).
- **Der Bildwickel muss `display: block` sein** — sonst Zeilenbox mit
  Unterlänge, Kasten höher als das Bild, Visualizer-Prozente verschoben (§2.2).
- **Der Höhentest kann nicht messen** — `dist` hat kein Layout, ein als
  Layout-Test geschriebener Test ist immer grün (§1.2).
- **Das Traceability-Skript sieht `.mjs` nicht** (§3.1).
- **`p` ist ein gültiges Regelpräfix** — ein ungeankerter Grep findet `p-2-…` in
  irgendeinem Regex und meldet eine unbekannte Regel (§3.1).
- **Die Suite läuft im PR heute nicht** — ohne §5.2 ist jeder neue Test
  Dekoration (§1.3).
- **Zwei Showroom-Branches sind unterwegs.**
  `feature/showroom-plate-plays-the-visualizer` und
  `feature/showroom-seek-track` fassen `HeroProduct.tsx`, `showcase.css` und
  `product-gallery.test.mjs` an. Ihr Inhalt steht laut Baum bereits auf
  `origin/dev` (`showroom/src/visualizer/` ist da) — ihre Diffs gegen
  `origin/dev` sind gegen eine ältere Merge-Basis gerechnet. Vor dem Landen
  prüfen, ob eine der beiden noch offen ist und `shot-tile.css`, `ShotTile.tsx`
  oder `MeasuredSeekTrack.tsx` anfasst.

---

## Parallelität

**Der Plan wird nicht geschnitten. Ein Strang.**

Die Arbeit zerfällt scheinbar sauber in zwei Gruppen:

- A: `showroom/src/components/**`, `showroom/tests/**`
- B: `docs/ux-rules.md`, `scripts/check-ux-traceability.sh`,
  `scripts/check-project-quality.sh`, `LICENSES/**`, `LICENSING.md`

Die Dateigruppen sind disjunkt — der Schnitt scheitert nicht daran, sondern an
der Verifikation. `scripts/check-ux-traceability.sh` kann in **keinem** der
beiden Zweige grün werden:

- A allein: die Tests nennen `show-1` … `show-5`, das Dokument kennt sie nicht →
  *„test references unknown rule SHOW-1"*.
- B allein: fünf `[active]`-Regeln, und die abdeckende Testquelle liegt im
  anderen Zweig → *„[active] rule SHOW-1 has no rule-named test"*.

Beide Zweige wären fertig und korrekt und könnten ihr eigenes Gate trotzdem
nicht grün fahren — genau die Konstellation, an der der Flathub-Strang D am
11.08.2026 hängengeblieben ist. Der Schnitt schöbe die Prüfung vollständig
hinter den Merge, und das Regelbuch verlangt in Zeile 13 das Gegenteil: Regel
und Test im selben Commit.

Dazu kommt: B ist klein — ein Dokumentabschnitt, zwei Skriptzeilen, eine
Lizenzdatei. Der Schnitt kaufte keine Wanduhr, sondern kostete eine
Merge-Reihenfolge und eine Prüfung, die niemand vor dem Merge fahren kann.

Die Textkürzung (§5.3) ist zwar unabhängig, aber zu klein für einen eigenen
Strang — sie wird ein eigener Commit im selben Branch.

**Merge-Reihenfolge:** entfällt.
**Nach-Merge-Gegenprüfungen:** entfallen — es gibt keine Strang-Grenze.
