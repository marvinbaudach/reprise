# Handover — one-centering-path-for-jump-and-clear, 19.08.2026 abends

**Worktree:** `/home/marvin/Projects/reprise-one-centering-path-for-jump-and-clear`
**Branch:** `feature/one-centering-path-for-jump-and-clear` (sauberer Baum)
**Plan:** `docs/plans/one-centering-path-for-jump-and-clear.md` (`phase: coded`)
**Companion TODO:** `docs/plans/queue-centering-ignores-section-headers.md` (`phase: todo`)

Kein PR. Der Zweig steht auf `origin/dev = 7bb1a3c433` rebased; alle sechs
Aufgaben sind entschieden. Der vorige Handover ist verbraucht und entfernt
(so will es `docs/plans/README.md`).

---

## Was jetzt auf dem Zweig liegt

| Commit | Inhalt |
|---|---|
| `cd3cddeb29` | Task 1 — jeder Schreiber auf dem Zentrierpfad ist benannt |
| `d42bfb3c6a` | Task 2 — Kontrollarm zeichnet die Wertfolge auf, nicht den Endwert |
| `69f1aa81e2` | Task 5 — SEARCH-16 benennt den Zwischenzustand |
| `681f702e81`, `266bfbd85b` → `9bf04026bd` | Plandokumente |
| **`cde025ab67`** | **Task 3 — ein Pfad, ein Zug** |
| **`d8ba71602d`** | **Task 6 — NAV-19, Seitenleisten-Wechsel zentriert** |
| **`ea5a6f1aa6`** | Notnagel feuert nicht mehr gegen eine vorhergesagte Zentrierung |

**Task 4 ist ersatzlos entfallen.** Begründung unten.

## Die Wurzel, die den Ausschlag gab

Der vorige Handover schickte den Nachfolger los, GTKs Allokationsschreibung
über einen vorgesäten Bereich auf dem zentrierten Wert landen zu lassen. Das
ist **die halbe Wahrheit**, und die Messung hat die andere Hälfte geliefert:

```
Bereich vorgesät, kein Anker
  centered.reveal.seed     2927.0   ← unsere saubere Zentrierung
  gtk                      6561.0   ← upper − page, das Listenende
  centered.reveal.instant  2923.5   ← Korrektur
  gtk                      6561.0   ← und wieder darüber
```

Der vorgesäte Bereich verhindert nur, dass unsere Schreibung in den *alten*
Bereich geklemmt wird (714 bei 21 Treffern). Gegen GTKs eigene
Anker-Wiederherstellung hilft er nicht.

**`scroll_to` richtet die Zeile bedingungslos oben aus** — auch dann, wenn sie
bereits vollständig und mittig im Blick steht. Gemessen: aus `value = 2923.5`
*und* aus `value = 2927.0` machte `scroll_to(89)` beide Male `3026.0`
(= 89 × 34, der Zeilenanfang). Damit sind die Werte, die ein einziger Zug
halten kann, **genau die Zeilenkanten** — jeder andere Wert wird vom
Allokationsdurchlauf überschrieben.

Also: `centered_scroll_restore::centered_anchor` nimmt die Zeilenkante, die dem
arithmetischen Mittelwert am nächsten liegt, und übergibt GTK die Zeile, die
genau diesen Wert erklärt. Ergebnis: **ein Schritt**,
`centered.reveal.seed → 2924.0`, bei einem exakten Mittelwert von 2923.5.

**Der Preis:** höchstens eine halbe Zeile Versatz. In der gemessenen Geometrie
0,5 px von 239 px Viewport. Zwei Geschwistertests
(`start_3_loaded_track_is_selected_centered_and_marked_paused`,
`fil_9_filter_changes_center_the_visible_playing_track`) hielten den Pfad auf
`< 0.5` und halten ihn jetzt auf eine halbe Zeile — mit der Begründung im Test,
nicht als stillschweigend gelockerte Toleranz.

**Warum Task 4 entfällt:** `AdjustmentHold` korrigiert aus einem Idle. Er ist
damit *selbst* der zweite sichtbare Schritt, den er verhindern soll — die erste
Messung protokollierte das als vierstufige Prügelei. Die Verankerung lässt
nichts zu korrigieren übrig. `hold.release_now()` vor der Zentrierung bleibt
unverändert stehen.

## Nachweise, die schon gelaufen sind

- **23 betroffene Display-Tests grün** gegen den endgültigen Baum: die
  SEARCH-16-Familie, START-3 (beide), FIL-9, NAV-10b (fünf), BROWSE-4,
  die zwei Queue-Sektions-Tests, NAV-19 (beide).
- **`cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo test --workspace`** — alle grün.
- **Drei Mutationsnachweise**, je genau ein Vorkommen getauscht:
  - `RevealMotion::Instant` → `Glide`: Kontrollarm rot mit
    `gtk 6460.0 / glide.instant 2923.5 / gtk 6460.0` — exakt die Messung, die
    diesen Plan gestoppt hatte.
  - Kantenwahl → exakter Mittelwert: Kontrollarm rot (zwei Schritte,
    2927.0 → 2924.0), **Endwerttest daneben bleibt grün.** Genau der blinde
    Fleck, für den der Kontrollarm gebaut wurde.
  - NAV-19-Aufruf am Aufrufort entfernt: positiver Fall rot, negativer grün.

  **Achtung beim Wiederholen:** die Skripte nehmen die Mutation über
  `trap 'git checkout -- $FILE' EXIT` zurück. Das stellt **HEAD** wieder her.
  Erst committen, dann mutieren — sonst ist uncommittete Arbeit an derselben
  Datei weg, ohne Warnung und mit Exit 0. Ist mir heute genau so passiert.

## Das eine offene Stück

**Die volle Display-Suite läuft gerade im Hintergrund** gegen
`ea5a6f1aa6`, gestartet abends:

```
scripts/check-display-tests.sh   # DISPLAY_TEST_JOBS=3
→ /tmp/claude-1000/-home-marvin-Projects-reprise/3af198a7-…/scratchpad/display-suite-final.log
```

Die Bilanz steht erst am Ende der Datei (`== display test summary ==`); vorher
sieht das Log leer aus, das ist normal — jeder Worker schreibt in sein eigenes
Log und alles wird am Schluss zusammengedruckt. Ein früherer Lauf wurde
verworfen, weil er eine Quelltextänderung überspannte und damit zwei Bäume
gemessen hätte.

Falls der Scratchpad weg ist: einfach neu starten. Die Suite ist
wiederholbar und braucht keinen Zustand aus dieser Sitzung.

Erwartung: grün, aber die Suite ist im Rudel bekannt flaky (STATS-23 und die
Display-Suite allgemein). Ein einzelner roter Test gehört einzeln nachgefahren,
bevor er als Regression gilt:

```
$SCRATCH/dt.sh <voller::test::pfad>     # ein Test, gleiche Isolation wie das Gate
```

**Nicht** `scripts/check-merge-readiness.sh` — das Sammel-Gate läuft nie durch
und hat den Plan schon zweimal je zwei Stunden gekostet.

## Danach

Der Zweig ist landereif, sobald die Suite grün ist. Es gibt keinen PR; das
Anlegen ist der nächste Schritt und war bewusst nicht Teil dieser Sitzung.

`wip/one-centering-path-rebuild` wird nicht mehr gebraucht — was daran taugte
(`RevealMotion`, `ScrollGlide::jump_to`, der Notnagel hinter den Versuchen, das
Abräumen der toten Helfer) ist übernommen, was rot war, ist ersetzt. Der Zweig
kann gelöscht werden.

`queue-centering-ignores-section-headers` bleibt offen und hat eine **neue
Adresse**: der Wiederherstellpfad rechnet die Kopfzeilen jetzt mit — er muss,
weil seine Kante aus derselben Geometrie kommen muss wie GTKs. Übrig ist der
**Sprungpfad** (`RevealMotion::Glide` → `scroll_center::centered_scroll_target`
→ `centered_scroll_value_with_height`), der weiterhin reine Zeilenmathematik
rechnet. Steht so im TODO.
