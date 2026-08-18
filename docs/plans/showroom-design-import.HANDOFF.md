# Handover — Showroom: die Reparaturrunde ist drin, der Stapel wartet aufs Landen

Stand: 18.08.2026, 18:20. Geschrieben für eine frische Sitzung nach `/clear`.
**Es läuft nichts mehr.** Kein Codex, kein Preview-Server, keine offenen Agenten.

## Wo die Arbeit steht

Drei Zweige, aufeinander gestapelt, **19 Commits über `origin/dev`**:

```
origin/dev
  └─ feature/showroom-plate-plays-the-visualizer   (Visualizer-Platte)
       └─ feature/showroom-seek-track              (gemessene Seek-Spur)
            └─ feature/showroom-design-import      (die Design-Fassung)  ← Spitze
```

Worktree der Spitze: `/home/marvin/Projects/reprise-showroom-design-import`,
HEAD `d41c235fa5`.

Der Auftraggeber hat entschieden: **alles in einem Rutsch landen**, in genau
dieser Reihenfolge. Nichts ist gepusht.

| Zweig | Stand |
|---|---|
| Visualizer | fertig, abgenommen, im Stapel enthalten |
| Seek-Spur | fertig, nachgemessen (siehe Belege) |
| Design-Import | **alle neun Stufen gebaut**, Plan auf `phase: reviewed` |

## Was seit dem Review passiert ist

Der Auftraggeber hat **die sechs empfohlenen Befunde** ausgewählt; sie sind
repariert — nicht von Codex (dessen Wochenkontingent stand bei 2 %), sondern in
der Hauptschleife. Commit `d41c235fa5` auf `feature/showroom-design-import`.

| Befund | Reparatur |
|---|---|
| `seekTrack.ts` merkt sich den Fehlschlag | `pendingTrack` wird im `catch` zurückgesetzt |
| Seite hinter dem Dialog nicht `inert` | Lightbox rendert per Portal auf `<body>`, `#showroom-root` bekommt `inert` + `aria-hidden` |
| Zoom-Rücksetzung einen Rahmen zu spät | Zoomzustand hängt am Bildindex, der Effekt ist weg |
| `let _ = cr.paint()` | `cr.paint().unwrap()` |
| Schließ-Hintergrund in der Tab-Reihenfolge | `tabIndex={-1}`, und die Fokusfalle zählt `tabindex="-1"`-Knöpfe nicht mehr mit |
| `fieldset` mit `aria-label` | sichtbar verborgene `<legend>` |

**Ein siebter Befund kam bei der Sichtprüfung dazu und ist mit repariert:** das
Lightbox-Bild wurde beschnitten statt eingepaßt. Die `width`/`height`-Attribute
wirken als gesetzte Größen, die `max-width`/`max-height` nur beschneiden, und
der Zoom-Knopf, den der Port um das Bild gelegt hat (die Vorlage hatte das
`<img>` als direktes Flex-Kind), hatte keine aufgelöste Höhe. Fix:
`width:auto;height:auto` am Bild, `height:100%` am Knopf.

### Was liegen bleibt

Die fünf kleinen Befunde aus dem Review, bewußt nicht angefaßt: unabbrechbare
rAF-Schleife in `counters.ts`, ungetrackter `setTimeout` in `reveal.ts`,
Ref-Zuweisung im Render-Körper von `MeasuredSeekTrack.tsx`, kein
`AbortController` auf dem geteilten Seek-`fetch`, `u32::try_from`-Panik in
`waveform.rs`.

### Neuer Befund, nicht repariert: die Sprossen brechen auf dem Handy

Bei 390 px Viewport ist `.rungs__rung` **560 px breit** in einem 347 px breiten
`.rungs`-Container; alles darüber wird an `body { overflow-x: hidden }`
abgeschnitten. Die dritte Spalte („CAN PROVE …") ist auf dem Telefon
unlesbar. Das ist **Bestand des Design-Imports**, nicht der Reparaturrunde, und
braucht eine Gestaltungsentscheidung (umbrechen? scrollen? stapeln?).

## Nächster Schritt

**Landen**, Reihenfolge Visualizer → Seek-Spur → Design-Import, mit Rebase auf
das frisch gewordene `dev` dazwischen. `land.sh` liegt unter
`~/.claude/skills/pipeline/scripts/`. Nichts ist gepusht.

## Belege der Reparaturrunde (18.08., nachmittags)

- **Gates auf der Spitze nach dem Fix:** `npm test` **34/34** (32 vorher, zwei
  neue Wächter), `npm run typecheck` Exit 0, `npm run lint` Exit 0,
  `cargo check -p reprise-gnome --tests` Exit 0.
- **Mutationsprobe:** die `pendingTrack = undefined;`-Zeile entfernt →
  `tests/seek-track.test.mjs` rot (1 von 2), Zeile zurück → grün. Der Wächter
  ist ein echter Verhaltenstest: er treibt `loadSeekTrack()` erst durch einen
  503er und dann durch einen Erfolg.
- **Sichtprüfung Desktop (1440×900):** Portal hängt an `<body>`,
  `#showroom-root` trägt `inert` + `aria-hidden`, Fokus liegt auf „Close",
  Schließ-Hintergrund `tabIndex -1`. Bild 1251×760 im 1381×760-Feld,
  Seitenverhältnis 1,647 gegen natürliche 1,648 — der Rahmen sitzt am Bild.
- **Zoom-Befund gemessen:** zoomen → `scale(2.1)`, Pfeil rechts → das nächste
  Bild kommt im selben Render mit `transform: none` und Ursprung `center`.
- **Escape:** Lightbox weg, `inert` und `aria-hidden` weg, `overflow`
  zurückgesetzt, Fokus zurück auf der auslösenden Kachel.
- **Sichtprüfung Handy (390×844):** Bild 362×221, Verhältnis 1,643, paßt in
  362×720. Modus-Pille 356 px breit in 390 px, `<legend>` mißt 1×1 px und ist
  per `clip-path: inset(50%)` verborgen; das `aria-label` am `fieldset` ist weg.

## Belege der Vorsitzung — alle selbst gefahren, nicht Codex geglaubt

- **Seek-Spur:** `seek-track.bin` = 2004 B; Kopf-`u32` = 369 786 ms, und das
  ist auf die Millisekunde die Länge der Quelldatei (`ffprobe`: 369,786 s).
  Pegel 1–255 über 163 verschiedene Werte, Centroid 0–255 über 228; längster
  konstanter Lauf 4 bzw. 17.
- **Paritätswächter:** `SILENCE_RMS` in `crates/reprise-view/src/waveform.rs`
  verstellt → Test rot (`SILENCE_RMS drifted from Rust`) → zurückgestellt →
  grün. Alle acht Konstanten sind abgedeckt.
- **Gates auf der Spitze:** `npm test` 32/32, `npm run typecheck` Exit 0,
  `npm run lint` Exit 0.
- **Sichtprüfung** im Browser über alle Kapitel: Hero, Tempo-Band, CH.01,
  CH.02, Mosaik, CH.03, CH.04, CH.05, Fußzeile — alles vorhanden und dem
  Entwurf entsprechend.

## Fallen der Reparaturrunde

- **`cargo check` wird vom Lastregler-Hook still abgefangen.** Kein Fehler, kein
  Text, Exit 0, leeres Log — es läuft schlicht nicht. Über `heavy-run medium --`
  fahren, und weil `heavy-run` die stderr des Kindes frißt, das Kind selbst
  umleiten lassen: `heavy-run medium -- sh -c 'cargo … > log 2>&1'`.
- **Der sichtbare Chrome kann nicht unter 578 px breit werden**, `set_viewport`
  meldet trotzdem Erfolg. Ausweg: eine Wegwerf-Seite in `dist/` mit einem
  `<iframe>` der Zielbreite (gleicher Origin, also greift `contentDocument`);
  ein `transform: scale()` darauf ändert den inneren Viewport nicht, so paßt
  auch ein 1440er Rahmen ins Fenster. `dist/` ist ignoriert.
- **Die Schnappschüsse hinken den DOM-Änderungen hinterher.** Nach einer
  Layout-Verschiebung zeigt das Bild noch das alte Kapitel, während die
  Messung schon das neue meldet. Was belegt werden soll, statt dessen per
  `position: fixed` in den sichtbaren Bereich holen.
- **`cp` fragt zurück und hängt das Skript auf** (Alias `cp -i`). Für
  Wiederherstellungen `command cp -f`.
- **`pkill -f "vite preview"`** trifft die eigene Kommandozeile mit.

## Fallen, die die Vorsitzung gekostet haben

- **Der Design-Export löst die MCP-Sperre.** Die Vorlage liegt jetzt getrackt
  unter `docs/design/reprise-showroom.design.html` (1666 Zeilen) mit
  `docs/design/README.md` als Leseführer. Sie kam aus dem Browser-Export als
  JSON-String im `<script type="__bundler/template">`. Damit baut **Codex** den
  Design-Import, nicht mehr die Hauptschleife.
- **`npm run typecheck` läuft in `pages.yml`.** Rot heißt: der Showroom-Deploy
  bricht. Stufe 1 hatte ihn rot hinterlassen, Codex hat es als „vorher schon
  da" abgehakt. Der Typecheck gehört in jede Belegliste.
- **Der Browser-Prüfstand liefert nach dem Scrollen nur schwarze Bilder.**
  `window.scrollTo` wirkt im DOM, aber der Schnappschuß bleibt leer. Ausweg:
  nicht scrollen, sondern das Layout verschieben
  (`document.getElementById('main-content').style.marginTop = '-6100px'`) und
  bei Scrollposition 0 aufnehmen. Dazu `[data-reveal]{opacity:1!important}`
  einspritzen und `loading="lazy"` auf den Bildern aufheben, sonst sieht man
  leere Rahmen.
- **Zähler mitten im Hochzählen sehen aus wie falsche Zahlen.** Der Screenshot
  zeigte 345'872, die Quelle führt 347'842. Vor dem Melden einer
  „widersprüchlichen Zahl" immer in `src/data/measurements.ts` nachsehen.
- **Der Lastregler war 6/6 belegt** (fremde `cargo test --workspace`-Läufe).
  Ein Codex-Start hängt dann still in der Warteschlange — das ist kein
  Fehlstart, nur Warten. `heavy-run status` zeigt es.
- **`.pipeline-codex.md` ist getrackt** und konfligiert bei jedem Rebase.
  Vor dem Rebase `git checkout -- .pipeline-codex.md`.
- Der Lastregler blockt schon den **Kommandotext**: sobald `codex-run.sh` im
  Befehl steht, greift `heavy-run-gate.sh` — auch bei `head`/`sed` darauf.
  Lesen mit dem Read-Werkzeug, Starten mit `heavy-run medium -- …`.

## Betrieb

- **Wake-Lock freigegeben.** Für den nächsten unbeaufsichtigten Lauf wieder
  einen nehmen: `wake-lock acquire showroom-design-import "…"`.
- Die vier Codex-Aufträge der Vorsitzung (Stufen 1–3, 0/4–6, 7–9, Nachleuchten)
  lagen im Sitzungs-Scratchpad und sind mit ihr weg; sie sind abgearbeitet.
- Die Reparaturrunde ging **nicht** an Codex: dessen Wochenkontingent stand bei
  2 %. Sie wurde in der Hauptschleife programmiert.
