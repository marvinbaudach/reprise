---
slug: p1a-welle0-queue-naht
worktree: /home/marvin/Projects/reprise-p1a-welle0-queue-naht
branch: feature/p1a-welle0-queue-naht
phase: refactored
codex_session:
created: 2026-08-01
---
# P1a Welle 0 — Die Queue-Naht schließen

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `QueueViewModel` hält keinen Closure mehr. Das faule Nachladen der
Kontext-Zeilen wird von einem gespeicherten Rückruf zu einem expliziten
Aufruf — und ein Compile-Time-Nachweis verhindert dauerhaft, dass ein
Closure zurückkommt.

**Architecture:** Der Umbau passiert **in place** in `reprise-gnome`. Kein
Crate-Umzug, keine Sichtbarkeitsänderungen — die kommen erst mit Welle 7.
Form und Ort werden bewusst getrennt, damit beide einzeln prüfbar bleiben.

**Warum zuerst:** Spike-Befund zu Frage 3
(`docs/research/android-spike-2026-08.md`): UniFFI weist
`Rc<dyn Fn(usize, usize) -> Vec<i64>>` mit drei Trait-Fehlern ab. Jede
spätere Welle erbt das hier festgelegte Muster.

**Tech Stack:** Rust 1.92, gtk4-rs, bestehende Crate `reprise-gnome`.

**Spec:** `docs/superpowers/specs/2026-08-01-multi-surface-frontends-design.md`
**Wellenplan:** `docs/superpowers/plans/2026-08-01-p1a-waves.md`

## Global Constraints

- **Gates vor jedem Commit:** `cargo fmt --check`,
  `cargo clippy --all-targets --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo audit`.
- **Einzig akzeptiertes Advisory:** `RUSTSEC-2024-0436`.
- **Test-Baseline** in Task 1 messen; jeder Task nennt sie als Referenz.
- **Bekannte Sandbox-Fehlschläge** (falls in einer Sandbox gearbeitet wird):
  Display-gebundene GTK-Tests, `ReadOnlyFilesystem` in Cover-/Cache-Tests
  (`XDG_CACHE_HOME` in den Worktree setzen), und zwei `reprise-mcp`-
  Radio-Tests mit `PermissionDenied`. **Urteile nach Differenz zur Baseline,
  nicht nach Kategorie.**
- **Verhalten bleibt identisch.** Diese Welle ändert keine sichtbare
  Funktion; jede Verhaltensänderung ist ein Fehler.
- **Dateigrößenregel:** jede bearbeitete Datei endet < 800 Zeilen.
- **Commit-Format:** `<type>: <description>`, englisch.

---

### Task 1: Den Nachweis bauen, bevor die Form stimmt

Der Nachweis ist hier kein Test, sondern eine **Compile-Time-Zusicherung**.
`Rc<dyn Fn>` ist weder `Send` noch `Sync`; alle übrigen Felder sind es
(`QueueItem` ist `enum { Track(i64), Episode(i64) }`, `QueueSection` trägt
`u32` und `String`, `VirtualContextIdentity` trägt `(u64, u64)` und `usize`).
Eine `Send + Sync`-Zusicherung schlägt also **genau dann** fehl, wenn ein
Closure im Modell steckt — heute, und bei jedem künftigen Rückfall.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/track_list/queue_sections.rs`

**Interfaces:**
- Consumes: nichts.
- Produces: die Zusicherung, gegen die Task 2 arbeitet.

- [ ] **Step 1: Test-Baseline messen**

```bash
cargo test --workspace 2>&1 | tail -20
```

Baseline: `________ passed; ________ ignored`

- [ ] **Step 2: Die Zusicherung einfügen**

In `queue_sections.rs`, direkt nach den `use`-Zeilen:

```rust
/// P1a's binding rule: no view model may hold a closure, because UniFFI
/// cannot carry one across an FFI boundary — it rejects
/// `Rc<dyn Fn(usize, usize) -> Vec<i64>>` with three trait errors
/// (`TypeId`, `Lower`, `Lift`; see docs/research/android-spike-2026-08.md).
///
/// `Rc<dyn Fn>` is neither `Send` nor `Sync` while every other field here is,
/// so this assertion fails to compile the moment a closure comes back. It is
/// a permanent guard, not a one-off migration check.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<QueueViewModel>();
};
```

- [ ] **Step 3: Den Fehlschlag sehen — das ist der Beweis**

```bash
cargo build -p reprise-gnome 2>&1 | grep -A3 'cannot be sent between threads\|cannot be shared between threads' | head -12
```

Erwartet: **FEHLSCHLAG**, weil `QueueViewModel` über
`Option<VirtualContextTail>` ein `Rc<dyn Fn(usize, usize) -> Vec<i64>>` hält.
Kompiliert es **doch**, ist die Zusicherung wirkungslos — dann stimmt etwas
an der Annahme und der Task ist nicht erledigt, sondern zu klären.

Kein Commit in diesem Task: Der Baum ist absichtlich rot. Task 2 macht ihn
grün.

---

### Task 2: Den Closure aus dem Modell nehmen

**Files:**
- Modify: `crates/reprise-gnome/src/ui/track_list/queue_sections.rs`

**Interfaces:**
- Consumes: die Zusicherung aus Task 1.
- Produces: `VirtualContext` (Daten), das Trait `ContextWindow` (Verhalten),
  und `items_window`/`all_items` mit einem Anbieter-Parameter. Task 3 zieht
  die Aufrufstellen darauf nach.

- [ ] **Step 1: Den Typ teilen**

`VirtualContextTail` verliert seinen Closure und heißt fortan
`VirtualContext` — reine Daten:

```rust
/// How long the virtual context tail is, and which context it belongs to.
/// Deliberately data only: the rows themselves are fetched through
/// [`ContextWindow`], never through a closure the model carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VirtualContext {
    count: usize,
    identity: Option<VirtualContextIdentity>,
}

impl VirtualContext {
    #[cfg(test)]
    pub(crate) fn new(count: usize) -> Self {
        Self { count, identity: None }
    }

    pub(crate) fn identified(count: usize, sequence: (u64, u64), start: usize) -> Self {
        Self {
            count,
            identity: Some(VirtualContextIdentity { sequence, start }),
        }
    }
}

/// Supplies the context rows a [`QueueViewModel`] describes but does not
/// hold. The GTK side implements this over the windowed query; a future
/// Android side implements it over the same query behind UniFFI.
pub(crate) trait ContextWindow {
    fn rows(&self, offset: usize, limit: usize) -> Vec<i64>;
}
```

`QueueViewModel`s Feld wird `context: Option<VirtualContext>`.

- [ ] **Step 2: Den Zugriff umstellen**

`items_window` nimmt den Anbieter als Parameter statt ihn zu speichern. Die
bisherige Zeile `(context.window)(context_offset, context_limit)` wird
`tail.rows(context_offset, context_limit)`:

```rust
    pub(crate) fn items_window(
        &self,
        offset: usize,
        limit: usize,
        tail: &dyn ContextWindow,
    ) -> Vec<QueueItem> {
```

`all_items` reicht denselben Anbieter durch:

```rust
    pub(crate) fn all_items(&self, tail: &dyn ContextWindow) -> Vec<QueueItem> {
        self.items_window(0, self.total_len(), tail)
    }
```

`compose_virtual` nimmt `Option<VirtualContext>` statt
`Option<VirtualContextTail>`; sein Rumpf ändert sich nicht, weil er nur
`tail.count` liest.

- [ ] **Step 3: Die handgeschriebenen Trait-Impls prüfen**

`QueueViewModel` hat handgeschriebene `Debug`- und `PartialEq`-Impls, weil
der Closure keines von beidem konnte. Prüfe, ob sie jetzt entfallen können —
`VirtualContext` leitet beides ab. Wenn ja, ersetze sie durch
`#[derive(Clone, Debug, Default, PartialEq)]`; wenn nein, halte im
Kommentar fest, warum.

- [ ] **Step 4: Die Zusicherung grün sehen**

```bash
cargo build -p reprise-gnome 2>&1 | tail -5
```

Erwartet: kein `Send`/`Sync`-Fehler mehr. Andere Fehler an den
Aufrufstellen sind erwartet — die erledigt Task 3.

- [ ] **Step 5: Commit** (der Baum ist noch rot; das ist beabsichtigt und
  gehört in die Nachricht)

```bash
git add crates/reprise-gnome/src/ui/track_list/queue_sections.rs
git commit -m "refactor: make the queue view model closure-free

The model described its context tail and also carried the closure that
resolved it. UniFFI cannot carry a closure across an FFI boundary, so the
resolution moves out to a ContextWindow the caller supplies. A Send+Sync
assertion now fails to compile if a closure returns.

Call sites follow in the next commit; the tree does not build in between."
```

---

### Task 3: Die Aufrufstellen nachziehen

23 Dateien nennen `queue_sections`. Der Compiler führt durch die Liste — es
gibt keinen Grund, sie vorab zu raten.

**Files:**
- Modify: die vom Compiler benannten Aufrufstellen, darunter sicher
  `crates/reprise-gnome/src/ui/playback/queue_transport.rs`,
  `crates/reprise-gnome/src/ui/track_list/track_list_model.rs`,
  `crates/reprise-gnome/src/ui/window/window_queue_model.rs`,
  `crates/reprise-gnome/src/ui/now_playing/up_next_panel.rs`

**Interfaces:**
- Consumes: `VirtualContext`, `ContextWindow`, die neuen Signaturen aus Task 2.
- Produces: einen bauenden Baum mit unverändertem Verhalten.

- [ ] **Step 1: Die Liste vom Compiler holen**

```bash
cargo build -p reprise-gnome 2>&1 | grep -E '^error' -A2 | grep -oE '[a-z_/]+\.rs:[0-9]+' | sort -u
```

- [ ] **Step 2: Den Anbieter dort ansiedeln, wo der Closure herkam**

Wo heute ein `Rc<dyn Fn(usize, usize) -> Vec<i64>>` an
`VirtualContextTail::identified` übergeben wird, entsteht stattdessen ein
Typ, der `ContextWindow` implementiert und denselben Aufruf kapselt:

```rust
/// Resolves the context tail through the same windowed query the closure
/// used to hold. Lives on the GTK side because the query needs the
/// frontend's database handle; `reprise-view` only ever sees the trait.
struct QueueContextWindow {
    // dieselben Felder, die der bisherige Closure eingefangen hat
}

impl ContextWindow for QueueContextWindow {
    fn rows(&self, offset: usize, limit: usize) -> Vec<i64> {
        // exakt der Rumpf des bisherigen Closures
    }
}
```

**Nichts an der Abfrage selbst ändern.** Diese Welle verschiebt, wo der
Aufruf wohnt — nicht, was er tut.

- [ ] **Step 3: Bauen bis grün**

```bash
cargo build -p reprise-gnome 2>&1 | tail -5
cargo clippy --all-targets --workspace -- -D warnings 2>&1 | tail -5
```

- [ ] **Step 4: Der eigentliche Regressionsnachweis**

`queue_sections.rs` enthält den Test
`que_7_context_tail_is_not_materialised`. Er ist die Zusage, dass der
Kontext-Schwanz **nicht** vorab materialisiert wird — genau die Eigenschaft,
die dieser Umbau gefährden könnte.

```bash
cargo test -p reprise-gnome que_7_context_tail_is_not_materialised -- --exact --nocapture
cargo test -p reprise-gnome que_7 2>&1 | tail -5
```

Erwartet: grün. Ist er rot, hat der Umbau die Faulheit zerstört und die
Aufrufstellen holen zu viel — das ist ein echter Fehler, kein Testproblem.

- [ ] **Step 5: Volle Gates**

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
```

Erwartet: Testzahl **unverändert** gegenüber der Baseline aus Task 1. Diese
Welle fügt keine Tests hinzu und entfernt keine.

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-gnome/src/ui
git commit -m "refactor: supply the queue context tail instead of storing it

Every call site now hands the model a ContextWindow rather than a captured
closure. The windowed query is unchanged — only its owner moved."
```

---

### Task 4: Das Muster festschreiben

Was hier entstanden ist, gilt für sieben weitere Wellen. Es gehört
dokumentiert, wo die nächste Welle es findet — nicht nur in dieser
Commit-Historie.

**Files:**
- Modify: `docs/superpowers/plans/2026-08-01-p1a-waves.md`
- Modify: `.superpowers/sdd/progress.md`

- [ ] **Step 1: Das Muster in den Wellenplan**

Ergänze in §4 („Regeln, die für jede Welle gelten") unter Regel 6 den
konkreten Weg:

```markdown
   Das Muster steht in `track_list/queue_sections.rs` (Welle 0): Das
   ViewModel beschreibt die Länge, ein `ContextWindow`-Trait liefert die
   Zeilen, und eine `const`-Zusicherung auf `Send + Sync` hält den Closure
   dauerhaft draußen. Jede Welle mit faulem Nachladen folgt dieser Form.
```

- [ ] **Step 2: Ledger-Eintrag**

Eine Zeile im Hausformat an `.superpowers/sdd/progress.md` anhängen: Commit,
Basis, was bewiesen wurde (Zusicherung erst rot, dann grün; QUE-7 grün;
Testzahl unverändert).

- [ ] **Step 3: Volle Gates und Commit**

```bash
cargo fmt --check && cargo clippy --all-targets --workspace -- -D warnings && cargo test --workspace && cargo audit
git add docs .superpowers
git commit -m "docs: record the closure-free view model pattern for later waves"
```

---

## Nach dieser Welle

Welle 1 (Strings und reine Werte) etabliert die **Umzugs**-Mechanik —
Modulpfade, die 577 Sichtbarkeiten, wie `reprise-gnome` die neue Crate
konsumiert. Erst danach wandert `queue_sections.rs` selbst, in Welle 7.

Diese Trennung ist Absicht: Welle 0 ändert die **Form**, Welle 7 den **Ort**.
Beides in einem Schritt hätte bedeutet, dass ein Fehlschlag nicht mehr
zuzuordnen ist.
