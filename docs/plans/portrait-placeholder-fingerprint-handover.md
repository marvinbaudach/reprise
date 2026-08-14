# Handover — Porträt-Platzhalter per Fingerabdruck

Stand: 2026-08-14, 14:11 CEST. Plan: `docs/plans/portrait-placeholder-fingerprint.md`
(`phase: reviewed`). Vorgänger: `deezer-placeholder-portraits.md`, gelandet als
#469. Dies ist die Übergabe für alles, was **noch offen** ist.

## Was vorher erledigt wurde

- **#469** (Deezer-Platzhalter, Kennungsliste) gelandet als `e77557ee62`,
  dev-Lauf grün.
- **#470** (`wait`-Defekt in `scripts/cua-common/session.sh`) gelandet als
  `1b87c5ce60`. `cua_common_stop_daemon` eskaliert jetzt nach 2 s auf SIGKILL;
  Kontrollarm belegt unter `docs/evidence/bounded-daemon-stop/`.
- **Porträt-Cache einmalig gelöscht** (162 Einträge, 26 MB) — die Freigabe dafür
  liegt vor, sie ist verbraucht.

Damit sind Punkt 1, 2 und 3 der vorherigen Übergabe erledigt. Punkt 4 (E1s
gebrochene Prämisse) ist zu diesem Plan geworden, Punkt 5 (Namensvetter-Gesicht)
im Grill bewusst gekippt.

## Wo die Arbeit steht

- **Branch** `feature/portrait-placeholder-fingerprint`, 6 Commits,
  **Worktree** `/home/marvin/Projects/reprise-portrait-placeholder-fingerprint`.
- Rebased auf `origin/dev` = `29b2edff4c` (#479), 0 zurück.
- **Nicht gepusht, kein PR.**
- **Ein Codex-Lauf ist noch in der Luft** (`/refactor`, drei Review-Befunde). Der
  Arbeitsbaum ist deshalb schmutzig: `mod.rs`, `placeholder.rs`,
  `placeholder_measurement.rs`. Erst prüfen, ob er noch läuft —
  `pgrep -f "<worktree>/.pipeline-findings.md"` — bevor irgendetwas angefasst
  wird.

Produktcode: `placeholder.rs` (208 Z.) neu, `mod.rs` +157, `deezer.rs` +23,
`cache.rs` +6, dazu `placeholder_measurement.rs` (235 Z., testgebunden) und das
korrigierte Abnahme-Orakel. Kein GNOME-Code, keine neue Abhängigkeit —
`image = "0.25"` war schon da.

## Der Kern in drei Sätzen

Deezers graue Silhouette wird nicht mehr an der Bildkennung erkannt, sondern am
Bild: 32×32 Graustufen, normalisierter RMSE gegen zwei hinterlegte
Referenz-Miniaturen, Schwelle `0.0025`. Ein Treffer heißt „kein Porträt" — es
wird **kein** zweiter Kandidat probiert. Liegt bereits ein Bild im
Zwischenspeicher, überlebt es die Ablehnung und wird aufgefrischt, statt von der
Negativ-Marke gelöscht zu werden.

---

## Offene Punkte

### 1. Den laufenden Codex-Lauf einsammeln

Drei Review-Befunde sind unterwegs (`.pipeline-findings.md` im Worktree):

- **MEDIUM** `mod.rs:134` — `cache::refresh_image(…).unwrap_or_else(|| path.clone())`
  schluckt jeden Fehlschlag. Schlägt das Auffrischen fehl, bleibt der Zeitstempel
  alt, der Eintrag gilt ewig als veraltet, und die App lädt und verwirft
  denselben Platzhalter bei **jedem** Aufruf — stumm. Braucht eine WARN-Zeile im
  `None`-Zweig, symmetrisch zu den beiden E5-Zeilen.
- **LOW** — jedes angenommene Bild wird zweimal dekodiert
  (`placeholder::thumbnail`, dann `validated_image_extension`).
- **LOW** — die Test-Fixtures verlassen sich darauf, dass `Lanczos3` bei 32×32 →
  32×32 exakt die Identität ist. Ein `image`-Upgrade kann sie nahe der Schwelle
  kippen.

Danach muss die E6-Messung neu laufen und `rust-separation.txt` aktualisiert
sein; die Margen müssen weiter ≥10× über dem schlechtesten Platzhalter und ≥20×
unter dem nächsten echten Bild liegen.

### 2. Das Gate selbst fahren — die Behauptung ist unbelegt

Codex behauptet einen vollständigen Merge-Readiness-Lauf inklusive „672/672
Display-Tests". **Dafür gibt es kein Artefakt.** `check-display-tests.sh` legt
seine Ergebnisse in ein `mktemp -d`, das ein `trap … EXIT` löscht — es *kann*
nichts überleben, grün wie rot. Kein Push, kein PR, kein CI-Lauf.

Beschlossen ist: rebasen (erledigt) und `fmt`, `clippy -D warnings` und
`cargo test -p reprise-core` **selbst** fahren. Die Display-Suite bleibt aus, weil
die Änderung reiner Core-Code ohne GNOME-Anteil ist. Was der Rust-Reviewer bereits
gemessen hat: `cargo check` und `clippy -D warnings` sauber,
`cargo test -p reprise-core artist_portrait` 35 grün / 1 ignoriert — aber vor dem
Rebase und vor den drei Fixes.

### 3. Live-Abnahme (Kriterium 8) — nie gelaufen

`acceptance/deezer-placeholder-portraits/run-accept.sh` ist auf das neue Verhalten
angepasst (Rang 10 zeigt Initialen), aber nie gefahren; `runs/` ist leer. Der
Reviewer hat das Orakel gelesen und bestätigt, dass es fehlschlagen *kann*
(`negative_marker_for` verlangt Marke **und** kein Bild, unter `set -euo pipefail`).
Belegt ist die sichtbare Wirkung damit nicht.

Wer sie fährt, braucht: einen Sockelpfad unter 107 Byte, den
`test-fixtures`-Build, und muss wissen, dass Ränge 6–20 erst nach dem Klick auf
„Show more top artists" existieren.

Erwartete sichtbare Wirkung in dieser Bibliothek: **vier Silhouetten verschwinden**
(Aetheriality, In Your Grave, Our Vices, Wake Me → Initialen), und **Oceano
wechselt vom Fremdgesicht auf Initialen**.

### 4. Landen

Nichts hindert es nach 1–3. Wie üblich: rebasen, pushen, sofort mergen, nicht auf
CI warten (`scripts/land.sh` liegt unter `~/.claude/skills/pipeline/scripts/`).
`.pipeline-codex.md` ist **getrackt** und blockiert den Rebase als ungestagte
Änderung — vorher `git checkout --` darauf.

---

## Entscheidungen, die im Grill gefallen sind

Wer den Plan ändern will, ändert eine davon — sie sind nicht beiläufig.

| # | Beschluss | Warum |
|---|---|---|
| E0 | Der **Fehlalarm** ist der teure Fehler | Ein verworfenes echtes Bild ist unsichtbar und wird nie gemeldet; ein durchgelassener Platzhalter fällt auf und kostet eine Zeile. |
| E1 | 32×32 grau, RMSE, Schwelle 0,0025 | Beste gemessene Marge; der Entwurf mit 16×16/0,15 hätte **sieben echte Cover** verworfen. |
| E2 | Kein Rückfall auf den nächsten Kandidaten | Der Apparat hätte in 195 Künstlern genau **ein** Bild gekauft — das Gesicht eines Fremden. |
| E3 | Kennungsliste schrumpft auf die zwei strukturellen Einträge | „Kein Bild" ist strukturell, „Silhouette" ist inhaltlich. Zwei Sorten, zwei Mechanismen. |
| E4 | Ablehnung darf kein vorhandenes Bild löschen | `write_negative` löscht Bilddateien (`cache.rs:92-94`). |
| E5 | WARN bei Ablehnung **und** im Graubereich | Beide Fehlerrichtungen sind sonst unsichtbar. |
| E6 | Schwelle mit dem Rust-Code messen, Margenregel als Latte | PIL rechnet Rec. 601, `image` Rec. 709 — das kostete real ein Viertel der Trennung (330× → 241,8×). |

---

## Zahlen, auf denen alles steht

Messung über die **ganze** Bibliothek, 195 Künstler, 238 Kandidatenbilder,
0 Fehlschläge. Korpus: `~/.cache/reprise-portrait-corpus/` (227 Dateien, nach
Kennung benannt, **nicht** im Repo — Deezers Bytes). Reproduzierbar über
`docs/evidence/portrait-placeholder-fingerprint/library-sweep.py`.

```
schlechtester Platzhalter   0.000245098   → Marge 10,200×
nächstes echtes Bild        0.059268005   → Marge 23,707×
Schwelle                    0.0025        Fenster [0.00245098 , 0.00296340]
verworfen                   18/18 Platzhalter, 0/219 Fotos
```

**18 Platzhalter-Instanzen in einer Bibliothek**, zehn über die Leerstring-MD5,
acht über gewöhnliche Kennungen. Die Kennungsliste aus #469 bräuchte hier heute
zehn Einträge statt zwei.

---

## Fallen, die diesen Lauf gekostet haben

- **Eine Stichprobe aus dem Kopf der Verteilung lügt.** 22 Bilder der Ränge 1–18
  ergaben Faktor 6 Trennung und eine Schwelle, die sieben echte Cover verworfen
  hätte. Die Spitzenkünstler haben dunkle Pressefotos; helle, flächige Cover
  sitzen im Schwanz. Erst alle 195 Künstler zeigten die richtige Größenordnung.
- **Bilder anschauen, nicht nur Abstände lesen.** Dass „Currents" bei 0,0507 ein
  Teller mit Besteck ist und kein Platzhalter, stand in keiner Zahl.
- **Eine Margenregel kann arithmetisch unerfüllbar sein.** „Beidseitig ≥20×"
  verlangt 400× Gesamttrennung; gemessen waren 330× (Python) und 241,8× (Rust).
  Codex hat korrekt angehalten. Die Regel war der Fehler, nicht die Messung.
- **PIL ≠ `image`.** Rec. 601 gegen Rec. 709 und ein anderer Lanczos-Kern kosten
  ein Viertel der Trennung. Referenzen und Schwelle gehören in denselben Code,
  der sie später vergleicht.
- **Dieselbe Zeichnung kommt nicht als dieselben Bytes.** Zwei Sweeps im Abstand
  von 2,5 Stunden liefern für dieselbe Kennung 0,00000 und 0,00017. Ein Byte-Hash
  hätte die Mehrheit der Instanzen verfehlt.
- **`/tmp` wird stündlich abgeräumt.** Sonden, CSV und Bildkorpus waren einmal
  weg. Alles, was eine Messung belegt, gehört ins Repo oder nach `~/.cache/`.
- **Der Lastregler liest den Kommandotext.** Ein `git log`-Aufruf, in dessen
  Kommandozeile das Wort `codex-run.sh` vorkommt, wird als schwerer Lauf
  blockiert.
