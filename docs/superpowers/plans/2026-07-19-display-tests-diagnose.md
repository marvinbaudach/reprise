---
slug: display-tests-fix
worktree: /home/marvin/Projects/reprise/.worktrees/album-view-improvements-display-tests-fix
branch: feature/display-tests-fix
phase: shipped
codex_session:
created: 2026-07-19
---
# Display-Tests — Diagnose und Handoff (2026-07-19, 12:34)

Gemessen auf `main` = `0577121d`. Ersetzt die P0-Liste in
[`2026-07-19-folgetasks-plan.md`](2026-07-19-folgetasks-plan.md), die von
`3388046d` stammt und in beide Richtungen veraltet ist.

## Messmethode

209 Display-Tests, jeder als **Einzelprozess** unter eigenem
`xvfb-run -a dbus-run-session`. Kein Sammellauf — der bekannte
MainContext-Race macht parallele Läufe unbrauchbar.

```sh
cargo test -p reprise-gnome -- --ignored --list        # Pfade holen
xvfb-run -a dbus-run-session -- cargo test -p reprise-gnome \
  --quiet -- --ignored --exact <pfad> --nocapture
```

**Ergebnis: 10 rot von 209.** Der alte Plan nannte 9 von 166; davon sind
7 inzwischen grün, 8 der aktuellen Roten sind dort nicht verzeichnet.

## Fallstrick, der die Messung fast entwertet hätte

`cargo test --exact <pfad>` meldet bei einem **nicht existierenden** Pfad
`test result: ok` — bei null gelaufenen Tests. Der erste Lauf gegen die
Modulpfade aus dem alten Plan meldete deshalb alle neun als grün. Die
Pfade hatten sich durch Refactors verschoben.

**Regel: jedes `--exact`-Ergebnis gegen `--list` gegenprüfen oder auf
`N passed` statt auf `ok` prüfen.** Gleiche Fehlerklasse wie der
Fail-Fast-Runner: eine Meldung, die Abwesenheit von Prüfung als Erfolg
ausgibt.

## P0 — Echter Produktionsdefekt (1)

### Glass-Insets zielen auf den Viewport statt auf den Inhalt

`crates/reprise-gnome/src/ui/glass/insets.rs:91-98`

`SafeInsetApplier::discover` nimmt `scrolled.child()` als Inset-Ziel.
GTK4 wickelt jedes Kind, das **nicht** `GtkScrollable` implementiert,
automatisch in einen internen `GtkViewport` — `child()` liefert dann den
Viewport, nicht das übergebene Widget. Empirisch gegen GTK 4.22.4:

```
Label    -> child() = GtkViewport   (identisch mit Label?    nein)
Box      -> child() = GtkViewport   (identisch mit Box?      nein)
ListView -> child() = GtkListView   (identisch mit ListView? ja)
```

Die Margin landet auf dem Viewport und verkleinert die **Scroll-Öffnung**,
statt den **Inhalt** zu polstern — Gegenteil der Absicht in
`library_player_bar.rs:3-5`.

**Betroffen:** Ansichten mit nicht-scrollbarem Kind. Beide Diagnosen sind
sich bei `artist_detail_pane.rs:130` (`Box`) einig; darüber hinaus wurden
`sidebar.rs:217` (`ListBox`) bzw. `stats_view.rs:175` (`Stack`) genannt.
**Nicht betroffen:** Tracks (`ColumnView`), Alben (`GridView`), Artists
(`ListView`) — die sind `GtkScrollable`, dort landen die Insets korrekt.

**Schweregrad ist offen.** Eine Diagnose nennt es „totes Band, Glas
blurrt leeren Hintergrund", die andere „kosmetisch, Zeilen bleiben
erreichbar". Vor einem Commit-Text am echten Fenster nachmessen.

**Fix** in `collect_scrolled_children` — genau eine Ebene auspacken:

```rust
let target = scrolled.child();
let target = match target.and_downcast_ref::<gtk4::Viewport>() {
    Some(vp) => vp.child(),
    None => target,
};
```

Nicht pauschal Viewports überspringen — ein app-eigener `GtkViewport`
würde sonst ganz übergangen. Eine Ebene auspacken ist in beiden Fällen
richtig.

**Rote Tests aus dieser Wurzel (4):**
- `ui::glass::tests::inset_applier_adds_exact_padding_to_every_scrolled_child`
- `ui::window::library_chrome::tests::search_2a_search_reveal_extends_the_shared_top_glass_zone`
- `ui::player_bar::library_player_bar::tests::play_7a_player_bar_is_a_global_overlay_at_bottom_and_top`
- (viertes Vorkommen desselben Musters in `glass/mod.rs:147`)

**Wichtig:** Der Fix ist für zwei dieser Tests *notwendig*, aber **nicht
bewiesen hinreichend**. Der Pfad `GlassSurface::set_on_allocate → apply()`
(`surface.rs:68`, `library_player_bar.rs:68-93`) wurde nie mit einem
Nicht-Null-Inset beobachtet, weil dieser Bug ihn verdeckt. Nach dem Fix
**neu messen, nicht annehmen.**

Der Code ist einen Tag alt (`735ad53d`, `247861d2`, `4ef6b1cd`) und alle
betroffenen Tests hängen hinterm Display-Gate — sie waren vermutlich nie
grün, nicht regressiert.

## P1 — Testfehler, kein Nutzerfehler (6)

Für alle sechs gilt: **ein realer Nutzer sieht den Fehler nicht.**
Tastaturbedienung nach ACC-3 und SEARCH-2a funktioniert.

### `has_focus()` statt `is_focus()` (3 Tests)

GTK4 unterscheidet: `is_focus()` = Fokuswidget im Toplevel;
`has_focus()` = zusätzlich `window.is_active()`. Unter Xvfb wird das
Fenster erst **~21 ms nach `present()`** aktiv, gemessen:

```
t=0ms  active=false  b1.has_focus=false  b1.is_focus=true
window became active after 20.785483ms
now    row.has_focus=true   active=true
```

Die Tests treiben nur `while MainContext::iteration(false) {}` — ein
nicht-blockierender Drain, der nie 21 ms spinnt. Deterministisch, nicht
flaky.

- `sidebar_tests.rs:155` `acc_3_bottom_pinned_issues_collection_is_a_tab_stop`
  → `row.is_focus() || issues.is_focus()`
- `sidebar_tests.rs:252` `acc_3_focus_transfer_..._does_not_resync_mid_flight`
  → `missing.is_focus()`
- `sidebar_tests.rs:192` `focus_driven_selection_browses_without_routing...`
  → hier **bis `window.is_active()` pumpen**, nicht die Assertion
  umschreiben: dieser Test läuft durch echten Produktionscode
  (`sidebar_row_wiring.rs:52` gated auf `row.has_focus()`) und muss die
  echte Guard-Logik ausüben.

Vorbehalt zu `sidebar_tests.rs:252`: sobald der Fokus greift, ändern die
Folgeassertions ihre Bedeutung. Der Agent hat beide Zweige durchdacht
(das `unselect_all()` in `wire_row_selected_on` räumt so oder so), aber
**nicht ausgeführt** — als begründet, nicht als gemessen behandeln.

Optionale Härtung: `sidebar_row_wiring.rs:52` auf `is_focus()` umstellen.
`has_focus()` vermengt „diese Zeile ist Fokuswidget" mit „unser Toplevel
ist aktiv", was hier nicht gefragt ist. Für Nutzer verhaltensgleich.

### Vertauschte `is_ancestor`-Argumente (1 Test)

`crates/reprise-gnome/src/ui/shortcuts.rs:241`

```rust
.is_some_and(|focus| focus == *widget || widget.is_ancestor(&focus))
```

`a.is_ancestor(b)` heißt „*a* liegt in *b*". `GtkSearchEntry` delegiert
den Fokus an sein internes `GtkText`, das Fokuswidget ist also ein
**Kind** des Entry. Der Helfer fragt „liegt der Entry im GtkText" —
niemals wahr.

```
focus widget = GtkText
AS-WRITTEN  entry.is_ancestor(focus) = false
CORRECTED   focus.is_ancestor(entry) = true
```

Fix: `focus == *widget || focus.is_ancestor(widget)`. Eingeführt in
`3b2a3032`, seitdem nie grün. Betrifft
`ui::shortcuts::tests::search_2a_ctrl_f_reveals_and_focuses`.

### Fremde Desktop-Einstellung (1 Test)

`ui::window::window_decorations::tests::client_and_system_modes_project_to_every_window_control`

Scheitert an der **Konfiguration der Entwicklermaschine**:

```
org.gnome.desktop.wm.preferences button-layout = 'close,minimize:appmenu'
```

Close/Minimize liegen links, rechts steht nur `appmenu` (rendert nichts).
Der Test verlangt nicht-leere Controls auf der `End`-Seite. Gegenprobe
mit sauberem `XDG_CONFIG_HOME`: **grün**. Kein Codefehler — auf CI wäre
das nie aufgefallen.

Fix: seitenagnostisch prüfen, `!c.is_empty()` über alle `WindowControls`,
ohne `side()`-Bedingung. Die API-Zusicherungen
(`shows_start_title_buttons()` / `shows_end_title_buttons()`,
`window_decorations.rs:235-236`) sind bereits grün.

### Überspezifizierte Pixelerwartung (1 Test)

`ui::track_list::list_density::tests::list_density_changes_a_representative_track_table_row`

Erwartet `(10,8)`, gemessen `(8,8)`. `(10,8)` passt **weder** zu den
Tokens `(16,8)` noch zur Realität. Gemessene Zeilenhöhen 66/74/82;
Modell `row = max(min_height, inhaltsboden) + 46` bestätigt bei Standard
und Comfortable exakt, Compact liegt 8px darüber.

Ursache: ein intrinsischer Inhaltsboden von 20px (Rating-Sterne/Cover).
**`ROW_MIN_HEIGHT_COMPACT = 12` (`style/tokens.rs:37-43`) ist
unerreichbar** — Compact ist nur 8px enger als Standard statt der
gedachten 16.

Kein Nutzerfehler: drei sichtbar verschiedene Höhen entstehen. Aber das
Token behauptet etwas, das nie eintritt. Erwägen, es auf ~20 zu heben,
damit es die Wahrheit sagt.

Fix der Assertion — Invariante statt Magic Numbers:

```rust
assert!(compact < standard && standard < comfortable, "...");
assert_eq!(comfortable - standard,
    ROW_MIN_HEIGHT_COMFORTABLE - ROW_MIN_HEIGHT_STANDARD);
```

## P1 — LYR-4, Grenzwert (1)

`ui::lyrics::lyrics_view_tests::lyr_4_start_of_song_is_not_centered`
(`lyrics_view_tests.rs:343`)

`top offset was 20 (expected ~18)`, Assertion `|20-18| < 2.0` — verfehlt
die Grenze um exakt null. Die 2px stammen **nicht aus Projektcode**: für
`.lyrics-list` existiert kein CSS im Repo, es ist Adwaitas
Default-Zeilenpolsterung eines `ListBoxRow`.

Die drei übrigen Assertions desselben Tests belegen LYR-4 bereits:
`content_margin_top() == 18`, `scroll_values().0 == 0.0`,
`line_center_offset(0) < -20.0` (gemessen −71). Die vierte behauptet, die
Labelposition sei mit der Container-Margin **identisch**, und ignoriert
die intrinsische Row-Polsterung.

**Urteil: Test zu streng, Code korrekt.** Erwartung so fassen, dass sie
„Block sitzt oben" prüft und Theme-Polsterung zulässt — nicht die
Toleranz still aufweiten.

## Nebenbefunde (nicht angefragt, aber relevant)

**1. Vakuäre Assertions.** `window_decorations.rs:306-325`:

```rust
assert_eq!(headers.len(), 0);
headers.into_iter().all(|header| { ... })
```

Erst auf leer prüfen, dann über die leere Sammlung iterieren — die
`.all()`-Kette ist **immer wahr**. Vier Aufrufstellen (Zeilen 240-241,
263-264, 276-277) prüfen nichts. `build_mini()`
(`compact_player_layouts.rs:46`) baut nur `Image`/`Label`, nie eine
`adw::HeaderBar`. Entfernen oder echte Fixture geben.

**2. Zwei Mechanismen für eine Zusicherung.** `track_list_builder.rs:48-54`
setzt hartkodiert `PLAYER_BAR_HEIGHT = 86` als statische
`margin_bottom`, während `SafeInsetApplier` dieselbe Zusicherung dynamisch
fährt. Die Magic Number driftet bei Theme-/Font-Wechsel. Der Applier
sollte alleinige Quelle sein.

**3. `discover()` ist eine Einmal-Momentaufnahme.** `insets.rs:61-68`
läuft den Baum einmal bei Konstruktion ab. Später gebaute Scroller
(Lazy-Stack-Seiten, ausgetauschte Kinder) bekommen nie Insets. Der
Early-Return in `insets.rs:71-73` verschärft das: ändert sich die
Zielmenge bei gleichem Inset-Wert, bleiben neue Ziele ungepolstert.

**4. SQLite-Streu im Repo.** `crates/reprise-gnome/unused.db` und
`unused.db-journal` sind **versioniert** und werden von jedem Testlauf
verändert. Gehören in `.gitignore` und aus dem Index.

**5. Systematisches Audit empfohlen.** Der `has_focus()`-Fehler betrifft
strukturell den ganzen `#[ignore]`-Satz, nicht nur die drei gefundenen.
Alle `has_focus()`-Assertions im Display-Gate prüfen.

## Arbeitsreihenfolge

1. **Insets-Fix** (`insets.rs`) — einziger Produktionsdefekt. Danach alle
   4 zugehörigen Tests neu messen.
2. **Testkorrekturen** — 4× Fokus, 1× Fensterdekoration, 1× Dichte,
   1× LYR-4. Jede mit Begründung im Commit; keine Assertion still
   aufweichen.
3. **Nebenbefunde 1 und 4** — vakuäre Assertions, `unused.db`.
4. **Fail-Fast- und `--exact`-Lehre** in `RELEASING.md` verankern.

Nach jedem Schritt: voller Einzelprozess-Lauf über alle 209, nicht nur
die reparierten.
