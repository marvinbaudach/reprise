# Handover — Showroom: Visualizer gelandet? nein. Design-Import begonnen.

Stand: 18.08.2026, nachmittags. Geschrieben für eine frische Session nach
`/clear`. Drei Zweige sind offen, einer davon hat einen Codex-Lauf in der Luft.

## Die drei Zweige

Alle drei hängen aufeinander: `dev` → Visualizer → (Seek-Spur | Design-Import).
Sie müssen **in dieser Reihenfolge** landen, mit Rebase dazwischen.

| Zweig | Worktree | Stand |
|---|---|---|
| `feature/showroom-plate-plays-the-visualizer` | `../reprise-showroom-plate-plays-the-visualizer` | **fertig, abgenommen, nicht gereviewt, nicht gelandet.** `213d980a29`. 16/16 Tests grün. `phase: coded`, Sichtprüfung als Nachtrag im Plan. |
| `feature/showroom-seek-track` | `../reprise-showroom-seek-track` | **Codex läuft** (seit ~15:0x, PID 417128). Plan: `docs/plans/showroom-seek-track-measured.md`. |
| `feature/showroom-design-import` | `../reprise-showroom-design-import` | **Stufe 1 gebaut**, `d856bb5e26`. Plan: `docs/plans/showroom-design-import.md`. |

Beide Nebenzweige sind von der **Visualizer-Spitze** geschnitten, nicht von
`origin/dev` — sonst fehlte ihnen die Platte bzw. der Kontext.

## Was in der Luft ist

- **Codex auf `feature/showroom-seek-track`.** Log:
  `/tmp/claude-1000/-home-marvin-Projects-reprise/b01bf2d6-*/scratchpad/codex-seek-track.log`,
  Bericht landet in `<worktree>/.pipeline-codex.md`. Ein Monitor (`bwcyz5ep6`)
  meldet das Ende. Prüfen: `kill -0 417128`.
  Auftrag: ein `#[ignore]`-Extraktor über `GstreamerWaveformBackend::extract_render_data`,
  `scripts/render-showroom-seek-track.sh`, das Asset
  `showroom/public/media/showroom/seek-track.bin` (**2004 B**: `u32` LE
  Spieldauer in ms, 1000 Pegel, 1000 Centroid) und ein Test dazu.
  Quelle: `/home/marvin/Music/Lorna Shore/…And I Return to Nothingness (2021)/01 To the Hellfire.flac`.
- **Wake-Lock `showroom-visualizer` hält.** Erst freigeben, wenn kein Lauf mehr
  offen ist.

## Nächste Schritte, in der Reihenfolge

1. **Codex-Ergebnis abnehmen** (Seek-Spur): Bericht lesen, Zahlen prüfen —
   Dateigröße 2004 B, Spieldauer plausibel, beide Spuren nicht konstant.
2. **Design-Import weiterbauen**: §4 des Plans, sechs Stufen, Hero zuerst.
3. **Landen**, in dieser Reihenfolge: Visualizer → Seek-Spur → Design-Import,
   jeweils `/check` davor und Rebase auf das frisch gewordene `dev` dazwischen.

## Fallen, die diese Sitzung gekostet haben

- **Codex kommt an das Design nicht heran.** Die Design-Dateien liegen nur
  hinter dem `claude-design`-MCP; der Sandkasten reicht bis zum Worktree. Was
  Design-Wissen braucht, baut die Hauptschleife; was keins braucht (die
  Meßspur), geht an Codex. Der Download-Weg über die Oberfläche wurde
  angeboten und dann zugunsten des MCP verworfen.
- **Die Design-Datei ändert sich unter der Lesung.** 1569 → 1592 Zeilen
  mitten in der Arbeit. Zeilenbereiche vor dem Zitieren neu holen, Etag
  vergleichen.
- **Der Lastregler blockt schon den Kommandotext.** `heavy-run-gate.sh` schlägt
  an, sobald `codex-run.sh` im Befehl steht — auch bei `head`/`sed` darauf.
  Lesen mit dem Read-Werkzeug, Starten mit `heavy-run medium -- …`.
- **Die pipeline-Skripte liegen nicht im Repo**, sondern unter
  `~/.claude/skills/pipeline/scripts/` (`status.sh`, `worktree.sh`,
  `codex-run.sh`, `land.sh`). `worktree.sh` schneidet immer von `origin/dev` —
  für die beiden Nebenzweige wurde deshalb von Hand
  `git worktree add -b … <basis>` benutzt.
- **`npm ci` je Worktree.** Ohne das bricht `npm run build` mit Code 127 ab.
- **Biome bricht die Testsuite, nicht nur den Lint.** `lint-contract.test.mjs`
  fährt `biome ci`; vor jedem Commit `npx biome check --write .` im
  `showroom/`-Ordner laufen lassen. Es formatiert CSS auf doppelte
  Anführungszeichen um.
- **Die Seite hängt unter `/reprise/`.** Dev-Server:
  `cd showroom && npm run dev` → `http://localhost:5173/reprise/`. Auf `/`
  ist sie leer, das ist kein Fehler.

## Belege dieser Sitzung

- Visualizer: `npm test` im Visualizer-Worktree, **16/16 grün**, selbst
  gefahren (nicht nur Codex' Wort). Bandspur 16 835 B = 259 Bilder à 65 B.
  Codex' Extraktor traf `bands.u8`/`kick.u8` bytegenau.
- Design-Import Stufe 1: `npm run build` grün, `npm test` 16/16, `npm run lint`
  grün, Commit `d856bb5e26`.
- Die Meßbelege des Visualizers liegen weiter in
  `~/.cache/reprise-visualizer-measure/` (7,8 MB, überlebt die Sitzung).
