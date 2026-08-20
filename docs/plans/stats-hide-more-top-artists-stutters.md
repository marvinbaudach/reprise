---
slug: stats-hide-more-top-artists-stutters
worktree: /home/marvin/Projects/reprise-stats-hide-more-top-artists-stutters
branch: feature/stats-hide-more-top-artists-stutters
phase: planned
codex_session:
created: 2026-08-20
---
# Das Ruckeln liegt nicht in der Sortierung

Klick auf „Hide more top artists" in My Stats lässt die Oberfläche sichtbar
ruckeln. Der Befund verdächtigte die Neusortierung aller ~151 Künstler. **Am
20.08.2026 gemessen und widerlegt.**

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

## Die Messung, die den Verdacht ausräumt

`StatsSnapshot::top_artists_sorted` (`stats_snapshot.rs:127`) auf einem
produktionsnah erzeugten Snapshot mit 151 Künstlern, Release-Profil, 1000
Wiederholungen, bewusst großzügig lange Zeichenketten (Label ~30 Zeichen, Pfade
~120 Zeichen, drei Cover-Kandidaten je Künstler):

| Aufruf | Median | Mittelwert |
| --- | --- | --- |
| `top_artists_sorted(SortBy::Plays)` | **24,09 µs** | 24,07 µs |
| `top_artists_sorted(SortBy::Time)` | 24,11 µs | 24,06 µs |
| nur `clone()`, ohne `sort_by` | 23,34 µs | 23,30 µs |

Zwei Schlüsse, beide belegt:

1. **24 µs sind rund 700-mal unter dem 16,7-ms-Budget eines 60-Hz-Frames.** Die
   Sortierung kann kein sichtbares Ruckeln erzeugen.
2. **Das Klonen dominiert, nicht das Sortieren** (23,3 von 24,1 µs). Die im
   Befund vorgeschlagene Richtung „beim Zuklappen gar nicht erst sortieren"
   würde also selbst dann fast nichts sparen, wenn die Zahl relevant wäre.

Die Ursache muss auf der GTK-Seite liegen. Das ist **nicht** gemessen — genau
das ist Task 1.

## Was der Klick tatsächlich tut

`crates/reprise-gnome/src/ui/stats/stats_bands_card.rs:196-210`:

```rust
move |button| {
    let reveal = !revealer.reveals_child();
    state.render(reveal);                    // synchron, VOR der Animation
    if reveal {
        revealer.set_visible(true);
        revealer.set_reveal_child(true);
    } else {
        revealer.set_reveal_child(false);
    }
    update_reveal_button(button, reveal);
}
```

`render` (`:44-62`) ruft je nach Richtung `render_continuation(&artists, sort_by)`
oder `clear_continuation()`. Und `clear_continuation` (`:64-70`) läuft **auch zu
Beginn von `render_continuation`** (`:73`) — beim Aufklappen werden also erst
alle GTK-Reihen aus den beiden Spalten-`Box`en entfernt und danach neu gebaut
(`stats_bands_more::build_row`), alles synchron im Klick-Handler, bevor der
Revealer überhaupt zu animieren beginnt.

Das ist der plausible Kandidat: bis zu ~136 Zeilen (151 minus der bereits
sichtbaren) werden pro Klick abgerissen und neu aufgebaut, im Main-Thread, im
selben Durchlauf wie der Animationsstart.

Plausibel — aber unbelegt. Nach der Sortier-Messung ist die Lehre gerade, dass
der naheliegende Verdacht falsch sein kann.

## Aufgaben

### Task 1 — Den Stall am laufenden Fenster messen

Ein Display-Test, der den Klick auslöst und die **Main-Thread-Blockade** misst,
nicht das Endergebnis.

Aufbau: Snapshot mit 151 Künstlern, Karte aufgebaut, dann Klick in beide
Richtungen (auf- und zuklappen), und dabei die Zeit messen, die der Main-Thread
am Stück nicht zum Zeichnen kommt. Im Repo etabliert dafür: ein Tick-Callback,
der die Abstände zwischen zwei Frames protokolliert — der größte Abstand um den
Klick herum ist der Stall.

**Beide Richtungen getrennt ausweisen.** Der Befund nennt das Zuklappen; die
Struktur oben lässt das Aufklappen teurer aussehen. Welche Richtung wirklich
ruckelt, ist Teil der Messung, nicht ihrer Voraussetzung.

**Zusätzlich zu trennen:** wieviel des Stalls entfällt auf `clear_continuation`
(Abriss) und wieviel auf `build_row` (Neubau)? Das entscheidet, welche der
Lösungsrichtungen überhaupt greift.

**Und mitzuzählen:** wieviele Fortsetzungszeilen die Karte tatsächlich baut. Die
151 stammen aus dem Befund; ob die Karte alle baut oder deckelt, ist ungeprüft
und gehört als Zahl in den Bericht.

Der Test ist zunächst **rot** und benennt den Betrag in Millisekunden.

Name mit Präfix `stats_23_`, damit `scripts/check-ux-traceability.sh` die
Kennung wiederfindet — die Karte gehört zu STATS-23.

**Achtung:** `stats_23_the_toggle_reorders_the_whole_row`
(`stats_bands_card_tests.rs:27-29`) ist bekannt flaky
(Memory *stats-23-cover-fallback-display-test-is-flaky*). Ein roter Nachbar ist
kein Beweis gegen diese Arbeit; einzeln nachfahren, bevor er als Regression
zählt.

**Auflage aus dem Grill — der Bericht muss die Zahlen tragen:** der
Abschlussbericht nennt für Task 1 ausdrücklich (a) den längsten Frame-Abstand in
Millisekunden **je Richtung**, (b) die Aufteilung Abriss gegen Neubau, (c) die
tatsächliche Zeilenzahl. Ohne diese drei Zahlen im Bericht ist Task 1 nicht
fertig, auch wenn der Test läuft.

**Akzeptanz:** Eine Zahl in Millisekunden für den längsten Frame-Abstand um den
Klick, getrennt nach Richtung und nach Abriss/Neubau, plus die Zeilenzahl.

### Task 2 — Die Richtung wird aus Task 1 gewählt

Der Befund nannte drei Richtungen. Welche taugt, hängt an Task 1 und wird hier
**bewusst nicht vorentschieden**:

- **Nur beim Zuklappen leeren** — greift nur, wenn der Abriss der teure Teil ist.
- **Das Leeren erst nach der Animation** — verschiebt die Arbeit aus dem
  Animationsfenster, spart sie aber nicht.
- **Fortsetzungszeilen wiederverwenden statt neu bauen** — der größte Umbau, und
  der einzige, der beim Aufklappen hilft, falls `build_row` dominiert.

Andere Richtungen sind erlaubt, wenn Task 1 sie nahelegt. Die Wahl ist Sache der
Implementierung.

**Auflagen aus dem Grill — die Wahl muss nachprüfbar sein:**

1. Der Commit-Text (oder ein Kommentar an der geänderten Stelle) **begründet die
   gewählte Richtung aus den Zahlen von Task 1** — welcher Anteil des Stalls
   damit verschwindet und warum die anderen beiden Richtungen ihn nicht treffen.
   Eine Begründung, die auf Plausibilität statt auf die Messung verweist, genügt
   nicht.
2. **Der Kontrollarm zeigt die Verbesserung als Zahl.** Derselbe Messaufbau wie
   Task 1, einmal vor und einmal nach der Änderung: vorher X ms, nachher Y ms,
   beide Zahlen im Bericht. „Der Test ist grün" ist kein Beleg — die Toleranz
   sagt nichts über die Größe des Gewinns.

**Wenn Task 1 keinen nennenswerten Stall findet** — weder am Abriss noch am
Neubau — dann liegt die Ursache woanders (Cover-Laden der Zeilen, Reflow der
ganzen Seite). Dann wird **nicht** vorsichtshalber etwas umgebaut: Task 2
entfällt, der Bericht sagt das ausdrücklich mit den Zahlen, und der Plan wird
neu geschnitten. Ein Umbau ohne gemessene Ursache ist hier der teure Fehler,
nicht die vorsichtige Wahl.

**Akzeptanz:** Der Test aus Task 1 wird grün, ohne dass eine Toleranz gelockert
wird — oder Task 1 belegt, dass es hier nichts zu holen gibt.

### Task 3 — Gegenmessung

Mutationsprobe an der in Task 2 gewählten Stelle — **genau ein Vorkommen** — und
Beleg, dass Task 1 rot wird. Erst committen, dann mutieren.

Entfällt zusammen mit Task 2, falls Task 1 keinen Stall findet.

## Nicht in diesem Plan

- **`top_artists_sorted`.** Gemessen, 24 µs, nicht die Ursache. Sie wird nicht
  „vorsichtshalber" optimiert.

## Belege

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`
- der Display-Test aus Task 1 **einzeln**, nicht über das Sammel-Gate
- `scripts/check-ux-traceability.sh`
- die Mutationsprobe aus Task 3
- die Vorher/Nachher-Zahlen des Kontrollarms aus Task 2

## Parallelität

**Ein Strang.** Task 2 ist ohne Task 1 nicht entscheidbar.

**Reihenfolge:** 1 → 2 → 3.

**Dateibesitz dieses Strangs:**

```
crates/reprise-gnome/src/ui/stats/stats_bands_card.rs
crates/reprise-gnome/src/ui/stats/stats_bands_more.rs
crates/reprise-gnome/src/ui/stats/stats_bands_card_tests.rs
crates/reprise-gnome/src/ui/stats/stats_view_widgets.rs
docs/ux-rules.md            (nur falls Task 2 STATS-23 berührt)
```

Dieser Strang ist der einzige der Welle, der `ui/stats/*` anfasst — mit den
übrigen vier Plänen gibt es keine Dateiüberschneidung außer der möglichen
`docs/ux-rules.md`. Diese Datei teilen sich alle Stränge der Welle; der Konflikt
wird **beim Landen** aufgeräumt, nicht vorher vermieden.

**Post-Merge-Querprüfungen:** keine.
