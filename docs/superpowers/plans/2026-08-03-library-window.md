---
slug: library-window
worktree: /home/marvin/Projects/reprise-library-window
branch: feature/library-window
phase: planned
codex_session:
created: 2026-08-03
---
# Die blätterbare Kern-Abfrage

**Goal:** Eine Kern-Abfrage, deren Anfrage aus Bereich, Suche und Ordnung
besteht und deren Antwort **Gesamtzahl und Fenster** trägt. Damit kann jede
Oberfläche ihr eigenes Vorladen wählen, statt an einer stillen Grenze zu
scheitern.

**Basis:** `dev` (`2684ed4d52`, Browse gemergt).

## Warum das jetzt kommt

Paket 3 hat gemessen, welche geteilten Nähte eine zweite Oberfläche wirklich
braucht. Die wertvollste stand in **keinem** der vier vermuteten P1a-Cluster:

`query_library_text_search` und `query_album_tracks` laufen über
`query_track_window` mit `MAX_WINDOW_LIMIT = 500` und geben weder Gesamtzahl
noch Offset zurück. **Eine Suche mit mehr als 500 Treffern wird still
abgeschnitten** — kein Fehler, keine Andeutung, einfach weniger Ergebnisse.
`query_albums` und `query_artists` haben das umgekehrte Problem: keine Grenze,
volle Materialisierung vor dem FFI-Übergang.

Der Desktop hat den richtigen Vertrag längst, aber nur für sich:
`TrackListModel` kennt die Gesamtzahl, lädt `WINDOW_SIZE = 200`-Fenster nach
Bedarf und hält höchstens acht im Cache. GTK virtualisiert Widgets, das Modell
die Daten. Compose' `LazyColumn` kann nur virtualisieren, was schon da ist —
und die fehlenden Zeilen nicht anfordern.

**Das ist keine Kotlin-Aufgabe.** Der Vertrag gehört in den Kern, sonst
schreibt Tauri ihn ein drittes Mal.

## Das ungemessene Risiko

Diese Grenze wurde **nie unter Last geprüft**. Der gesamte Android-Bestand
dieser Arbeit sind zwei Fixture-Dateien; sie widerlegen nichts. Bau den
Vertrag so, dass er auch dann stimmt, wenn niemand ihn an einer großen
Bibliothek ausprobiert hat — und **halte im Text fest, was weiterhin
ungemessen bleibt.**

## Global Constraints

- **Gates vor jedem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`,
  `bash scripts/check-architecture.sh`,
  `bash scripts/check-frontend-thinness.sh`,
  `bash scripts/tests/gettext-catalogs.sh`.
- **Exit-Codes einzeln erfassen**, nie durch eine Pipe. Testbilanz nach
  **Schlüsselwort** summieren.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Der Desktop darf sich nicht ändern.** `TrackListModel` behält sein
  Verhalten; wenn es den neuen Vertrag benutzen kann, ist das eine Vereinfachung
  und keine Verhaltensänderung — und muss als solche belegt werden.
- **Kein stilles Abschneiden mehr.** Wer weniger Zeilen bekommt als es gibt,
  erfährt es aus der Antwort.
- Kein `#[allow(…)]`, kein Schema-Wechsel.

---

## Task 1: Der Vertrag

**Files:**
- Modify: `crates/reprise-core/src/queries/`

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: Anfrage und Antwort benennen**

Eine Anfrage aus Bereich (ganze Bibliothek, Album, Interpret), Suchtext,
Ordnung und Fenster. Eine Antwort aus **Gesamtzahl** und den Zeilen dieses
Fensters.

**Lies zuerst `TrackListModel`** — es ist der einzige existierende Klient
dieses Vertrags und weiß, was ein Klient wirklich braucht. Die Ausrichtung der
Fenster, die Cache-Größe und das Vorladen bleiben Sache der Oberfläche; der
Kern liefert Gesamtzahl und Fenster, mehr nicht.

**UniFFI muss die Typen tragen können** — keine anonymen Tupel, keine
Closures, keine `&str` in der Antwort.

- [ ] **Step 3: Volle Gates und Commit**

---

## Task 2: Die Fassaden auf den Vertrag

**Files:**
- Modify: `crates/reprise-core/src/queries/surface_browse.rs`
- Modify: `crates/reprise-android-ffi/src/`

- [ ] **Step 1: Umstellen**

`query_library_text_search` und `query_album_tracks` liefern Gesamtzahl und
Fenster. Album- und Interpretenlisten bekommen dieselbe Behandlung, statt
unbegrenzt zu materialisieren.

- [ ] **Step 2: Ein Test, der das Abschneiden beweist**

Mehr Zeilen als ein Fenster, dann prüfen: die Gesamtzahl stimmt, das Fenster
ist vollständig, und ein zweites Fenster liefert den Rest ohne Lücke und ohne
Dopplung. **Beobachte ihn rot**, bevor er grün wird — das ist der Test, den es
vorher nicht gab und dessen Fehlen die Grenze verdeckt hat.

- [ ] **Step 3: Volle Gates und Commit**

---

## Task 3: Die Oberfläche blättert

**Files:**
- Modify: `android/app/src/main/java/…`

- [ ] **Step 1: `LazyColumn` fordert nach**

Beim Erreichen des Fensterendes das nächste anfordern. Die Gesamtzahl macht
die Bildlaufleiste ehrlich.

- [ ] **Step 2: Volle Gates und Commit**

---

## Task 4: Festhalten

- [ ] **Step 1: Was gemessen ist und was nicht**

Der Vertrag ist geprüft; **das Verhalten an einer großen realen Bibliothek
nicht.** Schreib das so hin. Ein Vertrag, der unter Last nie lief, ist
korrekt, nicht bewährt.

- [ ] **Step 2: Ledger, Gates, Commit**
