# Handover — Showroom: der Design-Import steht, der Review liegt zur Auswahl

Stand: 18.08.2026, 17:38. Geschrieben für eine frische Sitzung nach `/clear`.
**Es läuft nichts mehr.** Kein Codex, kein Dev-Server, keine offenen Agenten.

## Wo die Arbeit steht

Drei Zweige, aufeinander gestapelt, **18 Commits über `origin/dev`**:

```
origin/dev
  └─ feature/showroom-plate-plays-the-visualizer   (Visualizer-Platte)
       └─ feature/showroom-seek-track              (gemessene Seek-Spur)
            └─ feature/showroom-design-import      (die Design-Fassung)  ← Spitze
```

Worktree der Spitze: `/home/marvin/Projects/reprise-showroom-design-import`,
HEAD `720e26746c`.

Der Auftraggeber hat entschieden: **alles in einem Rutsch landen**, in genau
dieser Reihenfolge. Nichts ist gepusht.

| Zweig | Stand |
|---|---|
| Visualizer | fertig, abgenommen, im Stapel enthalten |
| Seek-Spur | fertig, nachgemessen (siehe Belege) |
| Design-Import | **alle neun Stufen gebaut**, Plan auf `phase: reviewed` |

## Der einzige offene Punkt: die Auswahl aus dem Review

Vier Reviewer (typescript, react, rust, security) sind gegen den ganzen Stapel
gelaufen, Basis `bf546d6cc8`. **Kein kritischer Befund. Security: Freigabe.**
Der Auftraggeber hat noch **nicht** gesagt, welche Befunde repariert werden —
das ist die erste Frage der nächsten Sitzung.

### Wichtig (4)

1. **`showroom/src/lib/seekTrack.ts:28`** — `loadSeekTrack()` merkt sich das
   Versprechen auch im Fehlerfall. Ein einziger geplatzter `fetch` liefert
   jedem späteren Aufrufer dieselbe Absage; beide Seek-Flächen zeigen bis zum
   Neuladen „Measured track unavailable". Fix: `pendingTrack` im `catch`
   zurücksetzen, bevor der Fehler weitergereicht wird.
2. **`showroom/src/components/showcase/Lightbox.tsx:101`** — die Seite hinter
   dem Dialog bekommt kein `inert`/`aria-hidden`. Die Tastaturfalle greift,
   der Lesecursor eines Screenreaders läuft trotzdem in Kopf- und Fußzeile,
   während `aria-modal="true"` das Gegenteil behauptet.
3. **`Lightbox.tsx:30`** — die Zoom-Rücksetzung hängt an einem Effekt auf
   `activeIndex` und kommt damit ein Bild zu spät: hineinzoomen, Pfeiltaste,
   und das nächste Bild blitzt auf 2,1× am alten Ursprung auf. Fix: den Zoom
   an den Index binden (`zoom.index === activeIndex`) statt an einen bloßen
   Wahrheitswert.
4. **`crates/reprise-gnome/src/ui/now_playing/song_visualizer_tests.rs:116`** —
   `let _ = cr.paint();` verschluckt den Cairo-Fehler; danach wird auf eine
   undefinierte Fläche gezeichnet und geschrieben, als wäre nichts. Drei Zeilen
   davor und danach schlägt jeder Fehler laut fehl. Nur Diagnosepfad
   (`REPRISE_VIS_WRITE_RGB=1`), nicht das ausgelieferte Asset.

### Klein (7)

Unabbrechbare rAF-Schleife in `counters.ts` · ungetrackter `setTimeout` in
`reveal.ts` · Ref-Zuweisung im Render-Körper von `MeasuredSeekTrack.tsx:54` ·
kein `AbortController` auf dem geteilten Seek-`fetch` · `fieldset` mit
`aria-label` statt `<legend>` (`MeasuredSeekTrack.tsx:202`) · der unsichtbare
Schließ-Hintergrund der Lightbox liegt in der Tab-Reihenfolge
(`Lightbox.tsx:131`, Fix: `tabIndex={-1}`) · `u32::try_from`-Panik bei
absurden Spieldauern (`waveform.rs:456`).

**Empfehlung der letzten Sitzung:** die vier wichtigen plus die beiden billigen
Barrierefreiheits-Kleinigkeiten (Tab-Reihenfolge, `<legend>`), Rest liegen
lassen — auf einer Einzelseite ohne Router folgenlos.

### Zwei Hinweise ohne Befundcharakter

- Die Seite hat **keine CSP**. Bestand schon vorher; GitHub Pages kann keine
  Header setzen, es ginge nur per `<meta http-equiv>`.
- Die Plandokumente tragen `/home/marvin/…`-Pfade in ein öffentliches Repo.
  Bestehende Praxis, aber beim Merge nach `main` dauerhaft sichtbar.

## Nächste Schritte

1. **Auswahl einholen**, welche Befunde repariert werden.
2. **Reparaturrunde an Codex** (`/refactor` oder ein eigener Auftrag im Stil der
   vier unten). Danach selbst nachfahren: `npm test`, `npm run typecheck`,
   `npm run lint` im Ordner `showroom/`.
3. **Landen**, Reihenfolge Visualizer → Seek-Spur → Design-Import, mit Rebase
   auf das frisch gewordene `dev` dazwischen. `land.sh` liegt unter
   `~/.claude/skills/pipeline/scripts/`.

## Belege dieser Sitzung — alle selbst gefahren, nicht Codex geglaubt

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

## Fallen, die diese Sitzung gekostet haben

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
- Die vier Codex-Aufträge dieser Sitzung (Stufen 1–3, 0/4–6, 7–9, Nachleuchten)
  lagen im Sitzungs-Scratchpad und sind mit ihr weg. Sie sind abgearbeitet; für
  die Reparaturrunde wird ein neuer geschrieben.
