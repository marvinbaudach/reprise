# Handover — Showroom: Reparaturrunde drin, ein Anzeigefehler ungeklärt

Stand: 18.08.2026, abends. Geschrieben für eine frische Sitzung nach `/clear`.
Es läuft **ein Dev-Server** (siehe „Betrieb"), sonst nichts.

Diese Übergabe löst `showroom-design-import.HANDOFF.md` ab; die ältere Datei
bleibt für die Vorgeschichte (Design-Import, Review, Belege) gültig.

## Wo die Arbeit steht

```
origin/dev
  └─ feature/showroom-plate-plays-the-visualizer   (Visualizer-Platte)
       └─ feature/showroom-seek-track              (gemessene Seek-Spur)
            └─ feature/showroom-design-import      (die Design-Fassung)  ← Spitze
```

Worktree der Spitze: `/home/marvin/Projects/reprise-showroom-design-import`,
HEAD `6b0511681f`, **21 Commits über `origin/dev`**. Nichts ist gepusht.
Landen soll alles in einem Rutsch, in genau dieser Reihenfolge, mit Rebase auf
das frisch gewordene `dev` dazwischen.

Drei Commits sind in dieser Sitzung dazugekommen:

| Commit | Inhalt |
|---|---|
| `d41c235fa5` | die sechs ausgewählten Review-Befunde + ein siebter aus der Sichtprüfung |
| `4c3aac62bd` | Fortschreibung der alten Übergabe |
| `6b0511681f` | Visualizer in der Vollansicht + Reveal-Fix |

**Gates auf der Spitze:** `npm test` 36/36, `npm run typecheck` 0,
`npm run lint` 0, `cargo check -p reprise-gnome --tests` 0.

## Was diese Sitzung gebaut hat

Die Reparaturrunde ging **nicht** an Codex (Wochenkontingent stand bei 2 %),
sondern in die Hauptschleife.

**Die sechs ausgewählten Befunde** (`d41c235fa5`):

| Befund | Reparatur |
|---|---|
| `seekTrack.ts` merkt sich den Fehlschlag | `pendingTrack` wird im `catch` zurückgesetzt |
| Seite hinter dem Dialog nicht `inert` | Lightbox rendert per Portal auf `<body>`, `#showroom-root` bekommt `inert` + `aria-hidden` |
| Zoom-Rücksetzung einen Rahmen zu spät | Zoomzustand hängt am Bildindex, der Effekt ist weg |
| `let _ = cr.paint()` | `cr.paint().unwrap()` |
| Schließ-Hintergrund in der Tab-Reihenfolge | `tabIndex={-1}`, Fokusfalle zählt `tabindex="-1"` nicht mehr mit |
| `fieldset` mit `aria-label` | sichtbar verborgene `<legend>` |

**Ein siebter, bei der Sichtprüfung gefunden:** das Lightbox-Bild wurde
beschnitten statt eingepaßt. Die `width`/`height`-Attribute wirken als gesetzte
Größen, die `max-*` nur beschneiden. Fix: `width:auto;height:auto` am Bild,
`height:100%` am Zoom-Knopf.

**Visualizer in der Vollansicht** (`6b0511681f`): die Aufnahme erklärt jetzt
selbst, daß sie einen Teller trägt (`visualizer: true` in `showcase.ts`), und
die Lightbox rendert denselben `VisualizerPlate`. Ein `.lightbox__frame` hält
Seitenverhältnis und Zoom-Transform, damit Bild und Teller nicht auseinander
laufen können. Nachgemessen: Canvas bei 19,63 % / 24,46 % / 60,74 % / 27,87 %
des Bildes — bildgleich mit der Kachel — und die Pixelsumme ändert sich
zwischen zwei Abgriffen, es läuft also wirklich.

**Reveal-Fix** (`6b0511681f`): `prepareReveals()` versteckte bei jedem erneuten
Lauf alles unterhalb der Falz wieder, während `reveal()` wegen `data-shown`
nichts mehr aufdeckte. Jeder Hot-Reload, jede geänderte Bewegungs-Voreinstellung,
jeder Re-Run des Effekts machte die Seite damit dauerhaft leer. `prepareReveals`
überspringt jetzt, was schon gezeigt wurde. **Das war ein echter Fehler, aber
offenbar nicht der, den der Auftraggeber sieht.**

## Gelöst: der Hintergrund malte über den Text

**Ursache:** `.backdrop-ground` war `position: fixed; z-index: 0` mit deckender
Hintergrundfarbe — und liegt im selben Stapelkontext wie der Seiteninhalt
(`.page` ist `position: relative; z-index: 1`). Nach CSS-Malordnung wird ein
positioniertes Element mit `z-index: 0` **nach** dem gesamten nicht
positionierten Fließinhalt gezeichnet. Die Ebene deckte damit jede Überschrift
und jeden Absatz zu; stehen blieb genau das, was selbst positioniert ist:
Kopfleiste, Navigation, die Produktkacheln. Kein Compositor-Fehler, keine
Erweiterung, kein Brave — korrektes CSS mit falscher Ebenenwahl.

Warum es wie „verschwindet nach dem Start" aussah: im Dev-Betrieb wird
`backdrop.css` erst mit seiner Komponente nachgeladen, der Text ist also kurz zu
sehen und wird dann zugedeckt.

**Fix:** alle drei festen Hintergrundebenen auf `z-index: -1`
(`showroom/src/components/chrome/backdrop.css`). Wächter:
`tests/backdrop-design.test.mjs` — jede `position: fixed`-Regel dort muß einen
negativen `z-index` tragen; Mutationsprobe bestanden.

**Wie es gefunden wurde:** eine eigene Brave-Instanz mit
`--remote-debugging-port` und Wegwerf-Profil, darin der Fehler reproduziert
(also nicht profil- oder erweiterungsgebunden), dann eine Bisektion, die die
drei Hintergrundebenen nacheinander abschaltet und nach jedem Schritt einen
Ausschnitt des Hero-Kopfs abgreift. Schritt 3 — Grundfarbe aus — brachte den
Text zurück.

### Drei Fehler, die auf dem Weg dorthin mitgefunden wurden

| Befund | Reparatur |
|---|---|
| Der Dev-Server hydriert gegen ein leeres `#root` — React verwarf bei **jedem** Start den Baum und baute ihn neu | `entry-client.tsx`: `hydrateRoot` nur mit Prerender-Markup, sonst `createRoot` |
| Der Reveal-Durchlauf kannte nur **einen** Nachzügler-Anlauf nach 400 ms; jede spätere Layout-Änderung ließ Inhalt unsichtbar | `usePageChoreography.ts`: `ResizeObserver` auf der Seite plus `document.fonts.ready` |
| Eine fehlschlagende Nebenwirkung im Sweep kippte die ganze Warteschlange — alles dahinter blieb für immer versteckt, auch beim Scrollen | `reveal.ts`: erst alle Eintretenden zeigen, dann jede Nebenwirkung einzeln abgesichert |
| Die Zahlen der Seite standen im Dev-Betrieb auf `0`: der doppelte Effektlauf las die schon genullte Zahl als Ziel | `counters.ts`: `prepareCounter` liest die Zahl genau einmal aus dem Markup |

Wächter dazu: `tests/reveal-sweep.test.mjs` (3), `tests/counters.test.mjs` (2),
beide mit Mutationsprobe. Gates auf der Spitze: `npm test` 42/42, `npm run lint`
0, `npm run typecheck` 0.

## Refaktorierung: der Frame-Pfad

Gemessen mit einer eigenen Brave-Instanz über CDP: ein gescripteter Lesedurchlauf
(180 Frames, Scrollen plus Zeigerbewegung) und ein Ladepfad, jeweils Median aus
drei Läufen. **Frame-Zeiten sind im Dev-Bau rauschdominiert** (max 21 bis 267 ms
bei identischem Code) — belastbar sind nur die Renderer-Zähler, und die sind auf
die Einheit reproduzierbar.

| | Layouts | Style-Neuberechnungen | Style-Zeit |
|---|---|---|---|
| Ladepfad vorher | 172 | 396 | 74,1 ms |
| Ladepfad nachher | **86** | **213** | 66,6 ms |
| Lesedurchlauf vorher | 206 | 573 | 12,6 ms |
| Lesedurchlauf nachher | 206 | **392** | 9,7 ms |

Vier Eingriffe:

- **`reveal.ts` — Lesen vor Schreiben.** `prepareReveals` und `sweepReveals`
  maßen ein Element, schrieben Stile, maßen das nächste. Jeder Wechsel erzwingt
  ein neues Layout; beim Laden trifft das alle 93 Elemente auf einmal. Jetzt
  erst alle Kästen lesen, dann alle Stile schreiben.
- **`reveal.ts` — `will-change` nur für das, was sich bewegt.** Vorher bekam die
  ganze Warteschlange eine Ebenen-Zusage, auch Elemente, die schon an Ort und
  Stelle stehen und nie animieren.
- **`usePageChoreography.ts` — Navigationsziele einmal auflösen.** Statt pro
  Link pro Frame ein `getElementById` über das Dokument.
- **`usePageChoreography.ts` — Zeigerbewegung in den Frame.** `pointermove`
  feuert öfter als der Bildschirm zeichnet; der Zeiger merkt sich jetzt nur die
  Position, geschrieben wird einmal pro Frame. Dazu `scrollHeight` je Frame nur
  noch einmal, ungültig gemacht vom `ResizeObserver`.

**Favicon:** `public/brand/favicon.svg` lag längst da, war aber nie verlinkt —
daher der `favicon.ico`-404 in jeder Konsole. `index.html` verlinkt sie jetzt
als `icon` und `apple-touch-icon`; Vite setzt beim Bauen die Basis `/reprise/`
davor (im `dist/index.html` nachgeprüft).

## Was bewußt liegen bleibt

- **Die fünf kleinen Review-Befunde:** unabbrechbare rAF-Schleife in
  `counters.ts`, ungetrackter `setTimeout` in `reveal.ts`, Ref-Zuweisung im
  Render-Körper von `MeasuredSeekTrack.tsx`, kein `AbortController` auf dem
  geteilten Seek-`fetch`, `u32::try_from`-Panik in `waveform.rs`.
- **Die Sprossen brechen auf dem Handy:** bei 390 px ist `.rungs__rung` 560 px
  breit in einem 347 px breiten Container; die dritte Spalte („CAN PROVE …")
  wird an `body { overflow-x: hidden }` abgeschnitten. Bestand des
  Design-Imports, braucht eine Gestaltungsentscheidung.
- **`#hero` gibt es im DOM nicht** — der Hero heißt `#rp-top`. Damit ist der
  `isHero()`-Zweig in `reveal.ts` (kürzere Anfahrt für den Hero) toter Code.
  Kein Fehler, aber eine Absicht, die nicht greift.

## Belege dieser Sitzung

- **Mutationsprobe:** `pendingTrack = undefined;` entfernt →
  `tests/seek-track.test.mjs` rot (1 von 2), Zeile zurück → grün. Der Wächter
  ist ein Verhaltenstest: er treibt `loadSeekTrack()` durch einen 503er und
  danach durch einen Erfolg.
- **Lightbox Desktop (1440×900):** Portal an `<body>`, `#showroom-root` mit
  `inert` + `aria-hidden`, Fokus auf „Close", Schließ-Hintergrund `tabIndex -1`.
  Bild 1251×760 im 1381×760-Feld, Verhältnis 1,647 gegen natürliche 1,648.
- **Zoom:** zoomen → `scale(2.1)`; Pfeil rechts → das nächste Bild kommt im
  selben Render mit `transform: none` und Ursprung `center`.
- **Escape:** Lightbox weg, `inert`/`aria-hidden` weg, `overflow` zurück, Fokus
  zurück auf der auslösenden Kachel.
- **Handy (390×844):** Bild 362×221, Verhältnis 1,643, paßt in 362×720.
  Modus-Pille 356 px in 390 px, `<legend>` 1×1 px und per
  `clip-path: inset(50%)` verborgen, `aria-label` am `fieldset` weg.

## Fallen dieser Sitzung

- **Die Shell kommt nicht ans Netz.** `curl http://localhost:5173/…` liefert
  `000`, der Browser dagegen lädt dieselbe URL. Alles, was den Dev-Server
  befragen soll, muß über den Browser laufen (`fetch` im `eval`), nicht über die
  Shell. Das erklärt auch, warum Brave headless leer blieb.
- **`cargo check` wird vom Lastregler-Hook still abgefangen:** Exit 0, keine
  Zeile Ausgabe, es läuft nicht. Über `heavy-run medium -- sh -c '… > log 2>&1'`
  fahren; der innere Redirect ist nötig, weil `heavy-run` die stderr des Kindes
  frißt.
- **Der sichtbare Chrome geht nicht unter 578 px Breite**, `set_viewport` meldet
  trotzdem Erfolg. Ausweg: eine Wegwerf-Seite mit einem `<iframe>` der Zielgröße
  (gleicher Origin → `contentDocument` erreichbar); ein `transform: scale()`
  darauf ändert den inneren Viewport nicht.
- **Die Seite hat `scroll-behavior: smooth`.** `scrollTop = …` wirkt verzögert;
  Messungen unmittelbar danach sehen `0` und lesen sich wie „scrollt nicht".
  `scrollTo({top, behavior:'instant'})` nehmen.
- **Im iframe scrollt `body`, nicht `window`** (`body { overflow: hidden auto }`).
- **Schnappschüsse hinken DOM-Änderungen hinterher:** nach einer
  Layout-Verschiebung zeigt das Bild noch das alte Kapitel. Was belegt werden
  soll, per `position: fixed` in den sichtbaren Bereich holen statt zu scrollen.
- **`cp` fragt zurück und hängt das Skript auf** (Alias `cp -i`) — `command cp -f`.
- **`pkill -f "vite preview"`** trifft die eigene Kommandozeile mit.

## Betrieb

- **Ein Dev-Server läuft** aus diesem Worktree auf **Port 5199**
  (`http://localhost:5199/reprise/`), gestartet mit `npm run dev -- --port 5199`.
  Er fährt garantiert den Stand `6b0511681f`. Zum Beenden den Port suchen
  (`ss -ltn | grep 5199`) und den Prozeß killen — nicht per `pkill -f vite`,
  das trifft die eigene Kommandozeile.
- **Kein Wake-Lock genommen.** Für unbeaufsichtigte Läufe:
  `wake-lock acquire showroom-design-import "…"`.
- Wegwerf-Rahmen unter `showroom/public/__t/` und `showroom/dist/__*.html`
  wurden entfernt; `git status` ist sauber.
