# Handover 2 — Porträt-Platzhalter per Fingerabdruck

Stand: 2026-08-14, 16:22 CEST. Plan: `docs/plans/portrait-placeholder-fingerprint.md`.
Vorgänger-Übergabe: `portrait-placeholder-fingerprint-handover.md` — deren Punkte
1, 2 und 3 sind **erledigt**. Offen ist nur noch ein neuer, bewusst gefasster
Beschluss und das Landen.

## Wo die Arbeit steht

- **Branch** `feature/portrait-placeholder-fingerprint` = `fbe65a00f9`,
  10 Commits, **Worktree** `/home/marvin/Projects/reprise-portrait-placeholder-fingerprint`.
- Rebased auf `origin/dev` = `8b87ae8ada`, **0 zurück**.
- **Nicht gepusht, kein PR.** Arbeitsbaum sauber bis auf das ungetrackte
  `.pipeline-findings.md`.
- Kein Codex-Lauf mehr in der Luft.

## Punkt 1 der alten Übergabe — erledigt

Der schwebende `/refactor`-Lauf ist eingesammelt und als `85225ac95d` gelandet.
Alle drei Review-Befunde sind umgesetzt und **von Hand nachgelesen**, nicht bloß
geglaubt:

- **MEDIUM** — `cache::refresh_image(…)` bekam die fehlende WARN-Zeile im
  `None`-Zweig, symmetrisch zu den beiden E5-Zeilen, plus einen Test, der den
  Fehlschlag erzwingt (Cache-Datei durch ein Verzeichnis ersetzen).
- **LOW** — Doppel-Dekodierung ist weg: `cover_download::decode_image` gibt ein
  `DecodedImage` zurück, das Fingerabdruck **und** Endungsprüfung bedient.
- **LOW** — die Fixtures hängen nicht mehr an der Lanczos-Identität. Codex hat
  es besser gelöst als gefordert: `thumbnail()` überspringt das Resampling, wenn
  das Bild schon 32×32 ist. Damit ist die Abhängigkeit **weg** statt umgangen.

Nebenwirkung, die eine Verbesserung ist: ein Bild in nicht unterstütztem Format,
das als Platzhalter erkannt wird, löscht jetzt kein vorhandenes Cache-Bild mehr.
Das ist E4 in einem Randfall, den vorher nichts abdeckte.

## Punkt 2 der alten Übergabe — erledigt, mit Kontrollarm

Die „672/672"-Behauptung des Vorlaufs hatte kein Artefakt. Selbst gefahren,
Protokoll unter `$SCRATCH/gates.log`:

| Gate | Ergebnis |
|---|---|
| `cargo fmt --all -- --check` | rc=0 |
| `cargo clippy --locked --all-targets --workspace -- -D warnings` | rc=0 |
| `cargo test -p reprise-core` | **2453 bestanden, 0 fehlgeschlagen**, 3 ignoriert |
| E6-Messung (ignoriert, echter 227-Datei-Korpus) | rc=0, 174,85 s |

Der Kontrollarm ist die vierte Zeile: die Messung schreibt in **dieselbe**
`docs/evidence/portrait-placeholder-fingerprint/rust-separation.txt`, die im
Commit liegt. `git diff` darauf blieb nach dem Neulauf **leer** — die Margen
10,200× / 23,707× sind reproduziert, nicht eingetragen.

Display-Suite bleibt aus: der Branch fasst nur `crates/reprise-core/` an.

## Punkt 3 der alten Übergabe — erledigt

Beweisverzeichnis: `acceptance/deezer-placeholder-portraits/runs/20260814T134930Z/`.
Der Lauf endet mit `acceptance evidence ready for independent visual review`.

```
prada_unchanged_by_the_fingerprint=true          34aefe17… in beiden Armen
before_oceano_differs_from_known_placeholders=true
after_oceano_has_negative_marker=true
after_oceano_has_cached_image=false
```

Im Bild: `before/my-stats.png` und `after/my-stats.png` sind Rang für Rang
identisch **bis auf Oceano** (heute Rang 13) — vorher ein Bandfoto, nachher das
Initial „O". Keine Nebenwirkung auf den anderen 19 Rängen.

Der Lauf brauchte drei Anläufe. Was daran hängen blieb, steht unten unter
„Fallen".

---

## Offen: der Beschluss „mehr Ränge rendern"

**Der Beschluss steht, die Umsetzung nicht.** Auf die Frage, ob Oceano als
Bildbeweis reicht, lautet die Antwort: die vier weiteren Silhouetten
(Aetheriality, In Your Grave, Our Vices, Wake Me) sollen **auch im Bild** belegt
werden, und zwar über eine längere Rangliste. Ich hatte davon abgeraten; die
Entscheidung ist gefallen und gilt.

Wer sie umsetzt, muss diese vier Zahlen kennen — sie sind gemessen, nicht
geschätzt:

| Künstler | Hörzeit | Rang | erreichbar ab |
|---|---|---|---|
| In Your Grave | 19,6 min | **40** | `ARTIST_ROW_EXTRA` ≥ 35 |
| Our Vices | 3,9 min | **122** | ≥ 117 |
| Wake Me | 3,3 min | **131** | ≥ 126 |
| Aetheriality | **0 Wiedergaben** | — | **durch keinen Deckel** |

Abfrage gegen `listen_events` in
`~/.local/share/reprise/reprise.db` (`immutable=1`, nur lesend); die Rangliste
ist `SUM(ms_played)` gruppiert nach `artist` — gegen den Screenshot verifiziert
(Lorna Shore 54 Wiedergaben / 4 h 56 stimmt auf die Minute).

### Was das bedeutet

- **Aetheriality ist unerreichbar.** Ein Track in der Bibliothek, null
  Wiedergaben. Die Rangliste kennt 155 Künstler mit Wiedergaben; wer nie
  gespielt wurde, steht in keiner. Die Erwartung „vier Silhouetten
  verschwinden" stammt aus dem **Bibliotheks-Sweep** über 195 Künstler, nicht
  aus der Statistik. Höchstens drei der vier sind im Bild zu zeigen.
- **Es gibt genau eine Oberfläche mit Künstlerporträts.**
  `crates/reprise-gnome/src/ui/now_playing/artist_portrait_worker.rs` sagt es im
  Kopfkommentar: „artist portraits shown in My Stats (STATS-23)". Trotz des
  Verzeichnisnamens rendert Now Playing keine. Es gibt also keine Ausweich-Ansicht.
- **Der Deckel** ist `crates/reprise-gnome/src/ui/stats/stats_bands_card.rs:20`,
  `ARTIST_ROW_EXTRA = 15`, plus fünf Kacheln oben = 20 Ränge. Er ist in
  `stats_bands_card_tests.rs:15` festgenagelt — dieser Test muss mit.
- **Der Preis** ist nicht der Deckel, sondern was daran hängt: jeder gerenderte
  Rang stößt einen Porträt-Abruf an (`MAX_IN_FLIGHT = 3`). 131 Ränge heißt ~131
  Deezer-Abrufe pro Öffnen der Ansicht statt 20. Der Prüfstand wartet zudem auf
  das Settlen **aller** gerenderten Ränge (`wait_for_rendered_portraits … 60
  "$RENDERED_TOP_ARTIST_RANKS"`), der Lauf wird entsprechend länger.

### Empfohlener Weg, falls nicht anders entschieden

Den Deckel **nicht** im Auslieferungsstand anheben, sondern über eine
Umgebungsvariable, die der Prüfstand setzt — das Repo hat dieses Muster schon
(`REPRISE_SMOKE_QUIT`, `REPRISE_AUDIO_SINK`, Feature `test-fixtures` in
`crates/reprise-gnome/Cargo.toml:18`). Dann rendert der isolierte Lauf 131 Ränge
und die App bleibt bei 20. Damit ist der Beschluss buchstäblich erfüllt, ohne das
Produkt für einen Test zu ändern.

Danach im Prüfstand: `RENDERED_TOP_ARTIST_RANKS` mitziehen, nach dem Aufklappen
zusätzlich auf die drei Namen warten und den Screenshot so schneiden oder
scrollen, dass sie drauf sind. Aetheriality im Plan als **nicht darstellbar**
festhalten statt stillschweigend fallen zu lassen.

## Offen: Landen

Nach dem Obigen hindert nichts. Wie üblich: rebasen, pushen, sofort mergen, nicht
auf CI warten (`scripts/land.sh` unter `~/.claude/skills/pipeline/scripts/`).

Zwei Stolpersteine beim Rebase, beide diesmal aufgetreten:

- `.pipeline-codex.md` ist **getrackt** und kollidiert in jedem Commit, der es
  anfasst. Vorher `git checkout --` darauf; im Rebase mit `--theirs` auflösen.
- `.pipeline-findings.md` ist ungetrackt und darf liegen bleiben — der Prüfstand
  verlangt nur einen sauberen **getrackten** Baum.

---

## Fallen, die diesen Lauf gekostet haben

- **Der Lastregler-Wrapper frisst die stderr des Kindes.**
  `heavy-run medium -- <cmd> > log 2>&1` fängt nur heavy-runs eigene Ausgabe. Ein
  Skript unter `set -euo pipefail`, das mit `echo … >&2; exit 1` abbricht,
  hinterlässt ein **0-Byte-Log** — nicht zu unterscheiden von „compiliert noch".
  Der erste Abnahme-Anlauf scheiterte so, und die Diagnose musste aus dem
  Beweisverzeichnis rekonstruiert werden. Lösung: die Umleitung ins Kind legen
  (`exec <cmd> >"$LOG" 2>&1` in einem Wrapper-Skript).
- **Fest verdrahtete Klickkoordinaten verrotten mit der Bibliothek des Nutzers.**
  `MY_STATS_CLICK_Y=692` kam mit #469 und stimmte um 05:00 noch. Bis 15:30 hatte
  die Seitenleiste zwei Zeilen verloren; der Klick landete im Leerraum. Der tote
  Klick war unsichtbar — das Skript wartete danach 24 Runden auf einen Künstler
  und meldete „window never exposed expected accessible label", während im
  Fenster noch die Musikbibliothek stand. Behoben in `7e3138eabe`: Koordinate neu
  gemessen (615, Zeilenabstand 38 px) **und** eine Selbstprüfung auf ein
  ansichtseigenes Steuerelement davorgesetzt, die die Koordinate in der
  Fehlermeldung nennt. Der AT-SPI-Weg hilft hier nicht: alle Seitenleisten-Zeilen
  melden Rahmen `0,0`, nur die Größen sind echt. Ein Tastaturweg existiert auch
  nicht — „My Stats" hat keinen Accelerator, und `session_restore.rs` bildet
  `ViewSource::MyStats` bewusst auf `SessionSource::Library` ab.
- **Ein Orakel kann seine Vorbedingung verlieren, wenn das Fundament vorrückt.**
  Zeile 777 verlangte, dass der Vorher-Arm für „The Devil Wears Prada" die graue
  Silhouette reproduziert. Das war die Lage von #469 — damals war der Vorher-Arm
  ein dev **ohne** Kennungsliste. Heute enthält dev #469, Pradas Platzhalter läuft
  über die Leerstring-MD5, also über eine der zwei strukturellen Kennungen aus E3,
  und dev fängt ihn schon bei der Auswahl ab. Die Forderung war **konstruktiv
  unerfüllbar** und sagte nichts über den geprüften Code. Behoben in `fbe65a00f9`:
  Prada ist jetzt der **Kontrollarm** (gleiche Bytes in beiden Armen beweisen,
  dass der Fingerabdruck einen bereits korrekten Künstler nicht anfasst), Oceano
  bleibt das Subjekt. Derselbe Fehlertyp wie die unerfüllbare Margenregel im
  Vorlauf: nicht die Messung war falsch, sondern die Latte.
- **Ränge sind kein Anker.** Oceano stand um 05:00 auf Rang 10, um 15:49 auf
  Rang 13. Die Review-Notizen nannten Rangnummern; sie nennen jetzt Namen.
- **Dieselbe Auswahl kommt nicht als dieselben Bytes.** Oceanos Vorher-Foto kam
  im zweiten Lauf als `ca747e27…`, im dritten als `ecc1ec0c…`. Genau deshalb
  prüft das Orakel gegen Referenzen und nicht auf Gleichheit. Die Silhouette
  hinter der Leerstring-MD5 war dagegen über den ganzen Tag byte-stabil
  (`bd8dae14…`, 16802 Byte) — vorab per `curl` gegen den CDN geprüft, bevor der
  Lauf startete.
- **Der Bildkorpus hat die gelöschten Referenzen gerettet.** Die im Manifest von
  #469 genannten Cache-Dateien sind mit dem einmaligen Cache-Löschen weg. Der
  Korpus unter `~/.cache/reprise-portrait-corpus/` (227 Dateien, intakt) enthält
  sie byte-identisch: `790f849972c0966b9494944b5ef513f6.jpg` = `0d659e80…`,
  `d41d8cd98f00b204e9800998ecf8427e.jpg` = `bd8dae14…`. Das sind die zwei
  `--placeholder-reference`-Argumente.
- **Jeder Abnahme-Lauf kostet 4 GB.** `runs/<stempel>/build/origin-dev` ist ein
  vollständiger Quellbaum plus eigenes `target/`. `runs/` ist gitignoriert; die
  Build-Bäume der abgebrochenen Läufe sind entfernt, das Beweismaterial liegt noch.

## Aufruf des Prüfstands

```
cd /home/marvin/Projects/reprise-portrait-placeholder-fingerprint
./acceptance/deezer-placeholder-portraits/run-accept.sh \
  --source-db /home/marvin/.local/share/reprise/reprise.db \
  --placeholder-reference ~/.cache/reprise-portrait-corpus/790f849972c0966b9494944b5ef513f6.jpg \
  --placeholder-reference ~/.cache/reprise-portrait-corpus/d41d8cd98f00b204e9800998ecf8427e.jpg \
  --confirm-read-only-copy
```

Voraussetzungen: getrackter Baum sauber, Reprise nicht laufend (für die
DB-Kopie), Sockelpfad unter 107 Byte, echter Netzzugang zu Deezer. Über
`heavy-run` fahren, aber die Ausgabe im Kind umleiten — siehe erste Falle.
