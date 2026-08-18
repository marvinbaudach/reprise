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

## Der offene Punkt: beim Auftraggeber verschwindet der Hero-Text

**Symptom** (vier Screenshots, Brave, Fenster ~2000 px breit): Kopfzeile und
beide Produktkacheln samt Bildunterschriften sind da, der Visualizer-Teller
läuft — aber **der gesamte `.hero__copy`-Block fehlt** (Eyebrow, `h1`, Lead,
Note, Scroll-Cue), ebenso die Wellenform der Hero-Seek-Spur (nur der türkise
Abspielstrich steht da) und alles unterhalb des Heros. Beim Hochscrollen bleibt
die Seite leer. Tritt „nach dem Seitenstart" auf und überlebt Reloads.

### Was ausgeschlossen ist

- **Nicht der Reveal-Fehler oben.** Nachgestellt bei 2000×1090, 1440×900,
  1000×545 und 390×844, mit Rad-Scrollen und mit `scrollTo`, runter und wieder
  hoch, mit Lightbox auf/zu davor, und nach einem Hot-Reload: jedes Mal
  `gestrandet: 0`, `h1` mit `opacity: 1`, nichts Unsichtbares im Blickfeld.
- **Nicht der falsche Code.** Der Server des Auftraggebers lief aus diesem
  Worktree — Beleg: in seiner Vollansicht steckte das Canvas im
  `.lightbox__frame`, und beides gibt es nur in `6b0511681f`.
- **Nicht meine Portal-Änderung.** `inert`/`aria-hidden`/`overflow` werden beim
  Schließen nachweislich abgeräumt, der Fokus kehrt zurück.
- **Nicht der Produktionsbau.** Auf dem gebauten `preview` ist die Seite über
  alle Kapitel sauber.

### Die beiden heißesten Spuren

1. **Brave.** Der einzige unaufgelöste Unterschied. Der Prüfstand fährt Chrome.
   Brave headless war aus der Sandbox nicht ans Netz zu bekommen (`--screenshot`
   lief zweimal in den Timeout; mit `dangerouslyDisableSandbox` startet er, hängt
   aber). Nächster Versuch: `--headless=old`, oder Brave mit
   `--remote-debugging-port` von Hand starten und von außen ansteuern.
2. **Die Hero-Geometrie paßt nicht zum CSS.** `.hero__grid` hat
   `max-width: 78rem` und ist zentriert; bei 2000 px CSS-Breite müßte das Gitter
   bei x≈376 beginnen und bei x≈1624 enden. Auf dem Screenshot reichen die
   Kacheln bis x≈1860. Entweder ist das Fenster deutlich breiter als der
   Screenshot suggeriert (HiDPI/Zoom), oder eine Regel greift dort anders.
   `.hero__copy` trägt `container-type: inline-size`, und `.hero__headline`
   bemißt sich in `cqi` — wenn der Container dort anders aufgelöst wird als hier,
   lohnt ein genauer Blick. **Zuerst `devicePixelRatio` und `innerWidth` des
   Auftraggebers erfragen.**

### Was als nächstes zu tun ist

Der Auftraggeber wurde zweimal um die Konsolenzeile gebeten, hat sie noch nicht
geliefert. Ohne Zahlen aus *seiner* Seite ist weiteres Raten sinnlos.

```js
(()=>{const a=[...document.querySelectorAll('[data-reveal]')];return{gesamt:a.length,sichtbar:a.filter(e=>+getComputedStyle(e).opacity>.5).length,gestrandet:a.filter(e=>e.dataset.shown&&+getComputedStyle(e).opacity<.5).length,ohneShown:a.filter(e=>!e.dataset.shown&&+getComputedStyle(e).opacity<.5).length,h1:getComputedStyle(document.querySelector('h1')).opacity,vp:[innerWidth,innerHeight],dpr:devicePixelRatio,overflow:document.documentElement.style.overflow,inert:document.getElementById('showroom-root').hasAttribute('inert')}})()
```

Lesart: `gestrandet > 0` → doch der Reveal-Weg, nur greift der Fix nicht.
`ohneShown` hoch bei `h1: "0"` → der Sweep läuft dort gar nicht (Brave bedient
den Scroll-/rAF-Takt anders). `inert: true` ohne offene Lightbox → doch das
Portal. `h1: "1"` bei unsichtbarem Text → gar kein Reveal-Problem, sondern
Schrift, Farbe oder Layout; dann `getBoundingClientRect()` und `color` der
`h1` nachsehen.

Zweite Bitte, die noch offen ist: **dieselbe URL einmal in Chromium oder
Firefox** öffnen. Ist es dort sauber, ist es Brave.

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
