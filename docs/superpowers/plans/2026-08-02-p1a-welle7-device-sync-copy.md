---
slug: p1a-welle7-device-sync-copy
worktree: ~/Projects/reprise-p1a-welle7-device-sync-copy
branch: feature/p1a-welle7-device-sync-copy
phase: refactored
codex_session:
created: 2026-08-02
---
# P1a, Welle 7 — die Geräte-Seite als Projektion

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `device_sync_page_copy.rs` zieht nach `reprise-view`. Damit ist
erprobt, ob eine Datei, die sich selbst „plain projection over already-computed
state" nennt, tatsächlich geteilt werden kann.

**Basis:** `dev`.

## Warum diese Welle, nachdem der Vorrat als erschöpft galt

Nach Welle 6 war der **adversarisch gegengeprüfte** Umzugsvorrat leer. Das galt
für die damals vermessenen Bereiche. Eine neue Messung am 2026-08-02, diesmal
über den ganzen Crate und mit korrigiertem Testfilter, ergab 6.712 Zeilen in
freistehenden Funktionen ohne Toolkit-Bezug, ohne Render-Aufruf und ohne
`self`. Das ist eine **Obergrenze**, keine Prognose; frühere Gegenprüfungen
haben von solchen Kandidaten 24–37 % bestätigt.

`device_sync` schien der größte Cluster. Die Einzelprüfung hat das widerlegt
und etwas Besseres ergeben:

- `reprise-core/src/device_sync/` hat bereits **35 Dateien, 11.889 Zeilen** —
  `machine.rs`, `selection.rs`, `delta.rs`, `preparation.rs`, `targets.rs`.
  **Die Sync-Logik ist längst im Kern.**
- `device_sync_effects.rs` führt aus, was `DeviceSyncMachine` entscheidet.
  Plattformarbeit, bleibt oberflächenspezifisch.
- `device_sync_picker_runtime.rs` orchestriert Kern-Queries. Von 216 `Db`-nahen
  Zeilen im ganzen GNOME-Crate enthalten nur **24** echtes SQL — die Schichtung
  ist in Ordnung.
- **`device_sync_page_copy.rs` ist der Rest, und er ist sauber.**

## Der Kandidat, gemessen

| | |
| --- | --- |
| Zeilen | 301 |
| Toolkit-Referenzen | **0** |
| Funktionen | 13 |
| Aufrufstellen | **2** |
| Eingaben | ausschließlich `reprise-core`-Typen (`DeviceView`, `SyncChangeSummary`, `MirrorBlocker`, `SyncPageWarning`, `SyncPageControls`, `PrimaryAction`, `TransferProfile`, `SyncPlaylistRow`) |

Der Modulkopf sagt: „Every function here is a plain projection over
already-computed state; none of it touches a widget."

**Ausführungskorrektur (2026-08-03):** Die Tabelle hat „Aufrufstellen" als
zwei konsumierende Dateien gezählt, nicht als einzelne Aufrufe, und
`DeviceView` fälschlich den Core-Typen zugerechnet. `DeviceView` sowie
`PreparationRunState` gehören zu GNOME; ersterer trägt sogar ein `gio::Icon`,
obwohl die Projektionsfunktionen es nie lesen. Genau dieser Messfehler machte
den ersten Rewrite-Commit notwendig: Die drei zu breiten `DeviceView`-Eingänge
wurden auf Core-Zustand und schmale Werte reduziert, der GTK-eigene
Vorbereitungslauf wird im Adapter in ein reines Fortschritts-DTO übersetzt.
Die 301 Zeilen waren produktive `cloc`-Zeilen; physisch hatte die Datei 333.

## Die zwei Entwurfsaufgaben

Diese Welle ist **kein reines Verschieben**, und das ist beabsichtigt.

**1. `String` wird `Message`.** Sechs Zeilen enthalten übersetzbaren Text. Ein
gerenderter `String` darf die Crate-Grenze nicht überqueren — Welle 1 hat für
genau das `reprise_view::strings::Message` gebaut, samt `Plural`. `counted(count,
singular, plural)` ist die Pluralform, für die `Message::plural` existiert und
für die `scripts/tests/gettext-catalogs.sh` bereits `--keyword=plural:1,2`
konfiguriert hat.

**2. Anonyme Tupel werden benannt.** `progress_copy` liefert heute
`Option<(String, String, String, f64)>`. **UniFFI kann kein anonymes Tupel
tragen** (siehe Spike). Was diese vier Werte bedeuten, muss ein Typ sagen.
`transfer_progress_copy` prüfen und gleich behandeln.

Wo eine Funktion sich gegen beides sperrt, **lass sie in `reprise-gnome`
zurück und begründe es im Commit** — eine ehrlich zurückgelassene Funktion ist
wertvoller als eine, die den `Message`-Typ verbiegt.

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
- **`view_floor` in `scripts/check-frontend-thinness.sh` steigt im selben
  Commit**, der den Code verschiebt. Es ist ein Boden, kein Deckel.
- **`reprise-view` darf nur von `reprise-core` abhängen.** Kein `gtk4`, kein
  `libadwaita`, kein `glib`, kein `gettextrs`.
- **Die zwei Aufrufstellen ändern sich nicht.** Adapter-Naht in
  `reprise-gnome`, die die Namen unter ihren alten Pfaden weiterreicht und
  `Message` in einen `String` rendert — wie die Wellen 1–6 es tun.
- **Keine sichtbare Verhaltensänderung.** Jeder heute erzeugte Text muss
  zeichengleich bleiben. Die Tests von `device_sync_page_*_tests.rs` sind der
  Beweis; sie bleiben grün, ohne dass ihre Erwartungen angefasst werden.
- Kein `#[allow(…)]`, keine neue Abhängigkeit, kein Schema-Wechsel.

---

## Task 1: Die Extraktion, ohne Crate-Wechsel

**Files:**
- Modify: `crates/reprise-gnome/src/ui/device_sync/device_sync_page_copy.rs`

- [ ] **Step 1: Baseline messen**

- [ ] **Step 2: `String` → `Message`, Tupel → Typ, an Ort und Stelle**

Regel 14 aus dem Multi-Surface-Plan: Extraktion und Crate-Umzug bleiben
getrennte Schritte. Dieser Schritt muss grün sein, **ohne dass
`reprise-view` angefasst und `view_floor` bewegt wurde.**

- [ ] **Step 3: Volle Gates und Commit**

---

## Task 2: Der Umzug

**Files:**
- Create: `crates/reprise-view/src/device_sync/…`
- Modify: `crates/reprise-gnome/src/ui/device_sync/device_sync_page_copy.rs` (Adapter)
- Modify: `scripts/check-frontend-thinness.sh` (`view_floor`)

- [ ] **Step 1: Verschieben und `view_floor` anheben**

Im selben Commit. Vorher beobachten, dass das Gate rot wird, wenn der Boden
nicht mitwandert — sonst ist unbelegt, dass er etwas bewacht.

- [ ] **Step 2: Volle Gates und Commit**

---

## Task 3: Der Beweis und das Festhalten

**Files:**
- Modify: `crates/reprise-view/src/…` (Tests)
- Modify: `docs/superpowers/plans/2026-08-01-p1a-waves.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: Ein Test in `reprise-view`, der ohne GTK auskommt**

Er prüft eine Projektion gegen eine `Message` — nicht gegen einen gerenderten
String, denn das Rendern findet erst in der Oberfläche statt.

- [ ] **Step 2: Den Wellenplan nachziehen**

`docs/superpowers/plans/2026-08-01-p1a-waves.md` sagt, der Vorrat sei nach
Welle 6 erschöpft. Das stimmte für die damals vermessenen Bereiche — halte
fest, was die neue Messung ergab, wie viel davon Obergrenze ist, und welche
vier Cluster als Nächstes zu prüfen wären (`tag_edit_flow`, `session_restore` +
`view_session`, `column_layout` + `keyboard_reorder`, `missing_view` +
`import_errors_view`).

- [ ] **Step 3: Ledger-Eintrag, volle Gates, Commit**

---

## Nach dieser Welle

Diese Welle ist ein **Test der Methode**, nicht nur ein Umzug. Bringt eine
Datei, die sich selbst als reine Projektion bezeichnet, ihre 301 Zeilen ohne
Verrenkung nach drüben, folgen die vier übrigen Cluster. Sperrt sie sich, ist
die Architekturfrage beantwortet: Android wird ein zweites Frontend auf einem
gemeinsamen Kern — was tragfähig ist, nur nicht das, was die Spec versprach.
