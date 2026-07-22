# ML-Runtime für Stem-Separation — Spike-Report (Paket E)

Stand: 2026-07-22

## Zweck und Auftrag

Dieser Report entscheidet **faktenbasiert** die letzte offene Frage des
Multi-Frontend-Core-Plans (`docs/plans/multi-frontend-core.md`, Beschluss 11):
Welche ML-Runtime setzt Paket G für die Vocal-Removal-/Instrumental-Pipeline
(`crates/reprise-stems`) um? Gemessen wurden auf dem Zielrechner im
Release-Profil zwei Kandidaten:

- **(a) candle** (pure-Rust, HF-Ökosystem) mit einem Demucs-Klasse-Modell.
- **(b) ort** (ONNX-Runtime-Bindings) mit einem MDX-Klasse-ONNX-Modell.

`libtorch` und Python-Subprozess sind laut Beschluss 11 verworfen und wurden
nicht vermessen. Deliverable ist **dieser Report mit Empfehlung**, kein
Produktionscode — Paket G implementiert die Empfehlung. Der Spike-Code liegt
in `crates/reprise-stems/examples/` hinter den optionalen Features
`spike-candle`/`spike-ort` (siehe [Anhang: Reproduktion](#anhang-reproduktion)).

**Qualität ist ausdrücklich nicht Gegenstand dieses Spikes** (Beschluss:
Demucs-Klasse-Qualität ist die Einschlussbedingung, aber die
Trennqualität wird hier nicht bewertet). Das Testsignal ist synthetisch und
dient ausschließlich einem **faithful Timing**.

## Kernergebnis (TL;DR)

- **candle scheitert an der Modellverfügbarkeit, nicht an der Runtime.**
  candle-transformers enthält **kein** Demucs; die einzige Rust-Demucs-Portierung
  (`demucs-rs`) setzt auf **Burn**, nicht candle. Ein echter htdemucs-Lauf unter
  candle erfordert einen **Hand-Port der Hybrid-Transformer-Architektur plus
  Gewichtskonvertierung** — außerhalb des Spike-Timebox und ein erheblicher
  Paket-G-Aufwand. Die Runtime selbst ist pure-Rust und schlank (verifiziert).
- **ort läuft sofort** — ONNX-Modelle laden ohne Architektur-Port. Sowohl ein
  MDX-Klasse-Modell als auch **htdemucs** (die geforderte Demucs-Klasse) laufen
  auf diesem Rechner **schneller als Echtzeit** (~2,5–3,5×).
- **Der Lizenz-Gate kippt die Modellwahl, nicht die Runtime:** Die
  UVR-MDX-Net-Community-Gewichte haben **keine belastbare Lizenz** (Gate-Fail),
  **htdemucs ist sauber MIT** (Meta; Gate-Pass).
- **Empfehlung: ort-Runtime + htdemucs-Gewichte (MIT) als ONNX.** Preis, den
  Paket G zahlt: die native-onnxruntime-Flatpak-Offline-Story und ein hoher
  Speicher-Peak (~6 GB fp32).

## Methode

### Zielrechner

Ausgelesen aus `/proc/cpuinfo` und `free -h`:

- **CPU:** Intel Core Ultra 7 258V (Lunar Lake), 8 Kerne / 8 Threads (kein SMT;
  4 P-Kerne Lion Cove + 4 LP-E-Kerne Skymont — **heterogen**, relevant für die
  Timing-Varianz unten).
- **SIMD:** AVX, AVX2, FMA, F16C — **kein AVX-512** (Lunar Lake).
- **RAM:** 30 GiB gesamt, ~19 GiB verfügbar zum Messzeitpunkt.
- **OS:** Linux 6.18.38-1-MANJARO x86_64.
- **Toolchain:** rustc/cargo 1.96.1; **Release-Profil** für alle Messungen.

### Testsignal

Synthetisch erzeugt (`synth_stereo`): Stereo, 44,1 kHz, f32, Summe aus
Sinus-Partialtönen (220/440/660/1760 Hz) plus schwaches Xorshift-Rauschen.
Längen: 45 s / 90 s / 120 s, um den langsamen ersten Chunk (ONNX-Runtime-Warmup)
zu amortisieren. Für Timing völlig ausreichend, da die Inferenzkosten allein von
der Tensor-Form und dem Modellgraphen bestimmt werden, nicht vom Signalinhalt.

### Modelle (Herkunft, Größe, Checksumme)

| Modell | Datei | Größe | sha256 | Quelle |
|---|---|---|---|---|
| MDX-Klasse | `UVR-MDX-NET-Inst_HQ_1.onnx` | 66.759.214 B (63,7 MiB) | `38a045c4…e29f7f9` | HF-Mirror `Blane187/all_public_uvr_models` |
| Demucs-Klasse | `htdemucs.onnx` (fp32) | 316.446.953 B (301,8 MiB) | `68d0bf16…fcc5e74` | HF `StemSplitio/htdemucs-onnx` |

Modellgraphen (per `ort_probe` ausgelesen, also real, nicht angenommen):

- **MDX:** Input `input` `[batch, 4, 3072, 256]`, Output `output` gleich.
  Spektrogramm-Domäne: 4 Kanäle = (L,R)×(Re,Im), dim_f=3072, dim_t=256.
  STFT extern (im Harness, `rustfft`): n_fft=6144, hop=1024 (MDX-Standard) ⇒
  ein Chunk = 256·1024 = 262.144 Samples = **5,944 s Audio**.
- **htdemucs:** Input `mix` `[1, 2, 343980]` (Stereo-**Waveform**, 7,8 s
  Segment), Output `stems` `[1, 4, 2, 343980]` (4 Quellen × Stereo). Die STFT
  liegt **im Graphen** — reine Waveform-I/O.

### Exakte Kommandos

```bash
# Kandidat (b): ONNX-Modellgraph inspizieren
cargo run -p reprise-stems --release --features spike-ort --example ort_probe -- <model.onnx>
# Kandidat (b): MDX-Timing (Sekunden optional, Default 45)
cargo run -p reprise-stems --release --features spike-ort --example ort_mdx_bench -- UVR-MDX-NET-Inst_HQ_1.onnx 120
# Kandidat (b): htdemucs-Timing (Demucs-Klasse)
cargo run -p reprise-stems --release --features spike-ort --example ort_demucs_bench -- htdemucs.onnx 90
# Kandidat (a): candle pure-Rust-Nachweis + CPU-Mikrobenchmark
cargo run -p reprise-stems --release --features spike-candle --example candle_probe
```

Gemessen wird: Kaltstart (Session-Build = Modell laden + Graph-Optimierung),
erste-Chunk-Latenz (inkl. ONNX-Warmup), reiner Inferenz-Echtzeitfaktor (RTF =
Wandzeit / Audiodauer), Peak RSS (`VmHWM` aus `/proc/self/status`),
Thread-Anzahl (onnxruntime intra-op default = 8).

## Messtabelle

| Metrik | (a) candle + Demucs | (b) ort + MDX-Net HQ | (b) ort + **htdemucs** (Demucs-Klasse) |
|---|---|---|---|
| Im Timebox lauffähig | **Nein** — Port-Blocker | Ja | Ja |
| Modelldatei | — | 63,7 MiB | 316 MiB (fp32) / 166 MiB (fp16) |
| Kaltstart (Session-Build) | — | **~0,14 s** (128–183 ms) | **~3,2 s** (3,16–3,93 s) |
| Erste Ausgabe (inkl. Warmup) | — | ~2,3–2,6 s | ~5,1–7,5 s |
| **Echtzeitfaktor (RTF)** | **nicht messbar** | **0,37–0,42×** (~2,5× RT) | **0,28–0,46×** (~2,5–3,5× RT)¹ |
| 4-Min-Song (240 s) → Rechenzeit | — | ~94 s | ~70–110 s¹ |
| Peak RSS | (Mikrobench: 144 MiB) | **~2,6–2,8 GB** | **~5,0–6,2 GB** (fp32) |
| Threads (intra-op) | 1 (Mikrobench) | 8 | 8 |
| Binary-Größenimpact | **+~2 MB** (pure Rust) | +~22 MB (onnxruntime statisch) | +~22 MB |
| Pure Rust | **Ja** (verifiziert) | Nein (onnxruntime 1.22.0) | Nein |

¹ **Wichtiger Vorbehalt:** Der htdemucs-Harness verarbeitet Segmente **ohne
Overlap** (Back-to-Back). Produktions-Demucs nutzt typischerweise Overlap 0,25
(und optional `shifts`/TTA), was die reale Rechenzeit um ~1,3× (bzw. bei
`shifts` deutlich mehr) erhöht. Die gemessenen RTF sind also eine **untere
Schranke**; realistisch mit Overlap ~0,4–0,45×. Die Varianz (0,28–0,46) stammt
messbar von der **heterogenen P+E-Kern-Topologie**: onnxruntime verteilt 8
Threads über ungleiche Kerne, was bei kurzen Läufen schwankt. Median-typisch
liegt htdemucs bei **~0,30–0,35× ohne Overlap**.

candle-Mikrobenchmark (nur Kontext, **kein** Demucs-RTF): matmul 512×384×384 =
25 GFLOP/s, Conv-Encoder-Stack 328 ms/Pass, Peak RSS 144 MiB. Das belegt: candle
läuft pure-Rust auf diesem Rechner; der Out-of-the-box-CPU-Durchsatz ist moderat
(ohne MKL/BLAS-Feature), was eine htdemucs-Portierung nicht per se disqualifiziert,
aber auch keinen Vorsprung verspricht.

## Build- und Packaging-Story

### (a) candle

- **Pure-Rust bestätigt:** `ldd` auf das candle-Beispielbinary zeigt **nur**
  `libc`/`libm`/`libgcc_s` — **keine** native ML-Bibliothek, kein BLAS, kein
  onnxruntime. Der Standard-CPU-Backend (`gemm`-Crate) ist reiner Rust-Code.
- **Binärgröße:** candle-Beispiel 2,5 MB; der Aufschlag durch candle ggü. einem
  Leer-Binary liegt bei ~2 MB. **Flatpak-Offline-Build trivial** — reine
  crates.io-Abhängigkeiten, `cargo --offline` mit vendorten Crates genügt, keine
  Build-Zeit-Downloads, kein native-Lib-Management pro Plattform.
- **Lizenz Runtime:** candle-core/candle-nn 0.11.0 = **MIT OR Apache-2.0** ✓.
- **Der Blocker liegt am Modell, nicht am Build** (siehe Risiken).

### (b) ort

- **Auflösung/Linking (empirisch aus dem `ort-sys`-Buildlog):**
  `onnxruntime not found using pkg-config, falling back to manual setup` ⇒ die
  Default-Strategie **`download-binaries`** lädt zur **Build-Zeit** eine
  vorkompilierte statische `libonnxruntime.a` (97,8 MB, onnxruntime **1.22.0**)
  von pykes CDN nach `~/.cache/ort.pyke.io/…` und linkt sie **statisch**
  (`cargo:rustc-link-lib=static=onnxruntime`, plus `stdc++`). Ergebnis: ein
  selbstständiges 26,7-MB-Binary ohne Runtime-`.so`-Abhängigkeit (`ldd` zeigt nur
  `libstdc++`/`libgcc_s`).
- **Flatpak-Offline-Build — der zentrale Preis:** `download-binaries` **bricht**
  einen Offline-/Flathub-Build (Netz zur Build-Zeit verboten). Tragfähige Wege:
  1. **`ORT_STRATEGY=system`** + die vorkompilierte `libonnxruntime.a`/`.so` als
     **deklarierte, per sha256 geprüfte Flatpak-Source** (flatpak-builder lädt
     Sources vorab mit Checksumme — genau das Muster, das der Plan für die
     Modell-Gewichte ohnehin vorsieht). `ORT_LIB_LOCATION` zeigt auf die Datei.
  2. **`load-dynamic`** + eine gebündelte `libonnxruntime.so`, die zur Laufzeit
     via `dlopen` geladen wird (entkoppelt Build und Lib vollständig).
  3. onnxruntime als eigenes Flatpak-Modul aus Source bauen (langsam, unnötig).
  Empfehlung für G: **Weg 1 oder 2** — beide sind Standard und lösbar, aber
  **echte Packaging-Arbeit**, die candle nicht hätte.
- **Portabilität:** onnxruntime hat offizielle Builds für Windows/macOS/
  Android/iOS — die Portabilitätszusage des Plans bleibt gewahrt, jedoch mit
  **nativer Lib pro Plattform** statt eines reinen Rust-Artefakts.
- **Lizenz Runtime:** ort/ort-sys 2.0.0-rc.10 = **MIT OR Apache-2.0** ✓;
  ONNX Runtime selbst = **MIT** (Microsoft) ✓. Hinweis: ort 2.0 ist noch
  **RC** (kein stabiles Release) — Paket G sollte auf die dann aktuelle
  Version re-pinnen.

### Workspace-Hygiene (beide)

Die Spike-Deps sind **optional** und die Beispiele tragen `required-features`;
`cargo check/test/clippy --all-targets` (ohne Features) überspringt sie. Belegt:
`cargo tree -p reprise-stems` (Default) ist **core-only**; `cargo check
--workspace`, `cargo test -p reprise-stems` und `cargo clippy --all-targets -p
reprise-stems -- -D warnings` sind grün; **`cargo audit` bringt keinen neuen
Advisory** (weiterhin nur der akzeptierte RUSTSEC-2024-0436 via `paste`/lofty).
494 Lock-Pakete insgesamt.

## Lizenz-Befunde gegen das LICENSING-Gate

Das Gate (`LICENSING.md`, Absatz „Audio-analysis and stem-separation…“) verlangt
für den MIT-Engine-Pfad: **Redistribution + kommerzielle Nutzung + Linking aus
GPL-Linux-Client und künftigen proprietären Frontends**; AGPL sowie
Non-Commercial-/No-Derivatives-Terms sind ausgeschlossen; jedes Modell braucht
**dokumentierte Lizenz und Provenienz** vor Einzug ins Repo.

| Artefakt | Lizenz | Quelle/Nachweis | Gate |
|---|---|---|---|
| candle-core/nn 0.11 | MIT OR Apache-2.0 | crates.io | **✓** |
| ort / ort-sys 2.0.0-rc.10 | MIT OR Apache-2.0 | crates.io | **✓** |
| ONNX Runtime 1.22.0 (nativ) | MIT | microsoft/onnxruntime (GitHub-API) | **✓** |
| **Demucs / htdemucs-Gewichte** | **MIT** (Meta Platforms) | `adefossez/demucs` LICENSE (MIT, „Copyright (c) Meta Platforms“); Gewichte via `demucs/remote/files.txt` von `dl.fbaipublicfiles.com` als Teil des MIT-Projekts | **✓** |
| htdemucs **als ONNX** | **MIT** | `StemSplitio/htdemucs-onnx`: „This repo is MIT-licensed, matching the original HT-Demucs.“; ONNX-Export von Metas offiziellen Gewichten | **✓** |
| **UVR-MDX-Net-Community-Modelle** | **unklar / nicht etabliert** | siehe unten | **✗ (Gate-Fail)** |

### Warum die UVR-MDX-Net-Gewichte durchfallen

- Das UVR-**Anwendungs**-Repo (`Anjok07/ultimatevocalremovergui`) meldet in den
  GitHub-Metadaten zwar MIT, hat aber **keine LICENSE-Datei** im Root
  (Default-Branch `master`; `raw …/LICENSE` = 404), und der GitHub-`/license`-
  Endpunkt liefert **`None`** (keine erkannte Lizenzdatei). Issue #2185 (2026,
  **unbeantwortet**) fragt genau die kommerzielle Nutzung der **Modelle** an und
  hält fest, dass keine explizite LICENSE existiert.
- **Entscheidender Punkt:** Selbst ein MIT auf dem *Anwendungscode* lizenziert
  **nicht** die **separat gehosteten Modell-Gewichte**. Die MDX-Net-Modelle sind
  eigene Artefakte (nicht im Repo, von verschiedenen Community-Trainern, auf
  Drittanbieter-HF-Mirrors), **ohne** beigelegte Lizenz. Auch
  `python-audio-separator` (die kanonische Modell-Registry) führt **keine**
  Pro-Modell-Lizenzfelder.
- Damit ist für die konkret getesteten Gewichte (`UVR-MDX-NET-Inst_HQ_1`)
  **keine Redistribution-/Kommerz-Lizenz etablierbar** — und „Lizenz nicht
  feststellbar“ ist laut Auftrag selbst ein Gate-relevanter Fail. Sie dürfen
  nach `LICENSING.md` **nicht** ausgeliefert werden.
- Provenienz-Randnotiz (kein Distributions-Blocker, aber ehrlich vermerkt):
  Demucs/MDX wurden auf MUSDB18(-HQ) (CC BY-NC) plus Zusatzdaten trainiert. Die
  Demucs-Autoren stellen die **resultierenden Gewichte** dennoch ausdrücklich
  unter MIT — die verbreitete (rechtlich nicht abschließend getestete) Position,
  dass Modellgewichte kein Derivat der Trainingsdaten-Lizenz sind. Für den Gate
  zählt die **ausgesprochene Distributionslizenz** = MIT.

## Risiken

1. **candle-Port-Aufwand (der Blocker):** htdemucs ist Hybrid-Transformer-Demucs
   — parallele Zeit- und Spektral-Zweige, Cross-Domain-Attention, Encoder/Decoder
   mit LSTM/Attention. Ein candle-Port bedeutet **~1000+ Zeilen Modulcode plus
   PyTorch→safetensors-Gewichtskonvertierung und numerische Verifikation**. Das
   ist ein mehrtägiger bis -wöchiger Aufwand mit Fehlerrisiko, klar außerhalb
   eines Spikes. **Schätzung: groß.**
2. **Speicher-Peak (ort/htdemucs):** ~6,2 GB fp32-Peak RSS (onnxruntime-Arena).
   Auf 8-GB-Geräten grenzwertig; **parallele Jobs sind ausgeschlossen** (der Plan
   sieht ohnehin 1 Job gleichzeitig vor — passt). Milderung: fp16-Export
   (166 MB, ~halbiert Gewichtsspeicher), onnxruntime-Arena-Konfiguration
   (`disable_cpu_mem_arena`/`memory_pattern`), kleinere Segmentlänge.
3. **ort 2.0 ist RC:** API-Drift möglich (dieser Spike traf rc.10; `session.run`
   verlangt `&mut`, `Tensor::from_array((shape, vec))`). Paket G pinnt neu und
   sichert mit einem Smoke-Test ab.
4. **Flatpak-Offline-Lib:** siehe Build-Story — lösbar (System-/load-dynamic +
   geprüfte Source), aber nicht kostenlos; muss im G-Scope eingeplant werden.
5. **Timing-Varianz auf P+E-CPU:** die heterogene Kern-Topologie streut die RTF.
   Für stabile Nutzerzeiten ggf. Thread-Affinität/Thread-Count in onnxruntime
   setzen; für die Kaufentscheidung unkritisch, da selbst der schlechteste Lauf
   klar schneller als Echtzeit ist.
6. **htdemucs-Nachbearbeitung:** v1 gibt nur die Instrumental-Spur aus
   (Beschluss 19). htdemucs liefert 4 Stems; Instrumental = Mix − Vocals (bzw.
   Summe drums+bass+other) plus die interne Overlap-/Fenster-Rekonstruktion —
   ein Implementierungsdetail für G, kein Runtime-Risiko.

## Empfehlung

> **Paket G implementiert die `ort`-Runtime (ONNX Runtime) und liefert als
> Modell htdemucs — Hybrid-Transformer-Demucs v4 — als ONNX-Export unter der
> MIT-Lizenz (Metas offizielle Gewichte, z. B. via `StemSplitio/htdemucs-onnx`).**
> Begründung auf Fakten: (1) htdemucs ist die vom Plan geforderte
> Demucs-Klasse-Qualität, und seine Gewichte sind **sauber MIT** und bestehen
> das `LICENSING.md`-Gate für Redistribution und kommerzielle Nutzung; (2) ONNX
> lädt **direkt** in ort — es gibt in candle **kein** Demucs-Modell (nur eine
> Burn-Portierung), und ein Hand-Port der Hybrid-Transformer-Architektur samt
> Gewichtskonvertierung ist ein großer, außerhalb dieses Spikes liegender
> Aufwand; (3) die gemessene Leistung auf dem Zielrechner ist mit ~0,3–0,45×
> Echtzeitfaktor komfortabel schneller als Echtzeit (ein 4-Minuten-Song in gut
> ein bis knapp zwei Minuten). Die **UVR-MDX-Net-Community-Modelle sind
> ausdrücklich nicht zu verwenden** — ihre Gewichte haben keine belastbare
> Lizenz und fallen durch das Gate. Der Preis dieser Wahl, den G einplanen muss,
> ist zweifach und beherrschbar: die native onnxruntime-Bibliothek muss für den
> Flatpak-Offline-Build als per-Checksum geprüfte System-/`load-dynamic`-Lib
> bereitgestellt werden (nicht der cargo-`download-binaries`-Default), und der
> fp32-Speicher-Peak von ~6 GB verlangt striktes Ein-Job-Serialisieren (ohnehin
> geplant) sowie die Prüfung des fp16-Exports. candle bleibt der **pure-Rust-
> Nordstern** für den Fall, dass später eine gepflegte candle-Demucs-
> Implementierung existiert — dann ist ein Wechsel wert, neu bewertet zu werden.**

### Was offen bleibt

- **fp16 vs. fp32:** fp16-htdemucs (166 MB) auf CPU messen — halbiert Download
  und Gewichtsspeicher, aber onnxruntime-CPU kann fp16 intern nach fp32 casten
  (evtl. langsamer). Für G in 1 h zu klären.
- **Overlap/Qualität:** reale Segment-Overlap-Kosten (0,25) und die
  Instrumental-Rekonstruktion (Mix − Vocals) messen; erwartet ~+30 % Rechenzeit.
- **onnxruntime-Bezug für Flatpak:** konkrete Source-URL + sha256 der
  vorkompilierten Lib festzurren (oder `load-dynamic`) und `ORT_STRATEGY` in der
  G-Buildpipeline verankern.
- **ort-Version:** beim G-Start auf die dann aktuelle (idealerweise stabile)
  ort-2.0-Version re-pinnen und den ONNX-Runtime-Versionsstand fixieren.
- **Chunking/Cancel/Progress (G-Scope):** deterministische Ausgabe über
  Chunk-Grenzen, Cancel zwischen Chunks, Progress-Callbacks (aus dem Plan).

## Anhang: Reproduktion

Spike-Code (bewusst schlank, klar als Spike markiert): `crates/reprise-stems/`
— `Cargo.toml` (optionale Features `spike-candle`/`spike-ort`, Beispiele mit
`required-features`) und `examples/{ort_probe,ort_mdx_bench,ort_demucs_bench,
candle_probe}.rs`. Modelle werden **nicht** eingecheckt; sie wurden zur Messung
von den in der Modelltabelle genannten HF-Quellen geladen. Aufgelöste
Versionen: candle 0.11.0, ort/ort-sys 2.0.0-rc.10 (onnxruntime 1.22.0),
rustfft 6.4.1, ndarray 0.16.1, hound 3.5.1.

### Umgebungsblockaden

**Keine.** Netzwerk (crates.io, huggingface.co, github.com) war während des
gesamten Spikes verfügbar; alle Crate- und Modell-Downloads sowie der
onnxruntime-Build-Zeit-Download gelangen. Es wurde keine Messung fingiert; die
einzige „nicht gemessene“ Größe — candles Demucs-RTF — ist ehrlich als
Port-Blocker dokumentiert, nicht als Umgebungsproblem.
