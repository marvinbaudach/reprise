---
slug: compact-player-misses-external-artwork
worktree: /home/marvin/Projects/reprise-compact-player-misses-external-artwork
branch: feature/compact-player-misses-external-artwork
phase: shipped
codex_session:
created: 2026-08-18
---
# Plan: Externes Cover auch im Kompakt-Player

Der Befund vom 16.08.2026 nennt die Ursache bereits richtig; dieser Plan macht
daraus Aufgaben. Alle offenen Fragen des Befunds sind am 18.08.2026 am Code
beantwortet — es bleibt nichts zu raten.

## Ausgangslage im Code

`crates/reprise-gnome/src/ui/playback/external_media_artwork.rs` ist 38 Zeilen
lang und bedient **ein** Ziel:

```rust
let generation = self.bar_cover_generation.get().wrapping_add(1);
self.bar_cover_generation.set(generation);
let size = self.bar.cover_image().pixel_size().max(1);
…
crate::ui::podcasts::source_image::load_into_image(
    self.bar.cover_image(), …, generation, &self.bar_cover_generation);
```

Der Dateipfad-Weg `sync_cover()` (`now_playing_wiring.rs:247-309`) bedient
dagegen beide Ziele, mit **zwei** Generationszählern: `bar_cover_generation`
(`:250`) und `compact_cover_generation` (`:295`). Für YouTube- und
Podcast-Episoden gibt es keinen lokalen Cover-Pfad, also läuft nur der externe
Weg — und der endet bei der Leiste. Der Kompakt-Player behält den Platzhalter
aus `compact_player_layouts.rs:63`.

## Am Code geklärt (keine offenen Fragen mehr)

- **Genau ein Aufrufer:** `player_controller_wiring.rs:77`. Umbenennen ist
  billig — aber `player_bar_tests.rs:151` prüft den Aufruf **wörtlich**
  (`assert!(source.contains("sync_external_bar_artwork(snapshot.as_ref())"))`)
  und muss mitgezogen werden.
- **Die rechte Now-Playing-Spalte ist nicht betroffen.** Sie liest
  `external.art_url` / `external.fallback_art_url` selbst
  (`now_playing_effects.rs:104-105`). Kein drittes Ziel.
- **Radio wird gratis mitrepariert.** `ExternalSession` hat genau zwei
  Ausprägungen, `Podcast` und `Radio`; beide laufen über denselben
  Schnappschuss. Kein eigener Code, aber ein eigener Nachweis.

## Aufgaben

1. `sync_external_bar_artwork` in eine Fassung überführen, die beide Ziele
   bedient. Der Name trägt „bar" und ist danach falsch — umbenennen (etwa
   `sync_external_artwork`), den Aufrufer und den Quelltexttest in
   `player_bar_tests.rs:151` mitziehen, und den Doc-Kommentar in Zeile 1, der
   ausdrücklich nur die Leiste nennt.
2. Je Ziel ein eigener Generationszähler: die Leiste weiter über
   `bar_cover_generation`, der Kompakt-Player über `compact_cover_generation`.
   Nicht derselbe Zähler für beide — ein spät eintreffendes Bild darf nur sein
   eigenes Ziel verwerfen.
3. Je Ziel die **eigene** Größe erfragen. Die Leiste liest
   `self.bar.cover_image().pixel_size()`, der Kompakt-Player hat seine eigene
   (`compact_player_layouts.rs:56-57`, `COVER_SIZE`). Die Größe der Leiste darf
   nicht weitergereicht werden.
4. Den Kompakt-Player **nicht** vorab leeren. `now_playing_wiring.rs:299-302`
   hält fest, warum: das ließ jeden Trackwechsel innerhalb eines Albums im
   Mini-Player flackern, weil er keine Überblendung hat. Kein `set_placeholder`
   vor dem Laden.
5. `images_allowed` einmal je Durchlauf bestimmen (wie heute, `:15-19`) und an
   beide Ziele weiterreichen, statt es je Ziel neu abzuleiten.
6. `StartupTiming` bleibt an `snapshot.restored` gebunden, für beide Ziele
   gleich.

## Nachweis

1. Eine YouTube-Episode abspielen, in die Kompaktansicht wechseln: das
   Episodenbild steht dort, nicht das Noten-Symbol.
2. Dasselbe für eine Podcast-Episode.
3. Dasselbe für einen Radiosender mit Logo — der Weg gilt für `Radio` mit.
4. Ein gewöhnlicher Musiktitel mit lokalem Cover zeigt weiterhin beide Ziele
   korrekt; der Dateipfad-Weg ist unberührt.
5. **Kein Flackern:** innerhalb eines Albums mehrfach weiterschalten, der
   Mini-Player fällt nicht auf den Platzhalter zurück. Das ist die
   Regressionsfalle aus Aufgabe 4 und muss beobachtet werden, nicht behauptet.
6. Schneller Wechsel zwischen zwei Episoden: es bleibt das Bild der zuletzt
   gewählten stehen, nicht das der zuerst geladenen (Generationszähler).

## Parallelität

**Nicht teilbar.** Die Änderung liegt in einer Datei
(`external_media_artwork.rs`) plus ihrem einen Aufrufer und dem Quelltexttest.
Umbenennung und Zielverdopplung betreffen dieselben Zeilen; ein Schnitt setzte
zwei Stränge auf dieselbe Datei — genau das, was er verhindern soll.
