# Handover — Showroom-Platte spielt den Visualizer

Stand: 18.08.2026, zweite Übergabe. Geschrieben für eine frische Session nach
`/clear`. Sie ersetzt die erste Übergabe vom selben Tag; deren Inhalt ist,
soweit noch gültig, hier eingearbeitet.

## Wo die Sache steht

**Plan fertig, gegrillt, und nach einer Messung umgeschrieben.**
`docs/plans/showroom-plate-plays-the-visualizer.md`, `phase: planned`,
325 Zeilen, acht Aufgaben in §5.

**Noch nichts implementiert.** Kein Branch, kein Worktree, keine Codeänderung.
Der Hauptcheckout ist sauber (`git status --porcelain crates/ scripts/` → 0).

**Der Weg hat sich in dieser Sitzung ein zweites Mal gedreht** — vom Video zum
Canvas-Port mit gemessener Bandspur. Warum, steht unten und ausführlich in §2.0
des Plans.

## Was in dieser Sitzung passiert ist

1. **Design-Import angesehen.** Claude-Design-Projekt
   `476e3050-d03c-4e6b-b376-448b2c137b03` („Reprise Promotion-Seite"), Datei
   `Reprise Showroom.dc.html`. Ergebnis: das ist keine Bildersammlung, sondern
   eine **komplett neu gebaute Promotion-Seite** — 1569 Zeilen Monolith, mit
   Sektionen, die es im Repo nicht gibt.
2. **Darin lief bereits ein Canvas-Port von `bars.rs`.** Kein Platzhalter: die
   Konstanten stehen mit ihren Rust-Namen daneben, `hsla_to_rgb` ist übersetzt.
3. **Der Auftraggeber hat ihn verworfen** — „der canvas port sieht aus wie auf
   dem design und nicht wie in der app", dann „die bewegung ist komplett
   unsinnig" — und eine Messung an einem echten Song verlangt: „messe doch mal
   nen song von lorna shore".
4. **Gemessen.** Zwölf Sekunden *To the Hellfire* durch die echte Kette. Vier
   Fehler des Ports gefunden und geschlossen, dann pixelweise gegengeprüft.
5. **Plan umgeschrieben** auf Port + gemessene Bandspur. Das Video ist raus.

## Die Entscheidungen, und wer sie getroffen hat

Aus dem Grill (erste Sitzung, alle bestätigt):

1. Kein wasm. Kein neues Crate, keine Extraktion von `visuals/`. Der Befund
   dazu steht in §1 des Plans, samt Zahlen, falls die Anforderung zurückkehrt.
2. Offline im Repo erzeugt — nicht Bildschirmaufnahme, nicht Emulator.
3. Prüfumfang: Tests in `showroom/tests/`, keine UX-Regel-IDs, keine
   Gate-Erweiterung, kein neues Showroom-Merge-Gate.
4. Werkzeug als `#[test] #[ignore]` in `song_visualizer_tests.rs` plus
   `scripts/render-showroom-visualizer.sh`.
5. Keine zusätzliche sichtbare Kopie; `alt`/`description` nachziehen.

Aus dieser Sitzung:

6. **Higgsfield ist vom Tisch.** Der Auftraggeber hat auf Nachfrage
   „Offline-Render, wie geplant" gewählt. Die Begründung gegen ein KI-Video
   (erfundene Bewegung an genau der Fläche, die die eigene Engine belegen soll;
   Flathub/GNOME lehnen KI-Einreichungen seit 29.05.2026 ab; das Repo ist seit
   11.08. öffentlich) muss nicht neu geführt werden.
7. **Video raus, Canvas-Port mit gemessener Bandspur rein.** Entschieden nach
   der Messung.
8. **Der Design-Import geht über die ganze Seite**, nicht nur den Visualizer —
   ausdrücklich so gewählt. Braucht einen **eigenen Plan**; siehe unten.

## Was die Messung ergeben hat

Alles Nötige steht in §2.0 des Plans. Die vier Fehler des Design-Ports in
Kurzform, damit sie beim Bau als Prüfliste dienen:

| Der Port tat | Wahrheit |
|---|---|
| Bänder aus zwei Sinus + Neigung | die Bänder kommen aus CAVA |
| auf Referenzhöhe 300 px zeichnen und skalieren | `bars.rs` rechnet direkt mit `ctx.width`/`ctx.height`; `SEGMENT_GAP` 2,5 und die `+2.0` der Reflexionen sind **absolute** Pixel |
| `bass_impact` = Mittel der ersten sieben Bänder | `bass_impact` ist `self.glow`: `max(glow, kick)` beim Ingest, `−0,06` je Tick (r = 0,12 zur Vermutung) |
| Verlauf endet auf `rgba(0,0,0,0)` | Cairo interpoliert vormultipliziert, Canvas nicht — der Endstop muss dieselbe Farbe tragen |

Danach: **MAE 3,2 von 255** über die Bilder 60/180/300/430, 0,2 % der Pixel
jenseits von 16 Stufen (Balkenkanten, Verlaufsquantisierung).

## Fakten, die teuer zu erheben waren

Damit sie niemand zweimal holen muss.

**Zur Engine:**

- **`engine.rs` glättet die Bänder nicht nach.** Zeile 1 nennt sie
  „already-smoothed", `engine.display_bands == bars` steht als Test fest. Eine
  aufgezeichnete Bandspur speist den Port ohne Zwischenmodell. Das ist der
  Befund, auf dem der ganze Ansatz ruht.
- `engine.tick()` ist genau **ein** Simulationsschritt (`advance_ticks(1.0)`),
  nicht `dt·60`. Im Extraktor kommt ein Tick je Chunk.
- Peak-Kappen: `max(peak, bar)` beim Ingest, **danach** `−0,018` gegen den
  Bandwert als Boden (`engine.rs:273-279`), `SETTLE_EPSILON` 0,002. Die
  Reihenfolge ist nicht vertauschbar.
- `bass_aura` war über alle 517 gemessenen Bilder **exakt 0** — die Aura-Schicht
  fehlt dem Port zu Recht.
- Chunk 1024 bei 44,1 kHz ⇒ **43,066 Bilder/s**. Die Bildrate ist damit
  vorgegeben, nicht wählbar.
- `render.rs` fährt einen Bloom-Unterstrich nur bei `glow > 0 && width > 0`;
  `bars.rs` setzt beides auf 0, der Durchgang feuert dort **nie**. War eine
  Sackgasse, muss nicht noch einmal geprüft werden.
- `capped_scene_size` deckelt Inline-Szenen auf 640 × 360 und Vollbild auf
  512 × 288. Der Extraktor umgeht das, weil er `scene(w, h)` direkt ruft.

**Zum Bauen und Messen:**

- Der Ofen steht schon im Repo: `render_bass_pressure_moments_ppm` in
  `crates/reprise-gnome/src/ui/now_playing/song_visualizer_tests.rs` liest
  rohes f32-PCM aus `REPRISE_VIS_PCM` und fährt die volle Kette. Der Extraktor
  ist eine Variante davon, kein Neubau.
- Der Visualizer-Code ist zwischen lokalem HEAD (`be5f014d3b`) und `origin/dev`
  **identisch** — leerer Diff über `visuals/`, `cava.rs`, `now_playing/`. Für
  reine Messungen kann also lokal mit dem warmen `target/` gebaut werden
  (Testlauf ~11 s). **Für die Umsetzung gilt das nicht**: `showroom/` fehlt im
  lokalen Checkout ganz, der Branch wird von `origin/dev` geschnitten.
- `origin/dev` steht inzwischen auf **`bf546d6cc8`**. Der Plan-Kopf nennt noch
  `aa1c1a00af` als Erhebungsbasis — Zeilennummern gegebenenfalls nachziehen.
- Der Lastregler-Hook (`~/.claude/hooks/heavy-run-gate.sh`) blockt auf der
  Zeichenkette `check-merge-readiness` **im Kommandotext** — auch wenn sie nur
  als Prosa in einem Heredoc vorkommt. `HEAVY_RUN_DISABLE=1` als Präfix hilft
  nicht, weil der Hook vor der Ausführung liest. Lange Texte deshalb mit dem
  Write-Werkzeug in eine Datei schreiben und mit einem kurzen Aufruf einbauen.
- `cp` ist interaktiv aliast und hängt Skripte auf; `command cp -f` nehmen.

**Zum Showroom:**

- Das Auflagerechteck ist **ausgemessen**, nicht mehr geschätzt: x 208–871,
  y 597–1249 von 1080 × 2404 ⇒ `left 19.259% / top 24.834% / width 61.389% /
  height 27.121%`.
- Die Platte wird klein dargestellt: `.hero-product__phone` ist
  `clamp(10.5rem, 18vw, 17rem)` = 168–272 px breit; das Quadrat macht ~61 %
  davon aus, also 102–166 CSS-px.
- Medienbudget heute: 874 KB über elf Bilder. Die Bandspur wäre ~17 KB.
- `showroom/` auf `origin/dev`: 56 Dateien, Vite + React mit SSR-Prerender,
  fünf Tests in `showroom/tests/` (`node --test`).
- `.github/workflows/pages.yml` hat nur Node 26.7.0, Trigger
  `paths: ['showroom/**']`, Deploy nur auf `main`; der Workflow merkt selbst an,
  dass die Pages-Quelle evtl. noch nicht auf „GitHub Actions" umgestellt ist.
- Kein Merge-Gate fasst `showroom/` an (21 Stufen, keine davon).
- `check-ux-traceability.sh` kennt nur `crates/` und `scripts/cua-e2e`,
  Level-Regex nur `core|gtk|e2e|manual`. Tests in `showroom/tests/` können es
  nicht grün machen.
- `check-architecture.sh` sieht neue Crates nicht — jede Schranke ist eine
  handgeschriebene `cargo tree -p <name>`-Sonde.
- Pipeline-Hilfsskripte gibt es in diesem Repo nicht — kein `status.sh`, kein
  `worktree.sh`, kein `codex-run.sh`, weder lokal noch auf `origin/dev`.

## Was aufbewahrt ist

`~/.cache/reprise-visualizer-measure/` (7,8 MB, überlebt die Session):

- `hellfire.f32` — die gemessenen zwölf Sekunden als rohes Mono-f32 44,1 kHz
  (aus *…And I Return to Nothingness (2021)/01 To the Hellfire.flac*, ab 96,0 s).
- `bands.csv`, `pressure.csv` — 517 Zeilen à 64 Bänder bzw. `kick,impact,aura`.
- `bands.u8` (33 088 B), `kick.u8` (517 B) — quantisiert, direkt verwendbar.
- `engine.webm` — der gemessene Cairo-Pfad als Video, 918 KB.
- `reference/` — **die verifizierte Referenzimplementierung**:
  `parity4.tmpl.html` trägt die korrigierte Zeichenfunktion, gegen die
  MAE 3,2 gemessen wurde; `port-trifft-engine.html` ist die laufende
  Gegenüberstellung; `engine-f*.png` / `port4-f*.png` / `parity-strip.png` sind
  die Belege; dazu eine Kopie des Plans.

Die Rohbilder (670 MB) sind gelöscht, in ~15 s neu erzeugbar.

**Achtung:** der Plan liegt weiterhin **ungetrackt** im geteilten Hauptcheckout
und kann dort verschwinden. Die Kopie in `reference/` ist die Sicherung.

## Nächste Schritte

1. **Code-Phase** zu diesem Plan: Branch von `origin/dev`, die acht Aufgaben
   aus §5. Aufgabe 3 (die drei TS-Module) sollte sich an
   `reference/parity4.tmpl.html` orientieren — dort steht der Code, der die
   Messung bestanden hat.
2. **Eigener Plan für den Gesamt-Import der Design-Seite.** Der Auftraggeber
   hat „ganze Seite übernehmen" gewählt. Umfang: der 1569-Zeilen-Monolith
   zurück in die React-Komponenten, mit Scroll-Choreografie, Mosaik samt
   Lightbox, einer Spectral-Seek-Sektion aus `waveform.rs`/`spectral_colour.rs`,
   einem neuen CH.04 für CLI und MCP, einem Tempo-Band und CH.05 als Ledger.
   Die Datei `github.md` im Design-Projekt führt eine Sektions-zu-Quelle-Tabelle
   und ein Sync-Protokoll — das ist der Einstieg, nicht der Monolith selbst.
   Der Visualizer aus diesem Plan gehört dort **nicht** noch einmal hinein.

## Hygiene

- Kein Wake-Lock offen (`showroom-visualizer` ist freigegeben).
- Keine Hintergrundläufe offen.
- Hauptcheckout sauber; die Extraktor-Funktion war zweimal kurz an
  `song_visualizer_tests.rs` angehängt und ist beide Male zurückgenommen.
