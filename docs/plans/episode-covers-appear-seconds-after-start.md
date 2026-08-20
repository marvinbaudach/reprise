---
slug: episode-covers-appear-seconds-after-start
worktree: /home/marvin/Projects/reprise-episode-covers-appear-seconds-after-start
branch: feature/episode-covers-appear-seconds-after-start
phase: planned
codex_session:
created: 2026-08-20
---
# Die sichtbaren Zeilen stehen in derselben Schlange wie alle anderen

Nach dem Start zeigen Kanal- und Episodenzeilen sekundenlang das graue
Kamera-Fallback; die Bilder liegen lokal. Der Befund vermutete die Bildarbeit
selbst. **Am 20.08.2026 gemessen — sie ist es nicht.**

**Dieser Plan lässt die Lösungsrichtung bewusst offen.** Das ist im Grill so
entschieden worden: Task 1 misst, und **Task 2 wählt die Richtung anhand der
gemessenen Zahlen selbst**. Die Entscheidung liegt bei der Implementierung — sie
muss aber im Diff und im Abschlussbericht **nachprüfbar begründet** sein. Wie,
steht als Auflage bei den Aufgaben.

**Alle Zeilenangaben gegen `51e9c6c9bb`.** Basis dieses Worktrees ist
`origin/dev` = `40655644fc`; die zwei Commits dazwischen (#583, #584) fassen nur
`.github/workflows/ci.yml` und `scripts/tests/cua-*.sh` an, keine Quelldatei
dieses Plans. Die Zeilennummern gelten unverändert — trotzdem vor dem Ändern
kurz gegenlesen.

## Was die Messung ausschließt

Zielgröße der Skalierung: Episodenzeilen übergeben `(36, 36)`
(`podcasts_row_interaction.rs:18-26`, `MediaShape::Square`); `decode_pixels`
(`source_image_texture.rs:33-52`) gibt `width*2, height*2` an
`Pixbuf::from_file_at_scale` — effektiv **72×72** physische Pixel.

Gemessen wurde `decode_pixels` selbst, Release-Profil, auf echten Dateien auf
NVMe/Btrfs (nicht tmpfs), kalt mit `posix_fadvise(DONTNEED)` vor jedem Lesen:

| Quelle | warm (n=40) | kalt (n=10) |
| --- | --- | --- |
| 600×600 | 3,58 ms | 4,03 ms |
| 1400×1400 | 13,56 ms | 12,84 ms |
| 3000×3000 | 81,45 ms | 65,69 ms |

Kalt und warm liegen dicht beieinander — der Plattenzugriff ist gegenüber
Dekodieren und Skalieren vernachlässigbar. Real gemessener Durchsatz über acht
Worker: 1141 / 374 / 92 Bilder pro Sekunde.

Daraus für 60 sichtbare Zeilen: **~30 ms** (600 px), **~96–160 ms** (1400 px),
**~493–654 ms** (3000 px). Selbst der pessimistischste Fall bleibt unter einer
Sekunde.

**Und die echten Cover liegen am untersten Ende dieser Spanne.** 400 Dateien aus
`~/.cache/reprise/covers` vermessen:

| | Kantenlänge |
| --- | --- |
| Minimum | 47 px |
| Median | **48 px** |
| p90 | 96 px |
| Maximum | 1024 px |

Verteilung: 331× 48 px, 34× 96 px, 21× 1024 px. Der reale Fall liegt damit
**unterhalb** der kleinsten gemessenen Zeile der Tabelle — die Bildarbeit
erklärt die gemeldeten „paar Sekunden" nicht einmal ansatzweise.

## Was übrig bleibt

Vier Kandidaten, keiner davon gemessen. Der erste ist strukturell bereits
belegt und der einzige, der die *gestaffelte* Erscheinung erklärt, die der
Screenshot zeigt (zwei Kanäle hatten ihr Bild, der Rest nicht).

**1. FIFO ohne Sichtbarkeitsvorrang.** `source_artwork_queue.rs` ist eine
schlichte Warteschlange über `ARTWORK_WORKERS = 8`, gefüttert in der Reihenfolge
der `queue()`-Aufrufe. Eine gezielte Suche nach einer Bevorzugung sichtbarer
Zeilen (`is_visible`, `visible_range`, Priorität) blieb **ohne Treffer**. Stehen
beim Start viele unsichtbare Aufträge — Episoden weiter unten, andere Kanäle,
Kanal-Kopfzeilen — vor den sichtbaren, warten die sichtbaren trotz günstiger
Einzelkosten hinter der ganzen Schlange. Die Länge dieser Schlange beim echten
Start ist **nicht gemessen**.

**2. Das Quiet-Gate.** `startup_quiet.rs` öffnet bei erstem gemaltem Frame plus
100 ms (`Priority::LOW`) und lässt dann **alle** angemeldeten Arbeiten auf einen
Schlag los — nicht gestaffelt. Die 100 ms erklären nichts; wie lange es bis zum
ersten gemalten Frame dauert, ist nicht gemessen.

**3. Wettbewerb mit dem Startscan.** Die acht Worker teilen sich die Kerne mit
dem Bibliotheks-/Cover-Scan. Die Messung oben lief auf einer ruhigen Maschine
und sagt über diesen Fall nichts.

**4. Der Texturspeicher überlebt den Prozess nicht.** Beim Start muss jede
sichtbare Zelle den vollen Weg gehen. Das ist der Grund, warum es überhaupt nur
beim Start auffällt — aber allein erklärt es die Dauer nicht, siehe Messung.

## Aufgaben

### Task 1 — Die Schlange beim echten Start messen

Ohne diese Zahl ist jede Lösung geraten.

Auf dem Erfolgspfad gibt es heute **keine** Zeitmessung: die vier vorhandenen
`tracing`-Aufrufe (`source_artwork_queue.rs:48,176,210`, `source_image.rs:382`)
sind sämtlich Fehlerpfade. Es fehlt also die Instrumentierung, nicht nur die
Messung.

Zu erheben, je Auftrag: Zeitpunkt des `queue()`-Aufrufs, Zeitpunkt des
Arbeitsbeginns im Worker, Zeitpunkt der Rückkehr auf den GTK-Thread — und ob die
Zeile beim Anmelden sichtbar war. Daraus die beiden Zahlen, um die es geht:
**wie lang ist die Schlange beim Start, und wie lange wartet eine sichtbare
Zeile darin.**

**Mitzuzählen, weil der Grill danach gefragt hat:** wieviele Zeilen beim Start
überhaupt sichtbar sind. Die Rechnung oben nimmt 30 und 60 an; die echte Zahl
entscheidet mit, ob Vorrang genügt.

Die Erhebung läuft headless nach dem Isolationsvertrag aus `AGENTS.md`
(`dbus-run-session`, `xvfb-run`, eigene `XDG_*`-Wurzeln, `REPRISE_AUDIO_SINK=fakesink`)
gegen ein **geimpftes** Profil — niemals gegen `~/.local/share/reprise/reprise.db`.

**Auflage aus dem Grill — der Bericht muss die Zahlen tragen:** der
Abschlussbericht nennt für Task 1 ausdrücklich (a) Median und Maximum der
Wartezeit sichtbarer Zeilen, (b) die Zahl der Aufträge vor der ersten sichtbaren
Zeile, (c) die Zahl der beim Start sichtbaren Zeilen, (d) die Zeit bis zum
Öffnen des Quiet-Gates. Ohne diese vier Zahlen ist Task 1 nicht fertig.

**Akzeptanz:** Eine Verteilung „Wartezeit sichtbarer Zeilen" mit Median und
Maximum, und die Zahl der Aufträge, die vor der ersten sichtbaren Zeile in der
Schlange standen.

### Task 2 — Die Richtung wird aus Task 1 gewählt

Der Befund fragte, ob ein sitzungsübergreifender Cache der skalierten Pixel
lohnt oder ob Priorisierung reicht. **Task 1 beantwortet das**, und vorher wird
es nicht entschieden:

- Ist die Schlange lang und die Wartezeit dominiert → **Sichtbarkeitsvorrang**.
  Der kleinere Eingriff, und er trifft die gemessene Ursache.
- Ist die Schlange kurz und die Zeit steckt in der Summe der Arbeit → **Cache
  über Sitzungsgrenzen**. Größer, und er verlagert Kosten auf die Platte. Vor
  dieser Wahl gegenrechnen, dass der Median-Cover 48 px groß ist: ein Cache für
  eine Arbeit, die je Bild im einstelligen Millisekundenbereich liegt, muss sich
  erst rechtfertigen.
- Steckt sie vor dem Gate → weder noch, dann ist es ein Startzeit-Thema, und der
  Plan wird neu geschnitten statt durchgezogen.

Fällt die Wahl auf Vorrang, ist „sichtbar" nicht die einzig mögliche Ordnung —
ob der Kanal-Kopf vor seine Episoden gehört, entscheidet ebenfalls die
Implementierung und begründet es aus den Zahlen.

**Auflagen aus dem Grill — die Wahl muss nachprüfbar sein:**

1. Der Commit-Text (oder ein Kommentar an der geänderten Stelle) **begründet die
   gewählte Richtung aus den Zahlen von Task 1** — welcher Anteil der Wartezeit
   damit verschwindet und warum die anderen Richtungen ihn nicht treffen. Eine
   Begründung, die auf Plausibilität statt auf die Messung verweist, genügt
   nicht.
2. **Der Kontrollarm zeigt die Verbesserung als Zahl.** Derselbe Messaufbau wie
   Task 1, einmal vor und einmal nach der Änderung: vorher X ms Wartezeit
   (Median/Maximum), nachher Y ms, beide Zahlen im Bericht. „Sieht schneller
   aus" ist kein Beleg.

**Akzeptanz:** Der Kontrollarm aus Task 1 zeigt die Verbesserung als Zahl, nicht
als Eindruck.

### Task 3 — Gegenmessung

Mutationsprobe an der in Task 2 gewählten Stelle — **genau ein Vorkommen** — und
Beleg, dass Task 1 rot wird. Erst committen, dann mutieren.

Entfällt zusammen mit Task 2, falls Task 1 die Ursache vor dem Gate verortet.

## Nicht in diesem Plan

- **`decode_pixels` optimieren.** Gemessen; bei den realen Covergrößen (Median
  48 px) im einstelligen Millisekundenbereich. Es wird nicht „vorsichtshalber"
  beschleunigt.
- **`compact-player-misses-external-artwork`** — dasselbe Bildmaterial, andere
  Oberfläche, eigener Befund.

## Belege

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`
- die Erhebung aus Task 1, headless und isoliert
- die Vorher/Nachher-Zahlen des Kontrollarms aus Task 2
- die Mutationsprobe aus Task 3

## Parallelität

**Ein Strang.** Task 2 ist ohne Task 1 nicht entscheidbar.

**Reihenfolge:** 1 → 2 → 3.

**Dateibesitz dieses Strangs:**

```
crates/reprise-gnome/src/ui/podcasts/source_image.rs
crates/reprise-gnome/src/ui/podcasts/source_image_texture.rs
crates/reprise-gnome/src/ui/podcasts/source_artwork_queue.rs
crates/reprise-gnome/src/ui/podcasts/source_image_fallback.rs
crates/reprise-gnome/src/ui/podcasts/podcasts_row_interaction.rs
crates/reprise-gnome/src/ui/startup_quiet.rs
docs/ux-rules.md            (nur falls Task 2 eine Zusage berührt)
```

Dieser Strang ist der einzige der Welle in `ui/podcasts/*` — keine
Dateiüberschneidung mit den übrigen vier außer der möglichen `docs/ux-rules.md`.
Diese Datei teilen sich alle Stränge der Welle; der Konflikt wird **beim Landen**
aufgeräumt, nicht vorher vermieden.

**Post-Merge-Querprüfungen:** keine.
