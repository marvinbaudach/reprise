---
slug: showroom-review-four-changes
worktree: /home/marvin/Projects/reprise-showroom-review-four-changes
branch: feature/showroom-review-four-changes
phase: planned
codex_session:
created: 2026-08-20
---

# Vier Eingriffe in den Showroom

> Quelle: Design-Projekt „Reprise Showroom Review" (Claude Design,
> `Reprise Showroom Review.dc.html`, Abschnitt 05 „Prompt für den Agenten").
> Erhoben gegen den Arbeitsbaum auf `dev` am 20.08.2026.

**Ziel.** Vier Änderungen an `showroom/`, jede mit eigenem Commit: der
Platten-Hover verliert den Zoom, die Gate-Grafik in CH.02 wird eine Strecke
statt eines Kamms, der Header bekommt einen Ausgang, und der Ledger wird auf
dem Handy lesbar.

**Rahmen (gilt für alle vier Aufgaben).**

- React 19 + TypeScript auf Vite, eine CSS-Datei pro Komponente, keine
  Animationsbibliothek. Keine neue Abhängigkeit, kein neues Design-Token in
  `src/styles/tokens.css`, keine Rust-Crate anfassen.
- Jede Farbe, Dauer und Kurve kommt aus `src/styles/tokens.css` oder aus
  Werten, die in der bearbeiteten Datei schon stehen.
- Kommentarstil beibehalten: Kommentare begründen, warum ein Wert der Wert
  ist, sie beschreiben nicht, was der Code tut.
- **Die Suite ist statische Analyse über `dist/` und die Quelltexte.** Jede
  hier genannte Testanpassung ist Teil der Aufgabe, nicht Nacharbeit. Grün
  heißt: `npm run lint`, `npm run typecheck`, `npm run test` in `showroom/`
  (`test` baut vorher selbst).
- Die Dateilisten sind Startpunkt, kein Zaun. Wenn eine Zusicherung eine
  Datei außerhalb der Liste braucht, ändere sie und schreib es in den
  Commit — halte nur an, wenn der Vertrag selbst falsch ist.

**Beschlüsse des Nutzers (20.08., vor dem Plan).**

1. **Kein `mailto:` auf der Seite.** Der Review schlägt für Header-CTA und
   Hero-Zeile eine Mailadresse vor; es gibt im Repo keine, und eine private
   Adresse gehört nicht ins öffentliche HTML auf GitHub Pages. Beide Links
   zeigen stattdessen auf die bestehende „Open to work"-Sektion im Footer
   (`#availability`).
2. **Alle vier Pakete**, in der Reihenfolge unten.

---

## 1 — Platten-Hover: Kante und Schatten, kein Zoom, kein Lift

**Dateien:** `showroom/src/components/showcase/shot-tile.css`,
`showroom/tests/gallery-hover.test.mjs`

Heute setzt `.shot-tile:hover` `--shot-zoom: 1.045` und `--plate-lift: -6px`.
Beide fallen weg. Ein auf 1.045 skalierter Screenshot beschneidet die
Fensterkante, die „native App" überhaupt erst belegt, und resampelt die Schrift
im Bild; der Lift ist überflüssig, sobald der Schatten die Erhebung trägt.

### Umbau

Die Struktur „einmal geschrieben, zweimal aufgerufen" (Hover in
`@media (hover: hover)`, `:focus-visible` außerhalb) bleibt genau so. Nur die
Tokens wechseln:

```css
.shot-tile {
  --edge: oklch(32% 0.018 269);
  --edge-inner: transparent;
  --glow: transparent;
  --ambient: oklch(4% 0.02 269 / 0.95);
  --ambient-geom: 0 40px 90px -50px;
  --cue-opacity: 0;
  --cue-rise: 5px;

  border: 1px solid var(--edge);
  box-shadow:
    inset 0 0 0 1px var(--edge-inner),
    var(--ambient-geom) var(--ambient),
    0 0 40px -12px var(--glow);
  transition:
    box-shadow 420ms ease,
    border-color 320ms ease;
}
```

Zustand für Hover **und** `:focus-visible`, in beiden Blöcken identisch
(sieben Deklarationen, ausschließlich Custom Properties):

```css
  --edge: oklch(58% 0.09 195);
  --edge-inner: oklch(74% 0.10 195 / 0.30);
  --glow: oklch(80% 0.12 190 / 0.34);
  --ambient-geom: 0 26px 54px -22px;
  --ambient: oklch(2% 0.02 269 / 0.98);
  --cue-opacity: 1;
  --cue-rise: 0px;
```

Weiter:

- `transform: translate3d(0, var(--plate-lift), 0)` auf `.shot-tile` entfällt
  ersatzlos, `--plate-lift` und `--shot-zoom` verschwinden aus der Datei.
- **`.shot-tile--phone` muss mitwandern.** Die Regel setzt heute eine eigene
  `box-shadow`-Kurzform und würde damit die Inset- und Glow-Slots wieder
  wegwerfen. Sie setzt künftig nur noch `--ambient-geom: 0 30px 70px -34px`
  und `--ambient: oklch(3% 0.02 269 / 0.95)` (plus die vorhandenen
  `border-color`/`border-radius`/`min-width`).
- `.shot-tile__picture` behält `position: relative; display: block` — der
  Visualizer-Canvas liegt darauf. `transform`, `transform-origin` und die
  900-ms-Transition entfallen; der Doc-Kommentar darüber wird auf den neuen
  Grund umgeschrieben (der Wrap hält Bild und Canvas zusammen; nichts skaliert
  mehr, also muss auch nichts mitreisen).
- Neu: `@media (hover: none) { .shot-tile { --cue-opacity: 0.55; --cue-rise: 0px; } }`
  — auf Touch erscheint der Zoom-Cue heute nie.
- Der `prefers-reduced-motion`-Block bleibt wie er ist und nennt weiterhin
  `.shot-tile`, `.shot-tile__picture`, `.shot-tile__zoom` mit `transform: none`
  und `transition: none`. Er hat jetzt weniger zu neutralisieren, ist aber
  weiterhin der einzige Ort, der den Cue-Rise abschaltet.

### Testvertrag

`tests/gallery-hover.test.mjs`, Test `show-1`: die drei Zusicherungen über
`--shot-zoom` (`.shot-tile__picture` trägt `transform:scale(var(--shot-zoom))`,
jeder `var(--shot-zoom)`-Verbrauch steht in einem `transform`) beschreiben
genau das entfernte Verhalten. Sie werden ersetzt durch:

- `.shot-tile__picture` existiert und deklariert **kein** `transform`;
- im gebauten Stylesheet kommt weder `--shot-zoom` noch `--plate-lift` vor;
- keine `.shot-tile`-Regel mit `:hover` oder `:focus-visible` deklariert
  `transform` — die Platte bewegt sich nicht mehr, der Schatten hebt sie.

Die Kontrolle über `LAYOUT_PROPERTIES` und die `grid-template-rows`-Zusicherung
bleiben unverändert; `show-2` bis `show-5` bleiben wörtlich stehen und müssen
grün bleiben (`show-3` vergleicht Hover und Focus deklarationsweise und
verlangt mindestens sechs Einträge — die sieben Tokens oben erfüllen das).

**Abnahme.** Hovern ändert nur Rahmen und Schatten; das Bild bewegt und
skaliert sich nicht; der Canvas im Hero-Telefon bleibt im Display;
`prefers-reduced-motion` neutralisiert den Cue-Rise weiterhin.

---

## 2 — CH.02: aus dem Kamm wird eine Strecke

**Dateien:** `showroom/src/components/chapters/ChapterTwo.tsx`,
`ChapterTwo.css`, `showroom/vite.config.ts`,
`showroom/src/virtual-modules.d.ts`, `showroom/tests/chapter-two.test.mjs`

`GateStrip` zeichnet heute 27 gleiche 3px-Marken, dann eine dehnbare 1px-Schiene,
dann einen MERGE-Kasten. Bei voller Breite ist die Schiene das größte Element
der Grafik und trägt keine Information, der Agent ist ein Textlabel statt eines
Knotens, und die eigentliche Regel — drei Mutationen müssen die Suite rot
machen — steht in der Prosa darüber, nicht im Bild.

### Struktur

Vier Knoten, drei kurze Verbindungen:

    AI agent (one change) → GATES (n) → MUTATIONS (3) → main

- Wurzel bleibt `.gate-strip` mit `data-blocked` und darunter unverändert
  `.gate-strip__readout` (`role="status"`, `data-tone`). Nur die Zeile
  darüber wird ersetzt: `.gate-strip__row` mit Marken, Schiene und Verdikt
  weicht `.pipeline`.
- `.pipeline` ist eine umbrechende Flex-Zeile mit `gap: var(--gap-line)`.
  Flexibel ist **nur** `.pipeline__node--gates` (`flex: 1 1 200px;
  min-width: 195px`), damit die Verbindungen bei 26px bleiben und keine
  Schiene mehr leerlaufen kann.
- `.pipeline__link` ist 26–34px gepunktete Hairline aus
  `repeating-linear-gradient` in `var(--accent)`, animiert über die
  vorhandene Bewegungsgrammatik der Datei; vor dem letzten Knoten
  `.pipeline__link--merge`, das unter `.gate-strip[data-blocked="true"]`
  auf `var(--second)`, gestrichelt und ohne Animation umschaltet.
- Letzter Knoten: `Merged / main / all green`, bei blockiert
  `Blocked / main / no partial merge`.
- Mutations-Knoten: drei Marken (`aria-hidden`) und die Zeile
  `3 must turn red`.
- Erster Knoten: `Source / AI agent / one change`.
- Die Gate-Anzahl im Kicker kommt aus `GATES.length`, nie als Literal
  (`show-10`).

### Die Marken werden geclustert

Innerhalb von `.pipeline__node--gates` sitzen die Marken in sechs Spalten,
eine je Gruppe aus `GATE_GROUPS`: `flex: 1 1 74px; min-width: 68px`, darunter
der kurze Gruppenname. Jede Marke behält `flex: 1 1 0`, also liest eine Gruppe
als Segmentbalken so breit wie ihr eigenes Label.

- `GATE_GROUPS` hat heute `name`, `line`, `gates` — kein kurzes Label. In
  `showroom/vite.config.ts` bekommt `GateGroupDefinition`/`GateGroup` ein
  Feld `short`, in `GATE_GROUP_ASSIGNMENTS` gefüllt mit `Boundaries`,
  `Distribution`, `Reachable`, `Traceable`, `Green`, `Toolchain`;
  `groupGates()` reicht es durch, `src/virtual-modules.d.ts` deklariert es.
  Die Anzahl je Gruppe bleibt abgeleitet (`group.gates.length`).
  `tests/gate-derivation.test.mjs` muss grün bleiben — wenn es die
  Gruppenform prüft, wandert `short` dort mit hinein.
- **Jede Marke behält ihr heutiges Verhalten exakt:** `data-gate={name}`,
  `data-broken`, `aria-pressed`, `aria-label={NN · name}` mit `NN` als
  Position in `GATES` (nicht als Position in der Gruppe), Klick toggelt,
  Hover und Focus benennen sie im Readout. Klasse bleibt
  `.gate-strip__tick`, Trefferfläche bleibt 44px hoch um eine 26px-Marke.
- **Kein Gate-Name erscheint als sichtbarer Text.** Sichtbar sind nur die
  sechs Gruppenlabels.

`src/lib/mergeGates.ts` wird **nicht** angefasst: `readout()`,
`displayedReadout()` und `toggle()` behalten Signatur und Tests. Das hier ist
eine Markup- und CSS-Änderung auf demselben State. `GateGroups()` (die sechs
`<article class="gate-group">` weiter unten im Kapitel) bleibt ebenfalls
unverändert — `show-18` prüft sie.

### Mobil

Unter 46rem bricht die Zeile zur Spalte um. Dann müssen die Verbindungen nach
unten zeigen statt zur Seite: Gradient auf `180deg` drehen, `width: 100%`,
`max-width: 1px`, `height: 24px`, `margin-inline: auto`.

### Testvertrag

In `tests/chapter-two.test.mjs`:

- `show-6`: die Zusicherung `assert.deepEqual(marks, names)` unterstellt
  Skriptreihenfolge im DOM. Nach dem Clustern ist die DOM-Reihenfolge die
  Gruppenreihenfolge. Ersetzen durch: die Marken sind eine Permutation der
  Namen — jeder Name genau einmal, keine Auslassung, kein Duplikat — **und**
  innerhalb jeder Gruppe stehen sie in Skriptreihenfolge. Die
  `aria-label`-Prüfung (`NN · name` mit `NN` aus der Skriptposition) bleibt
  wörtlich. Das Verbot `assert.doesNotMatch(chapter, /gate-wall/)` bleibt
  stehen: die neuen Klassen heißen `pipeline__*` und `gate-cluster*`, und
  seine Absicht — keine Wand sichtbarer Gate-Namen — gilt weiter.
- `show-21`: `.gate-strip__rail` gibt es nicht mehr. Die beiden Zusicherungen
  über `flex: 1 1 26px` und `min-width: 26px` werden ersetzt durch: es
  existiert keine `__rail`-Regel mehr, und `.pipeline__node--gates` ist der
  einzige Knoten mit `flex: 1 1 …`. Die `max-width`-Verbote für
  `.incident-figure`, `.gate-figure`, `.gate-groups` und die Zusicherungen
  über `.incident-panel__*` bleiben unverändert.
- `show-9`: der Touch-Block `@media (hover: none), (max-width: 46rem)` setzt
  heute `.gate-strip__tick { width: 44px; }`. Mit `flex: 1 1 0` in der Gruppe
  wird daraus `min-width: 44px`; der Test wird auf `min-width` umgeschrieben.
  Die Reduced-Motion-Zusicherung über `.gate-strip__tick` bleibt wörtlich.
- `show-7`, `show-8`, `show-10`, `show-18` bleiben unverändert und müssen
  grün bleiben.

**Abnahme.** Bei 1280px steht die ganze Strecke in einer Zeile ohne leere
Schiene; die sechs Gruppen sind lesbar; ein rot geklickter Check blockiert den
letzten Knoten, färbt die letzte Verbindung und die Live-Region sagt weiterhin
denselben Satz.

---

## 3 — Ein Ausgang im Header, eine Zeile im Hero

**Dateien:** `showroom/src/components/chrome/SiteHeader.tsx`, `chrome.css`,
`showroom/src/components/chapters/Hero.tsx`, `chapters.css`,
`showroom/src/components/chapters/SiteFooter.tsx`

Ein gefüllter Button würde das Register der Seite brechen. Verfügbarkeit wird
deshalb wie jede andere Messgröße behandelt: Mono-Kapitälchen, Hairline,
Statuspunkt.

- `SiteFooter.tsx`: die `<section className="availability">` bekommt
  `id="availability"` (das vorhandene `aria-labelledby` bleibt). Das ist das
  Ziel beider neuen Links — **kein `mailto:`**, siehe Beschluss 1.
- `SiteHeader.tsx`: nach dem Source-Link, innerhalb `<nav>`, ein
  `<span className="site-header__split" aria-hidden="true" />` und ein
  `<a className="site-header__hire" href="#availability">Work with me</a>`.
  Der Link trägt **kein** `data-navlink` und überlebt damit als einziges
  Nav-Element den 46rem-Breakpoint.
- `chrome.css`: `.site-header__hire` ist `inline-flex`, 7px/13px Padding,
  1px Rahmen aus `color-mix(in oklab, var(--accent) 52%, transparent)`,
  `border-radius: var(--radius-sharp)`, `color: var(--text-strong)`,
  Transition über `border-color`/`background-color`/`color` mit
  `var(--duration-fast)`. `::before` ist der 6px-Punkt in `var(--accent)` mit
  weichem Ring (`box-shadow: 0 0 0 3px color-mix(…22%…)`). Hover tönt Rahmen,
  Grund und Schrift aus dem Akzent. `.site-header__split` ist 1px × 18px in
  `var(--surface-line)`.
- Unter 46rem: `.site-header__hire { padding: 9px 14px; }`. Damit die Zeile
  bis 375px einzeilig bleibt, bekommen `.site-header__nav` und
  `.site-header__id` `white-space: nowrap` und `min-width: 0`, und unter
  26.5rem verschwindet das `Alpha`-Abzeichen (`.site-header__state`) — es ist
  Zustandsdekoration, der Ausgang nicht.
- `Hero.tsx`: nach dem Scroll-Cue, innerhalb `.hero__copy`, eine Zeile
  `.hero__offer` mit `data-reveal=""` (die vorhandene Choreografie greift sie
  damit auf), bestehend aus drei Teilen — Zustand, Aussage, Link:

      Available · Q4
      Five weeks, one developer, agents under gate control.
      The same method, your codebase ↓

  Der Pfeil ist bewusst `↓` statt des `↗` aus dem Entwurf: der Link bleibt auf
  der Seite. Sonst ist die Copy wörtlich.
- `chapters.css`: `.hero__offer` ist eine umbrechende Flex-Zeile
  (`gap: 14px 22px`) auf einer 1px-Oberkante aus `var(--surface-line-soft)`,
  `margin-top: 34px; padding-top: 20px`. `.hero__offer-state` ist Mono,
  `letter-spacing: 0.16em`, `uppercase`, `var(--accent)`, mit 6px-Punkt davor.
  `.hero__offer-link` ist Mono in `var(--text-strong)` mit 1px
  `border-bottom` aus dem Akzent, Hover färbt beides auf `var(--accent)`.
  Die Zeile darf nichts über ihr verschieben: sie kommt **nach** dem
  Scroll-Cue.

**Abnahme.** Die Kopfzeile bleibt bis 375px einzeilig; der Hire-Link ist in
jeder Breite sichtbar; im Hero rückt durch die neue Zeile nichts nach oben;
beide Links landen auf der Availability-Sektion.

---

## 4 — Mobil: der Ledger

**Dateien:** `showroom/src/components/chapters/ChapterFive.tsx`,
`ChapterFive.css`

Der Ledger hat vier Spalten, drei davon in Mono-Ziffern. Bei 375px bleiben rund
80px pro Spalte, jede Zelle bricht um, die Zeile liest nicht mehr als eine
Zeile. Horizontales Scrollen würde ausgerechnet die Delta-Spalte verstecken.

- `ChapterFive.tsx`: die Tabelle behält ihr Markup und bekommt die Rollen
  ausdrücklich zurückgeschrieben — `role="table"` auf `<table>`, `role="row"`
  auf den `<tr>`, `role="rowheader"` auf den `<th scope="row">`, `role="cell"`
  auf den `<td>`. Grund: `display: block` auf Tabellenteilen nimmt Safari und
  VoiceOver die Tabellensemantik. Jede Wertzelle bekommt zusätzlich
  `data-label="Before" | "After" | "Delta"`.
- `ChapterFive.css`, neuer Block `@media (max-width: 34rem)`: `.ledger`,
  `tbody`, `tr`, `th`, `td` werden `display: block`; `thead` wird visuell
  versteckt (`position: absolute; width: 1px; height: 1px; overflow: hidden;
  clip-path: inset(50%)`); jede `tbody tr` wird eine Karte — 12px/13px
  Padding, 8px Abstand, 1px Rahmen `var(--surface-line-soft)`, 2px linke
  Kante in `color-mix(in oklab, var(--accent) 46%, transparent)`, 8px Radius,
  Grund aus `var(--surface-raised)` — und zugleich ein
  `grid-template-columns: repeat(3, 1fr)` mit `gap: 8px`, in dem der
  Zeilenkopf `grid-column: 1 / -1` einnimmt. Die Wertzellen verlieren Rahmen
  und Padding und drucken ihr Label über `td::before { content: attr(data-label) }`
  in 0.56rem, `letter-spacing: 0.14em`, `uppercase`, `var(--text-muted)`.
  `caption` wird kleiner.
- Der Preis (`.ledger__price`) bleibt, wo er ist: letzte Zeile derselben
  Liste, nicht Kleingedrucktes.

`tests/headless-ledger-footer.test.mjs` prüft, dass die Tabelle vollständig und
ungefaltet ankommt — das bleibt wahr und der Test unverändert. Wenn die Rollen
oder `data-label` dort eine Zusicherung brauchen, füge sie hinzu, statt eine
bestehende zu lockern.

### Was am Seek-Track ausdrücklich **nicht** getan wird

Der Review verlangt für den Canvas vier Dinge. Drei davon stehen schon im Code
und werden nur verifiziert, nicht neu gebaut:

- DPR-Deckel: `MAX_DEVICE_SCALE = 2` in `src/lib/seekRenderer.ts:69`, benutzt
  in `Math.min(MAX_DEVICE_SCALE, window.devicePixelRatio || 1)`.
- Balkenzahl folgt der Breite: `Math.max(MIN_BAR_COUNT, Math.floor(width / BAR_STEP_PX))`.
- Nicht sichtbar, nicht rechnen: `IntersectionObserver` in
  `MeasuredSeekTrack.tsx` plus `isVisible()` an der Renderer-Registrierung.
- Trefferfläche und Geste: `.seek-track__canvas-frame` ist 148px hoch (Hero:
  46px) und trägt `touch-action: pan-y`.

Offen bliebe allein der `visibilitychange`-Handler — und der wäre wirkungslos:
Browser stellen `requestAnimationFrame` in verborgenen Tabs ohnehin ein, die
RAF-Schleife läuft dort nicht weiter. **Nicht einbauen.** Wenn eine der vier
Zusicherungen oben beim Nachsehen nicht zutrifft, ist das eine echte Aufgabe:
dann einbauen und im Commit benennen.

**Abnahme.** Bei 375px liest der Ledger als drei Karten ohne horizontales
Scrollen, und die Tabellenrollen stehen im ausgelieferten HTML.

---

## 5 — Nachtrag: der Konus im Hintergrund endet in einem Punkt

Nicht Teil des Design-Reviews, sondern am 20.08. beim Draufschauen gefunden und
auf Wunsch des Nutzers an diesen Branch gehängt.

`.backdrop-oil__sweep` in `src/components/chrome/backdrop.css` ist ein
`conic-gradient`. Der interpoliert über den Winkel, also schrumpft die
Pixelbreite jedes Farbübergangs mit dem Radius, und alle sechs fallen im
Mittelpunkt auf einen Punkt zusammen — ein Windrad mit harten Speichen. Die
Schicht ist `position: fixed`, dieser Punkt sitzt daher immer in der
Bildschirmmitte und wandert beim Lesen mit.

**Umbau.** Eine radiale Maske auf denselben Selektor, sonst nichts:
`mask-image: radial-gradient(circle closest-side at 50% 50%, transparent 0 8%, #000 30%)`
(mit `-webkit-`-Zwilling). Prozente statt Pixel, weil die Schicht in `vmax`
bemessen ist — ein festes Loch würde den Konus auf dem Handy schlucken und auf
einem breiten Bildschirm verschwinden. Farben, Winkel, Dauer, Opazität bleiben.

**Abnahme.** Konus allein über Schwarz, Drift eingefroren, gegen den gebauten
`dist/` über CDP bei 390/768/1280/1920: der mittlere Helligkeitssprung pro Pixel
innerhalb von r < 40px fällt von 0,41 auf 0,00, das Fernfeld behält seinen
Charakter. Kontrollarm mit einer überall deckenden Maske zeigt, dass die
zusätzliche Glättung im Fernfeld vom Kompositionspfad kommt und nicht vom Loch.
Kein Testvertrag: die Suite ist statische Analyse, und hier ändert sich weder
Struktur noch Rolle noch Text.

## Reihenfolge und Commits

Vier Commits in dieser Reihenfolge — 1 Hover, 2 Pipeline, 3 Header/Hero,
4 Ledger; Abschnitt 5 kam später dazu und hängt hinten an. Jeder Commit lässt `npm run lint`, `npm run typecheck` und
`npm run test` in `showroom/` grün; die Testanpassungen liegen im selben
Commit wie die Änderung, die sie beschreiben.

## Parallelität

**Kein Schnitt.** Die vier Aufgaben sind zwar dateidisjunkt, aber jede ist
klein, und drei von ihnen ändern Testdateien derselben Suite, die erst nach
`npm run build` gemeinsam grün wird. Ein Strang pro Aufgabe würde vier
vollständige Vite-Builds parallel fahren, um insgesamt vielleicht 300 Zeilen
Diff zu verteilen. Ein Strang, vier Commits.
