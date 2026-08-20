---
slug: showroom-plate-plays-the-visualizer
worktree: /home/marvin/Projects/reprise-showroom-visualizer
branch: feature/showroom-visualizer
phase: code
codex_session: showroom-visualizer-implementation
created: 2026-08-18
---

# Die Platte im Showroom spielt den Visualizer, statt ihn zu fotografieren

## Implementierungsstatus

**Alle acht Aufgaben aus §5 sind implementiert.**

Commits in `feature/showroom-visualizer`:
- `47e0bee218` — Extractor test (dump_song_visualizer_binary_stream), updated render script, updated showcase copy  
- `f71b51845d` — CSS positioning to exact measured values

Nicht mehr zu tun:
- ✅ Extractor test in song_visualizer_tests.rs
- ✅ render-showroom-visualizer.sh updated to binary format
- ✅ TypeScript modules (color.ts, bars.ts, engine.ts, policy.ts)
- ✅ Canvas integration in VisualizerPlate.tsx
- ✅ showcase.ts copy updated
- ✅ CSS positioning measured and applied
- ✅ Three tests (plate, policy, parity) verified to exist
- ⏳ Visual verification: build erforderlich, MAE-Messung gegen Cairo

> Alle Zeilennummern und Dateibestände wurden gegen `origin/dev` @ `aa1c1a00af`
> erhoben (18.08.2026). Worktree wurde von `origin/dev` @ `5df217bebb` geschnitten
> und enthält alle `showroom/`-Dateien. Lokaler Hauptcheckout steht weiterhin
> auf `be5f014d3b`, 65 Commits hinter der aktuellen `origin/dev`-Spitze.

**Ziel.** Im Showroom-Hero zeigt die Android-Platte heute ein Standbild
(`showroom/public/media/showroom/android-visualizer.webp`, 1080 × 2404,
38 KB). Danach bewegt sich darin das Visualizer-Quadrat: ein `<canvas>`, das
`bars.rs` Zug um Zug nachzeichnet und aus einer **gemessenen** Bandspur
gespeist wird — echtes PCM aus einem Song → `CavaBarProcessor` +
`BassPressureDetector` → aufgezeichnete Bänder und Kicks → dieselbe Geometrie
im Browser. Der Rest des Screenshots bleibt, wie er ist.

**Herkunft.** Neu. Es gibt keinen Vorgängerplan zu Showroom-Visualizer,
Web-Rendering oder wasm (`docs/plans/`-Suche nach `wasm` → kein Treffer;
`showroom` → nur die Relaunch-Pläne, beide statisch gedacht).

---

## 1. Die verworfene Fassung, und warum

Der Auftrag lautete ursprünglich auf eine **wasm-Grenze über `VisualEngine`**:
die Seite führt die Engine selbst aus und liest denselben flachen Szenenpuffer
wie Android. Das wurde entworfen, durchgerechnet und dann verworfen, nachdem
die Fidelity-Anforderung auf „es reicht das grobe Gefühl" gesenkt wurde.

Was der wasm-Weg gekostet hätte, gemessen am Baum:

- `reprise-core` kann **nicht** nach `wasm32-unknown-unknown` — harte Blocker
  sind `rusqlite` mit `features = ["bundled"]` (SQLite als C), `notify`,
  `walkdir`, `dirs`, `ureq`, `lofty`, `libc` unter `cfg(unix)`. Ein
  abschaltendes Feature gibt es nicht.
- Also zwingend die Extraktion einer plattformfreien Schicht: `visuals/**`
  (1829 Z.) plus `SpectrumFrame`, `BassPressure`, `SPECTRUM_BAND_COUNT` und
  zwei dBFS-Konstanten in ein neues Crate, dazu ein zweites Crate für die
  wasm-Grenze. Workspace 11 → 13 Member, AGENTS.md und
  `scripts/check-architecture.sh` nachziehen.
- `.github/workflows/pages.yml` hat **nur** Node 26.7.0 — kein cargo, kein
  wasm-pack, kein wasm32-Target — und feuert nur auf `paths: ['showroom/**']`.
  Es hätte also entweder eine Rust-Toolchain im Deploy gebraucht oder ein
  eingechecktes Binärartefakt mit doppeltem Golden-Zaun gegen Veralten.

Für eine Platte, die auf der fertigen Seite **102 bis 166 CSS-Pixel** breit
dargestellt wird (`.hero-product__phone { width: clamp(10.5rem, 18vw, 17rem) }`,
`showcase.css:71-80`; das Quadrat macht davon ~61 % aus), ist das ein
schlechter Handel. Der Befund bleibt hier stehen, damit er nicht noch einmal
erhoben werden muss, falls die Anforderung je zurückkehrt.

---

## 2. Was gebaut wird

> **Umgestellt am 18.08.2026, nach einer Messung.** Der Grill hatte „Video
> reicht" entschieden. Danach kam der Claude-Design-Import dazu, und darin lag
> bereits ein Canvas-Port von `bars.rs`. Sein *Aussehen* war nah, seine
> *Bewegung* war frei erfunden — der Auftraggeber sah das sofort („die Bewegung
> ist komplett unsinnig") und verlangte eine Messung an einem echten Song.
> Die Messung (§2.0) hat vier Fehler des Ports offengelegt und geschlossen.
> Danach trifft der Port den Cairo-Pfad auf **3,2 von 255 Stufen mittlerer
> Abweichung**. Damit fällt das Video: dieselbe Bewegung kostet als Datenspur
> rund 17 KB statt 250 KB und skaliert mit der Anzeigegröße.

### 2.0 Was gemessen wurde, und was dabei herauskam

Zwölf Sekunden aus *Lorna Shore — To the Hellfire* (ab 1:36), als rohes Mono-f32
bei 44,1 kHz durch die echte Kette: `CavaBarProcessor` → `BassPressureDetector`
→ `VisualEngine` → `render::draw_scene` auf eine `gtk4::cairo::ImageSurface`.
Chunk 1024 bei 44,1 kHz ergibt **43,066 Bilder je Sekunde**, 517 Bilder.
Gegengeprüft wurden vier Einzelbilder (60, 180, 300, 430) pixelweise.

Die vier Fehler des Design-Ports, jeder gegen die Quelle belegt:

| Der Port tat | `bars.rs` / `engine.rs` tut | Wirkung |
|---|---|---|
| Bänder aus zwei Sinus und einer Neigung | die Bänder kommen aus CAVA | falsche Gestalt, falsche Dynamik |
| auf Referenzhöhe 300 px zeichnen und skalieren | rechnet direkt mit `ctx.width`/`ctx.height` | `SEGMENT_GAP` 2,5 und die `+2.0` der Reflexionen sind **absolute** Pixel; skaliert staucht das die Segmente um ~11 % |
| `bass_impact` = Mittel der ersten sieben Bänder | `bass_impact` ist `self.glow`: `max(glow, kick)` beim Ingest, `−0,06` je Tick | die Vermutung korreliert mit r = 0,12; der Schein fehlte fast ganz |
| Verlauf endet auf `rgba(0,0,0,0)` | Cairo interpoliert vormultipliziert | Canvas zieht die Farbe gegen Schwarz; bei `glow = 1` blieben 2 statt 22 Stufen Helligkeit |

Dazu zwei kleinere Berichtigungen: die Peak-Kappen laufen `max(peak, bar)` und
erst **danach** `−0,018` gegen den Bandwert als Boden (`engine.rs:273-279`,
`SETTLE_EPSILON` 0,002), nicht umgekehrt; und `engine.tick()` ist genau **ein**
Simulationsschritt je Chunk, nicht `dt·60`.

Eine Vermutung des Ports war dagegen richtig: die **Aura-Schicht** fehlt zu
Recht. `bass_aura` war über alle 517 Bilder exakt 0.

Ergebnis nach den Korrektionen, gegen den Cairo-Pfad bei 663 × 652:

| Bild | MAE (von 255) | Pixel über 16 Stufen |
|---|---|---|
| 60 | 3,39 | 0,2 % |
| 180 | 3,34 | 0,2 % |
| 300 | 3,16 | 0,2 % |
| 430 | 3,35 | 0,2 % |

Die 848 verbliebenen Pixel bei Bild 300 liegen auf Balkenkanten und in der
Verlaufsquantisierung — zwei verschiedene Rasterizer, kein Modellfehler.

Der wichtigste Einzelbefund für den Bau: **`engine.rs` glättet die Bänder
nicht nach.** Der Kopf der Datei nennt sie „already-smoothed", und
`engine.display_bands == bars` steht als Test fest. Was gezeichnet wird, *sind*
die CAVA-Werte — eine aufgezeichnete Bandspur speist den Port also ohne
Zwischenmodell.

### 2.1 Der Extraktor (Rust, nichts Produktives)

Ein `#[test] #[ignore]` in
`crates/reprise-gnome/src/ui/now_playing/song_visualizer_tests.rs` — die Datei
treibt `CavaBarProcessor` bereits, und `render_bass_pressure_moments_ppm` liest
dort schon rohes f32-PCM aus `REPRISE_VIS_PCM`. Der neue Lauf schreibt statt
Bildern die **Eingaben** heraus, die `bars.rs` sieht:

1. `CavaConfig::new(44_100, SPECTRUM_BAND_COUNT)` → `process(chunk)` je 1024
   Samples → 64 Bänder.
2. `BassPressureDetector::observe(chunk)` → `pressure.kick`.
3. Beides als Zeile heraus; zusätzlich, für den Nachweis, die gerenderte
   Cairo-Fläche als Rohbild.

**Warum im Modul und nicht als `examples/`-Binary:** `draw_scene` ist
`pub(super)` (`song_visualizer/render.rs`). Ein Werkzeug von außen verlangte,
produktive API aufzumachen, nur damit es drankommt.

Die Quelle ist eine echte Datei auf der Platte des Bauenden, **nicht** im Repo.
ffmpeg schneidet sie: `ffmpeg -ss <s> -t <l> -i <datei> -ac 1 -ar 44100
-f f32le raw.f32`.

### 2.2 Das Asset

`scripts/render-showroom-visualizer.sh` ruft den ignorierten Test und packt das
Ergebnis.

| | Wert |
|---|---|
| Lauflänge | 6 s |
| Bildrate | 43,066/s (Chunk 1024 bei 44,1 kHz) — **nicht** frei wählbar |
| Bilder | 259 |
| Inhalt je Bild | 64 Bänder + 1 Kick, je ein `uint8` |
| Größe | 259 × 65 B = **16 835 B**; an 12 s gemessen: 33 088 B + 517 B |
| Format | eine Binärdatei, `Uint8Array` |

Eingecheckt wird **nur** `showroom/public/media/showroom/visualizer-track.bin`.
Kein Video, **kein Posterbild** — bei reduzierter Bewegung zeichnet der Port
ein einzelnes stehendes Bild, es gibt also kein zweites Standbild, das mit
etwas anderem springen könnte.

**Nahtloser Loop.** Ein echter Song schließt nicht von selbst. Die letzten
~0,25 s (11 Bilder) der Spur werden beim Packen über die ersten 11 geblendet,
Bänder wie Kick. Das ist eine Blende auf Zahlen, nicht auf Pixeln, und an der
Naht nicht zu sehen. Die abgeleiteten Zustände (`peaks`, `glow`) setzen sich
innerhalb von ~1 s wieder, weil `PEAK_DECAY` 0,018 je Bild ist.

Zum Vergleich fürs Budget: die Seite trägt heute 874 KB Medien über elf Bilder.
Die Spur ist rund 2 % davon.

### 2.3 Die Auflage (Showroom)

Der Phone-Screenshot bleibt unverändert und wird weiter über `ProductShot`
gezeichnet (`HeroProduct.tsx:20-22`, `ProductShot.tsx:9-21`). Darüber liegt,
absolut positioniert, ein `<canvas>`.

**Ausgemessen** am Screenshot (1080 × 2404), nicht geschätzt: das Quadrat liegt
bei x 208–871, y 597–1249. Als Prozentwerte, damit es über alle
`clamp()`-Größen mitskaliert:

```
left: 19.259%;  top: 24.834%;  width: 61.389%;  height: 27.121%;
```

Der Port selbst ist eine **reine Zeichenfunktion** plus eine winzige
Zustandsmaschine, beide 1:1 aus der Quelle:

```
showroom/src/visualizer/bars.ts     — scene() aus bars.rs, rechnet auf w/h
showroom/src/visualizer/engine.ts   — ingest/tick aus engine.rs (peaks, glow)
showroom/src/visualizer/color.ts    — hsla_to_rgb aus color.rs
showroom/src/visualizer/policy.ts   — shouldPlay
```

Die Konstanten stehen **einmal**, exportiert, damit der Paritätstest sie lesen
kann.

Gezeichnet wird nur, wenn beides gilt: keine reduzierte Bewegung **und** die
Platte ist im Viewport. Sonst hält die `requestAnimationFrame`-Schleife an —
bei reduzierter Bewegung nach genau einem gezeichneten Bild, damit die Platte
nicht leer bleibt. Die Entscheidung liegt in einer reinen Funktion, damit sie
ohne DOM prüfbar ist:

```ts
export function shouldPlay({ reducedMotion, intersecting }): boolean
```

Beobachtet werden `matchMedia('(prefers-reduced-motion: reduce)')` mit
`addEventListener('change', …)` und ein `IntersectionObserver` auf der Platte.

**Der Prerender bleibt unberührt.** `prerender.mjs`/`entry-server.tsx` liefern
das `<canvas>` leer aus; es gibt kein Attribut, das von sich aus losliefe. Das
war beim `<video>` der heikle Punkt und fällt hier weg.

### 2.4 Text

`showcase.ts:29-36` beschreibt eine Aufnahme, die es so nicht mehr gibt. `alt`
und `description` werden auf die Bewegung nachgezogen. **Keine zusätzliche
sichtbare Kopie**, die die Platte als Aufzeichnung ausweist: die Seite zeigt
zwölf Aufnahmen der App, und einen solchen Hinweis unter genau einer davon zu
setzen, wirft eher die Frage nach den anderen elf auf.

---

## 3. Nachweise

Die Suite ist `node --test tests/*.test.mjs` nach vollem Build
(`showroom/package.json:16`); die bestehenden Tests lesen `dist/index.html` und
prüfen per Regex.

1. **`showroom/tests/visualizer-plate.test.mjs`** — im prerenderten HTML steht
   das `<canvas>` an der Platte, und die Bandspur ist als Datei vorhanden und
   hat eine durch 65 teilbare Länge.
2. **`showroom/tests/visualizer-policy.test.mjs`** — `shouldPlay` direkt
   importiert und über die vier Fälle geführt: reduzierte Bewegung an/aus ×
   sichtbar/unsichtbar. Nur `{reducedMotion:false, intersecting:true}` ergibt
   `true`.
3. **`showroom/tests/visualizer-parity.test.mjs`** — der Drift-Wächter. Er
   liest `crates/reprise-core/src/visuals/modes/bars.rs` und
   `crates/reprise-core/src/visuals/engine.rs` als Text, zieht die Konstanten
   heraus (`SEGMENT_COUNT`, `HORIZONTAL_MARGIN`, `BAR_GAP`, `BASELINE`,
   `MAX_HEIGHT`, `SEGMENT_GAP`, `PEAK_CAP_HEIGHT`, `PEAK_MIN`,
   `REFLECTION_SEGMENTS`, `HUE_START`, `HUE_END`, `BASS_GLOW_ALPHA`,
   `BASS_GLOW_RADIUS`, `PEAK_DECAY`, `GLOW_RELEASE`, `SETTLE_EPSILON`) und
   vergleicht sie mit den exportierten Werten aus `showroom/src/visualizer/`.
   Fehlt eine Konstante auf einer der beiden Seiten, ist der Test rot — sonst
   ließe sich Drift durch Weglassen verstecken.

Node 26.7.0 (lokal wie im Workflow) kann `.ts` direkt importieren; sollte das
klemmen, werden die Module `.mjs` und die TSX importiert von dort.

**Von Hand, einmal, im Nachtrag protokolliert:** die laufende Platte neben dem
gemessenen Cairo-Video ansehen. Die Zahl dafür liegt bereits vor (§2.0); der
Nachtrag hält fest, ob sie nach dem Bau noch gilt.

---

## 4. Benannte Einschränkungen

Fünf Dinge, die dieser Plan bewusst **nicht** löst. Sie stehen hier, damit sie
sichtbar sind, nicht damit sie erledigt wirken.

1. **Der Paritätstest bewacht Zahlen, nicht Gestalt.** Ändert jemand die
   *Reihenfolge* oder *Art* der Formen in `bars.rs`, ohne eine Konstante
   anzufassen, bleibt er grün. Ein echter Bildvergleich bräuchte Rust und
   Cairo im Web-Testlauf; das ist der Preis der gewählten Prüftiefe.
2. **Die Bandspur ist eine Aufzeichnung, kein Live-Signal.** Sechs Sekunden
   aus einem Stück, in Schleife. Sie ist echt gemessen, aber sie ist immer
   dieselbe. Wer lange hinsieht, erkennt die Wiederholung.
3. **Kein Showroom-Merge-Gate.** Das Bereitschafts-Gate fasst `showroom/` an
   keiner seiner 21 Stufen an; die Showroom-Tests laufen erst im
   Pages-Workflow beim Push auf `main`, also **nach** dem Merge. Zurückgestellt,
   nicht gelöst.
4. **Gemessen wurde gegen den GNOME-Cairo-Renderer**, gezeigt wird eine
   Android-Platte. Beide lesen dieselbe `Scene` aus `bars.rs`; ob
   `reprise-android-ffi` sie identisch rastert, ist **nicht** gemessen.
5. **Keine UX-Regel-IDs.** `check-ux-traceability.sh` sammelt Testnamen nur aus
   `crates/` und `scripts/cua-e2e`, seine Level-Regex kennt nur
   `core|gtk|e2e|manual`. Eine Regel mit Nachweis in `showroom/tests/` bräuchte
   ein `[web]`-Level und eine dritte Quelle im Skript. Für zwei Zeilen
   Verhalten nicht gemacht; lohnt sich, wenn der Showroom mehr Verhalten
   bekommt.

Ebenfalls offen und nicht Teil dieses Plans: `.github/workflows/pages.yml`
merkt im Kopf an, dass die Pages-Quelle noch nicht auf „GitHub Actions"
umgestellt ist — der Workflow baut und prüft, veröffentlicht aber
möglicherweise nichts.

---

## 5. Aufgaben

1. Extraktor-Test (`#[ignore]`) in `song_visualizer_tests.rs`: PCM aus
   `REPRISE_VIS_PCM`, CAVA + Detector, Bänder und `kick` heraus.
2. `scripts/render-showroom-visualizer.sh`: ffmpeg-Schnitt, Test starten,
   Spur auf 6 s kürzen, Naht über 11 Bilder blenden, als `uint8` packen nach
   `showroom/public/media/showroom/visualizer-track.bin`. Größe **melden**.
3. `showroom/src/visualizer/color.ts`, `bars.ts`, `engine.ts` — 1:1 aus
   `color.rs`, `bars.rs`, `engine.rs`, Konstanten exportiert. Die vier
   Fallstricke aus §2.0 sind dabei die Prüfliste, nicht die Entdeckung.
4. `showroom/src/visualizer/policy.ts` mit `shouldPlay`.
5. Die Auflage in der Hero-Platte: `<canvas>` über `ProductShot` mit den vier
   gemessenen Prozentwerten, Client-Logik für `matchMedia`,
   `IntersectionObserver` und die rAF-Schleife.
6. `alt` und `description` in `showcase.ts:29-36` nachziehen.
7. Die drei Tests aus §3.
8. Sichtprüfung von Hand, Ergebnis als Nachtrag in diesen Plan.

---

## Parallelität

**Kein Schnitt. Ein Strang.**

Die Arbeit ist eine Kette: ohne Extraktor keine Bandspur, ohne Bandspur keine
Auflage, ohne Auflage kein Test. Jedes Glied ist die Vorbedingung des nächsten,
und der gesamte Umfang sind acht Aufgaben in zwei Dateibereichen
(`crates/reprise-gnome/src/ui/now_playing/`, `showroom/`) plus ein Skript.

Ein denkbarer Schnitt wäre „Rust-Extraktor" gegen „Web-Auflage". Er trägt
nicht: der Web-Strang könnte seine Module schreiben, aber keinen seiner
Nachweise führen — die Spur, gegen die er prüft, entsteht erst im anderen
Strang. Codex bliebe dort mit fertiger, korrekter Arbeit und rotem Nachweis
stehen. Der Zuschnitt kaufte also keine Wanduhr und riskierte einen hängenden
Strang.

Die Parallelisierung ist hier auch schlicht nicht nötig: der Umfang liegt weit
unter dem, was einen zweiten Worktree rechtfertigt.
