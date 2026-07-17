# UX-Tooltips (Sektion L) — Taskplan

> **Für ausführende Agenten:** Tasks strikt in Reihenfolge 0 → 9, ein Commit pro
> Task, TDD wo ein Test benannt ist (rot → grün → Commit). Checkboxen (`- [ ]`)
> dienen dem Fortschritts-Tracking. Prozessregeln: `docs/ux-rules.md` (Kopf) und
> `AGENTS.md` zuerst lesen.

**Ziel:** Tooltip-Regeln (TIP-1a/1b, TIP-2a/2b, TIP-3/4/5) als Sektion L in
`docs/ux-rules.md`, das Traceability-Gate lernt die `[manuell]`-Ebene, und der
Bestand wird auditiert und auf Regelkonformität gebracht.

**Architektur:** Ein test-only Widget-Walk (`tooltip_discipline.rs`) erzwingt
TIP-1a mechanisch; Disabled-Grund-Texte werden als pure Funktionen extrahiert
und pur getestet (TIP-2a); die `[manuell]`-Regeln laufen über eine neue
RELEASING.md-Checkliste, deren Regel-IDs das erweiterte Gate beidseitig prüft.
Alle neuen/umbenannten Strings gehen durch die `N_!`-Kataloge + `de.po`
(check-release erzwingt 100 % Abdeckung).

**Beschlüsse (gegrillt 2026-07-17):**

1. Sektion **L** (K ist von zwei parallelen Branches doppelt beansprucht);
   Tooltip = Beschriftung, keine Feedback-Rolle — P-1 bleibt unangetastet.
2. Gate-Erweiterung um `[manuell]`: `[aktiv] [manuell]` gilt als gedeckt, wenn
   RELEASING.md die ID wörtlich nennt (Wortgrenze); Gegenrichtung wird wie beim
   Test-Lint geprüft (keine Referenz auf unbekannte/ersetzte IDs).
3. TIP-1-Lesart (b): Verb ist Pflicht, Objekt nur wo es disambiguiert.
   „Play"/„Shuffle"/„Repeat" bleiben; „Previous/Next/Queue/Information/Back"
   werden verbalisiert. Split TIP-1a `[gtk]` (Existenz) / TIP-1b `[manuell]`
   (Form). Scope: nur Icon-only-**Buttons** (keine Scales/Spinner/Labels).
4. TIP-2 mechanismus-agnostisch: Grund muss **benannt** sein — icon-only per
   ergänztem Tooltip (TIP-2a `[gtk]`), gelabelt per sichtbarem Text (TIP-2b
   `[manuell]`). Container-Klausel: ganze deaktivierte Container tragen EINE
   Aussage, nicht jedes Kind einzeln.
5. Shortcut-Ergänzung nur wo ein Accel registriert ist: einzig Play/Pause
   (`space`). Schreibweise wie Bestand: „Play (Space)".
6. Datei-Ownership: `crates/reprise-gnome/src/ui/tag_edit/**` gehört
   `feat/tag-editor-rework`, `crates/reprise-gnome/src/ui/browse/**` gehört
   `feat/global-search-rework` — **beide Verzeichnisse werden hier NICHT
   angefasst**; Funde dort sind Handoffs (siehe Task 9). Deshalb bleiben
   TIP-1b und TIP-2b vorerst `[geplant]` (Flip-Kriterien stehen als
   Kommentar in Sektion L).
7. TIP-3-Audit-Ergebnis: einzige exklusive Hover-Information ist die Sync-ETA
   („~2 min left", nur in `sync_tooltip`) → Fix in Task 7. TIP-4/TIP-5 sind
   bestandskonform (gio::Menu kann keine Item-Tooltips; die zwei
   `query-tooltip`-Handler — Waveform-Zeit, Ellipsis-Volltext — sind von
   TIP-5/TIP-1a gedeckt).

## Globale Constraints

- Gates vor JEDEM Commit: `cargo fmt --check` ·
  `cargo clippy --locked --all-targets --workspace -- -D warnings` ·
  `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace` ·
  `scripts/check-ux-traceability.sh` · `scripts/check-architecture.sh`.
- Display-Tests einzeln headless:
  `scripts/check-display-tests.sh` (sammelt alle `#[ignore = "requires a display; run via xvfb-run"]` automatisch ein). Für einen einzelnen Test das Muster aus dem Script verwenden.
- Übersetzung: jede neue/geänderte `msgid` bekommt im SELBEN Commit ihren
  `de.po`-Eintrag; `po/reprise.pot` mit dem xgettext-Kommando aus
  `scripts/check-release.sh` (Zeilen 23–27) regenerieren. Verifikation lokal:
  `msgfmt --check --check-format -o /tmp/reprise.mo po/de.po` und
  `msgcmp --use-fuzzy po/de.po po/reprise.pot` und
  `test -z "$(msgattrib --untranslated po/de.po)"`.
- Alle User-facing Strings über `N_!`-Kataloge (`strings.rs` bzw. das
  thematisch passende Schwester-Modul), nie inline.
- Statuswechsel `[geplant] → [aktiv]` NUR in dem Task-Commit, der es sagt
  (T3 → TIP-1a, T5 → TIP-2a, T8 → TIP-3/4/5). TIP-1b/TIP-2b bleiben
  `[geplant]`.
- Sperrliste (nicht anfassen): `crates/reprise-gnome/src/ui/tag_edit/**`,
  `crates/reprise-gnome/src/ui/browse/**`.
- Commits englisch, kein Attribution-Footer, **kein Push**.
- Dateien < 800 Zeilen; Funktionen klein; keine Mutation geteilter Zustände.

## Abhängigkeiten / Parallelisierungs-Karte

```text
T0 → T1 → { T2 ∥ T3 } → T4 → T5 → { T6 ∥ T7 } → T8 → T9
```

T2 (Gate) und T3 (TIP-1a) sind unabhängig; T5 braucht T4s Konstanten; T6/T7
berühren disjunkte Features, aber beide `de.po` — als Einzelagent sequenziell
arbeiten.

---

### Task 0: Plan committen

**Files:** Create (bereits im Worktree): dieser Plan +
`docs/superpowers/plans/2026-07-17-ux-tooltips-codex.md`

- [ ] **Commit:**

```bash
git add docs/superpowers/plans/2026-07-17-ux-tooltips-taskplan.md docs/superpowers/plans/2026-07-17-ux-tooltips-codex.md
git commit -m "docs: add tooltip rules plan and codex handoff (grilled 2026-07-17)"
```

### Task 1: Sektion L in `docs/ux-rules.md`

**Files:** Modify: `docs/ux-rules.md` (nach Sektion J, vor dem
Schluss-Absatz „Wenn beim Testen ein Fall auftaucht …")

- [ ] **Step 1: Sektion einfügen — Wortlaut exakt:**

```markdown
## L. Tooltips

<!-- Sektionsbuchstabe K ist bewusst übersprungen: feat/global-search-rework
     („K. Filter- & Such-Sichtbarkeit") und feat/tag-editor-rework
     („K. Tag-Editor") beanspruchen K parallel; die Kollision wird bei deren
     Merge aufgelöst. -->

Tooltips sind Beschriftung, kein Feedback-Mechanismus — sie tragen nie die
einzige Aussage (TIP-3) und fallen daher nicht unter P-1s Rollenmodell.
Wird ein ganzer Container deaktiviert, gilt TIP-2a/b für die
Container-Aussage, nicht für jedes Kind einzeln (die leere Player-Leiste
ist ihre eigene Aussage).

- **TIP-1a** [geplant] [gtk] — Existenz folgt der Beschriftung:
  Icon-only-Buttons haben immer einen Tooltip; Buttons mit sichtbarem
  Textlabel bekommen keinen — das Label ist die Aussage, ein
  wiederholender Tooltip ist Rauschen. Ausnahme: ellipsierte/abgeschnittene
  Labels zeigen im Tooltip den vollen Text.
- **TIP-1b** [geplant] [manuell] — Form: Verb + Objekt („Eject Pixel 8",
  „Toggle sidebar"); das Objekt darf entfallen, wenn der Button es selbst
  eindeutig macht („Play", „Shuffle"). Existiert ein Shortcut, steht er in
  Klammern dahinter („Play (Space)").
  <!-- Flip-Kriterium TIP-1b: „Previous"/„Next" im Tag-Editor
       (tag_editor_form.rs, Ownership feat/tag-editor-rework) und „Back" in
       browse_bar (Ownership feat/global-search-rework) sind noch
       Substantive. [aktiv] erst, wenn beide nachgezogen sind. -->
- **TIP-2a** [geplant] [gtk] — Disabled erklärt sich (icon-only): ein
  deaktiviertes Icon-only-Control behält seinen Tooltip und ergänzt den
  Grund („Eject device — Sync in progress"). Nie ein toter Button ohne
  benannten Grund (Konkretisierung von P-2).
- **TIP-2b** [geplant] [manuell] — Disabled erklärt sich (gelabelt): ein
  deaktiviertes gelabeltes Control nennt seinen Grund sichtbar per Label,
  Subtitle oder Hint-Zeile („Requires same artist & album across
  selection", „Everything in sync") — nie nur per Tooltip (TIP-3: der
  Grund wäre sonst exklusive Hover-Information).
  <!-- Flip-Kriterium TIP-2b: Save/„Change cover…" im Tag-Editor
       (feat/tag-editor-rework) und der deaktivierte „Add filter"-Zustand
       in browse_bar (feat/global-search-rework) sind noch unbegründet
       tot. [aktiv] erst, wenn beide nachgezogen sind. -->
- **TIP-3** [geplant] [manuell] — Tooltips sind redundant, nie exklusiv:
  jede Information in einem Tooltip muss auch ohne Hover erreichbar sein
  (View, Dialog, sichtbares Label). Hover-Details (Sync-Karte:
  „28 of 82 · ~2 min left") sind Komfort-Duplikate einer erreichbaren
  Ansicht — Touch-Bedienung sieht Tooltips nie.
- **TIP-4** [geplant] [manuell] — Menüeinträge bekommen keine Tooltips.
  In Popover-/Kontextmenüs trägt das Label allein; eine feste
  Subtitle-Zeile („M3U · PLS · XSPF") ist erlaubt. Braucht ein Menüpunkt
  einen Tooltip, ist er falsch benannt oder gehört in einen Dialog.
- **TIP-5** [geplant] [manuell] — GTK-Standardverhalten: keine
  Custom-Delays, keine interaktiven/Rich-Tooltips; dynamische Werte
  (Prozent, Zeit, ellipsierter Volltext) sind erlaubt.
```

- [ ] **Step 2:** `scripts/check-ux-traceability.sh` laufen lassen — grün
  (alle TIP-Regeln `[geplant]`, keine Tests nötig).
- [ ] **Step 3: Commit:**

```bash
git add docs/ux-rules.md
git commit -m "docs: add ux-rules section L (TIP tooltip rules, planned)"
```

### Task 2: Gate lernt `[manuell]`

**Files:** Modify: `scripts/check-ux-traceability.sh`

**Interfaces — Produces:** Gate-Verhalten, auf das T8 baut: eine
`[aktiv] [manuell]`-Regel ist gedeckt ⇔ RELEASING.md nennt ihre ID als Wort.

- [ ] **Step 1:** Nach dem `status_of`-Block das Ebenen-Parsing ergänzen:

```bash
# --- Read the document: ID -> level (core|gtk|e2e|manuell) ---
declare -A level_of
while read -r id lvl; do
  level_of[$id]=$lvl
done < <(grep -oE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(aktiv|geplant)\] \[(core|gtk|e2e|manuell)\]' "$doc" \
  | sed -E 's/^- \*\*([A-Z]+-[0-9]+[a-z]?)\*\* \[(aktiv|geplant)\] \[([a-z0-9]+)\]/\1 \3/')
```

- [ ] **Step 2:** Nach dem `kebab_refs`-Block die RELEASING-Referenzen
  einsammeln (Gegenrichtung wie beim Test-Lint):

```bash
# --- Collect checklist references (RELEASING.md, word-bounded IDs) ---
releasing=RELEASING.md
prefixes_upper=$(printf '%s' "$prefixes" | tr '[:lower:]' '[:upper:]')
declare -A in_releasing
while read -r id; do
  [[ -n $id ]] || continue
  in_releasing[$id]=1
  case "${status_of[$id]:-missing}" in
    missing) echo "ERROR: RELEASING.md references unknown rule $id" >&2; fail=1 ;;
    ersetzt) echo "ERROR: RELEASING.md references replaced rule $id — re-point it" >&2; fail=1 ;;
  esac
done < <(grep -hoE "\b(${prefixes_upper})-[0-9]+[a-z]?\b" "$releasing" 2>/dev/null | sort -u || true)
```

- [ ] **Step 3:** Richtung 1 ersetzen — `[manuell]` deckt über die
  Checkliste, alles andere weiter über Tests:

```bash
# --- Direction 1: every [aktiv] rule is covered on its level ---
for id in "${!status_of[@]}"; do
  [[ ${status_of[$id]} == aktiv ]] || continue
  if [[ ${level_of[$id]:-} == manuell ]]; then
    if [[ -z ${in_releasing[$id]:-} ]]; then
      echo "ERROR: [aktiv] [manuell] rule $id is not referenced in RELEASING.md" >&2; fail=1
    fi
  elif [[ -z ${tested[$id]:-} ]]; then
    echo "ERROR: [aktiv] rule $id has no rule-named test" >&2; fail=1
  fi
done
```

- [ ] **Step 4 — Negativproben (rot sehen, dann zurücksetzen):**
  1. `bash scripts/check-ux-traceability.sh` → grün (Ist-Zustand).
  2. In `docs/ux-rules.md` temporär `**TIP-4** [geplant]` → `[aktiv]` ändern,
     Lauf → erwartet `ERROR: [aktiv] [manuell] rule TIP-4 is not referenced in RELEASING.md`. Zurücksetzen.
  3. Temporär `TIP-99` als Wort in RELEASING.md einfügen, Lauf → erwartet
     `ERROR: RELEASING.md references unknown rule TIP-99`. Zurücksetzen.
  4. Falls `scripts/tests/qa-linters.sh` Patterns für dieses Script prüft:
     `bash scripts/tests/qa-linters.sh` → grün.
- [ ] **Step 5: Commit:**

```bash
git add scripts/check-ux-traceability.sh
git commit -m "ci: teach ux traceability gate the [manuell] level via RELEASING.md"
```

### Task 3: TIP-1a — Walk, Tests, Existenz-Fixes → `[aktiv]`

**Files:**
- Create: `crates/reprise-gnome/src/ui/tooltip_discipline.rs`
- Modify: `crates/reprise-gnome/src/ui/mod.rs` (mod-Eintrag),
  `crates/reprise-gnome/src/ui/strings.rs` (`PLAY_ALBUM`),
  `crates/reprise-gnome/src/ui/library_views/album_card.rs` (Overlay-Play),
  `crates/reprise-gnome/src/ui/compact/compact_player_layouts.rs` +
  `compact/compact_player.rs` (Play/Pause-Tooltip),
  `crates/reprise-gnome/src/ui/scan/scan_worker.rs` (Echo-Tooltip weg),
  `crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs` (Walk-Test),
  `crates/reprise-gnome/src/ui/window/library_chrome.rs` (Walk-Test),
  `po/de.po` + `po/reprise.pot`

**Interfaces — Produces:**
`pub(crate) fn tooltip_violations(root: &gtk4::Widget) -> Vec<String>`
(test-only; T5/T8 referenzieren nichts hiervon, aber künftige Walk-Tests
nutzen dieselbe fn).

- [ ] **Step 1: Walk-Helper anlegen** — vollständiger Inhalt von
  `tooltip_discipline.rs`:

```rust
//! Test-only widget walk asserting UX TIP-1a: icon-only buttons carry a
//! tooltip, visibly labeled buttons carry none (the label is the statement).
use gtk4::prelude::*;

pub(crate) fn tooltip_violations(root: &gtk4::Widget) -> Vec<String> {
    let mut violations = Vec::new();
    walk(root, &mut violations);
    violations
}

fn walk(widget: &gtk4::Widget, violations: &mut Vec<String>) {
    check(widget, violations);
    let mut child = widget.first_child();
    while let Some(next) = child {
        walk(&next, violations);
        child = next.next_sibling();
    }
}

fn check(widget: &gtk4::Widget, violations: &mut Vec<String>) {
    let (icon, label) = if let Some(button) = widget.downcast_ref::<gtk4::Button>() {
        (button.icon_name(), button.label())
    } else if let Some(menu_button) = widget.downcast_ref::<gtk4::MenuButton>() {
        (menu_button.icon_name(), menu_button.label())
    } else {
        return;
    };
    let tooltip = widget.tooltip_text();
    match (icon, label) {
        (Some(icon), None) if tooltip.as_deref().unwrap_or("").is_empty() => {
            violations.push(format!("icon-only button `{icon}` has no tooltip"));
        }
        (_, Some(label)) if tooltip.is_some() => {
            violations.push(format!("labeled button `{label}` carries a redundant tooltip"));
        }
        _ => {}
    }
}
```

  Eintrag in `ui/mod.rs` (alphabetisch einsortieren):

```rust
#[cfg(test)]
pub(crate) mod tooltip_discipline;
```

  Anmerkung: `ToggleButton` ist GTK-Subklasse von `Button` — der
  `downcast_ref::<gtk4::Button>` fängt ihn mit. Buttons mit Custom-Child
  (Album-Karten, `add_filter`-Pill) haben `icon_name() == None` und
  `label() == None` und werden übersprungen — gewollt.

- [ ] **Step 2: Failing Tests schreiben** (jeweils im `tests`-Mod der Datei,
  Muster: bestehende Display-Tests der Datei; falls keiner existiert, Fixture
  nach dem Muster von `player_bar_layout.rs::tests` anlegen):

  In `player_bar_layout.rs::tests` (Fixture `build()` existiert dort):

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tip_1a_player_bar_buttons_follow_tooltip_discipline() {
    if gtk4::init().is_err() {
        return;
    }
    let layout = build();
    let violations = crate::ui::tooltip_discipline::tooltip_violations(layout.root.upcast_ref());
    assert!(violations.is_empty(), "{violations:?}");
}
```

  Analog: `tip_1a_library_chrome_buttons_follow_tooltip_discipline`
  (in `library_chrome.rs::tests`, bestehende Fixture),
  `tip_1a_mini_player_buttons_follow_tooltip_discipline`
  (in `compact_player_layouts.rs` — die Bau-Funktion des Card-Layouts der
  Datei verwenden; die Datei enthält den Play/Pause-Button aus Zeile ~88),
  `tip_1a_album_card_play_overlay_has_tooltip`
  (in `album_card.rs` — die Karten-Bau-fn der Datei verwenden, die das
  Hover-Overlay mit `media-playback-start-symbolic` erzeugt; Assertion:
  `tooltip_violations` leer),
  `tip_1a_scan_button_keeps_label_only`
  (in `scan_worker.rs` — `ScanControls`-Fixture, dann):

```rust
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tip_1a_scan_button_keeps_label_only() {
    if gtk4::init().is_err() {
        return;
    }
    let controls = /* ScanControls-Test-Fixture der Datei */;
    begin_scan_ui(&controls);
    assert_eq!(
        controls.button.label().as_deref(),
        Some(strings::text(strings::SCANNING).as_str())
    );
    assert!(controls.button.tooltip_text().is_none());
}
```

- [ ] **Step 3: Rot sehen.** Erwartete Fehlschläge: mini-player (Play/Pause
  ohne Tooltip), album-card (Overlay-Play ohne Tooltip), scan (Echo-Tooltip
  vorhanden). player_bar/library_chrome sind Absicherungs-Walks und dürfen
  sofort grün sein.
- [ ] **Step 4: Fixes.**
  1. `strings.rs` (bei den Player-Strings, ~Z. 396 ff.):

```rust
/// Tooltip of the album-card hover-overlay play button (TIP-1a).
pub const PLAY_ALBUM: &str = N_!("Play album");
```

  2. `album_card.rs` nach dem Builder des Overlay-Play-Buttons (~Z. 111):

```rust
play_button.set_tooltip_text(Some(&strings::text(strings::PLAY_ALBUM)));
```

  3. `compact_player_layouts.rs` nach dem Bau des `play_pause_button`
     (~Z. 88): `play_pause_button.set_tooltip_text(Some(&strings::text(strings::PLAY)));`
     und in `compact_player.rs::set_state` (~Z. 151) den Tooltip mit dem
     Icon umschalten (`PAUSE` wenn `is_playing`, sonst `PLAY`).
  4. `scan_worker.rs::begin_scan_ui`: die beiden Zeilen
     `controls.button.set_tooltip_text(Some(&strings::text(strings::SCANNING)));`
     ersatzlos streichen; in `finish_scan_ui` die Zeile
     `controls.button.set_tooltip_text(None);` ebenfalls (es wird nie mehr
     einer gesetzt).
  5. `de.po` + `po/reprise.pot`: `msgid "Play album"` /
     `msgstr "Album wiedergeben"`.
- [ ] **Step 5: Grün sehen** (Display-Tests einzeln headless), dann alle
  Gates.
- [ ] **Step 6:** In `docs/ux-rules.md`: `**TIP-1a** [geplant]` →
  `**TIP-1a** [aktiv]`.
- [ ] **Step 7: Commit:**

```bash
git add -A
git commit -m "feat: enforce tooltip existence discipline on icon-only buttons (TIP-1a)"
```

### Task 4: TIP-1b-Vorarbeit — Verbalisierung + Shortcut

**Files:**
- Modify: `crates/reprise-gnome/src/ui/strings.rs`,
  `strings_news.rs`, `player_bar/player_bar_layout.rs`,
  `player_bar/player_bar.rs` (Z. 354–359),
  `compact/compact_player_layouts.rs` + `compact/compact_player.rs`
  (Tooltips aus T3 auf neue Konstanten), `info_panel/info_panel.rs`
  (Z. 241), `po/de.po` + `po/reprise.pot`

**Interfaces — Produces** (T5 konsumiert diese Namen wörtlich):
`strings::TOOLTIP_PLAY`, `TOOLTIP_PAUSE`, `TOOLTIP_PREVIOUS`,
`TOOLTIP_NEXT`, `TOOLTIP_QUEUE`.

- [ ] **Step 1: Konstanten.** In `strings.rs` die bestehende
  `pub const QUEUE: &str = N_!("Queue");` (Z. 404, einziger Nutzer ist der
  Queue-Button) **umbenennen und umtexten** zu
  `pub const TOOLTIP_QUEUE: &str = N_!("Show queue");` und daneben anlegen:

```rust
/// Transport tooltips (TIP-1b): verb + object, shortcut in parentheses.
/// PLAY/PAUSE/PREVIOUS/NEXT above stay as menu labels (compact player menu).
pub const TOOLTIP_PLAY: &str = N_!("Play (Space)");
pub const TOOLTIP_PAUSE: &str = N_!("Pause (Space)");
pub const TOOLTIP_PREVIOUS: &str = N_!("Play previous track");
pub const TOOLTIP_NEXT: &str = N_!("Play next track");
```

  In `strings_news.rs` (neben `INFORMATION`, das Panel-Titel bleibt):

```rust
/// Tooltip of the headerbar info-panel toggle (TIP-1b).
pub const INFO_PANEL_TOGGLE: &str = N_!("Toggle information panel");
```

  **Wichtig:** `PLAY`/`PAUSE`/`PREVIOUS`/`NEXT` NICHT umtexten — sie sind
  Menü-Labels in `compact_player_menu.rs` und (PREVIOUS/NEXT) Tooltips im
  gesperrten `tag_edit/`.

- [ ] **Step 2: Umverdrahten.**
  - `player_bar_layout.rs`: Z. 113 `strings::TOOLTIP_PREVIOUS`, Z. 116
    `strings::TOOLTIP_PLAY`, Z. 120 `strings::TOOLTIP_NEXT`, Z. 211
    `strings::TOOLTIP_QUEUE`. Shuffle/Repeat/Volume/Main menu bleiben.
  - `player_bar.rs` Z. 354–359: `TOOLTIP_PAUSE`/`TOOLTIP_PLAY`.
  - `compact_player_layouts.rs`/`compact_player.rs`: die T3-Tooltips auf
    `TOOLTIP_PLAY`/`TOOLTIP_PAUSE` umstellen.
  - `info_panel.rs` Z. 241: `news_strings::INFO_PANEL_TOGGLE` (bzw. der in
    der Datei übliche Importpfad des news-Strings-Moduls).
- [ ] **Step 3: `de.po` + `po/reprise.pot`** — neue Einträge (bestehende
  msgids „Play"/„Pause"/„Previous"/„Next"/„Queue"/„Information" NICHT
  löschen, sie haben weitere Nutzer):

```po
msgid "Play (Space)"
msgstr "Wiedergeben (Leertaste)"

msgid "Pause (Space)"
msgstr "Pausieren (Leertaste)"

msgid "Play previous track"
msgstr "Vorherigen Titel wiedergeben"

msgid "Play next track"
msgstr "Nächsten Titel wiedergeben"

msgid "Show queue"
msgstr "Warteschlange anzeigen"

msgid "Toggle information panel"
msgstr "Informationsbereich ein-/ausblenden"
```

  (Anredeform/Terminologie an bestehende `de.po`-Einträge angleichen —
  „Toggle sidebar" ist dort „Seitenleiste ein-/ausblenden".)
- [ ] **Step 4:** Betroffene bestehende Test-Assertions (Suche nach den
  alten Tooltip-Strings in `crates/`) auf die neuen Konstanten umstellen.
  Gates + Display-Tests + msgfmt/msgcmp. **Kein Statusflip** (TIP-1b bleibt
  `[geplant]`).
- [ ] **Step 5: Commit:**

```bash
git add -A
git commit -m "feat: verbalize transport and panel tooltips with shortcuts (TIP-1b prep)"
```

### Task 5: TIP-2a — Disabled-Gründe icon-only → `[aktiv]`

**Files:**
- Modify: `crates/reprise-gnome/src/ui/strings.rs`,
  `device_sync/device_sync_strings.rs`, `device_view/device_view.rs`
  (Z. 160–168), `preferences/preference_sync_planned.rs` (Z. 34–38),
  `player_bar/player_bar.rs` (`set_transport_enabled`), `po/de.po` +
  `po/reprise.pot`

**Interfaces — Consumes:** `strings::TOOLTIP_PREVIOUS`/`TOOLTIP_NEXT` (T4).
**Produces:** `device_sync_strings::eject_tooltip(syncing: bool) -> String`,
`player_bar::transport_tooltips(enabled: bool) -> (&'static str, &'static str)`.

- [ ] **Step 1: Failing Tests (pure, kein Display):**

  In `device_sync_strings.rs::tests`:

```rust
#[test]
fn tip_2a_eject_tooltip_names_reason_while_syncing() {
    assert_eq!(eject_tooltip(true), "Eject device — Sync in progress");
    assert_eq!(eject_tooltip(false), "Eject device");
}
```

  In `player_bar.rs::tests` (bzw. `tests`-Mod anlegen):

```rust
#[test]
fn tip_2a_transport_tooltips_name_reason_when_nothing_queued() {
    let (prev, next) = transport_tooltips(false);
    assert_eq!(prev, strings::TOOLTIP_PREVIOUS_UNAVAILABLE);
    assert_eq!(next, strings::TOOLTIP_NEXT_UNAVAILABLE);
    let (prev, next) = transport_tooltips(true);
    assert_eq!(prev, strings::TOOLTIP_PREVIOUS);
    assert_eq!(next, strings::TOOLTIP_NEXT);
}
```

- [ ] **Step 2: Rot sehen** (`cargo test -p reprise-gnome tip_2a` —
  Compile-Fehler zählt als rot).
- [ ] **Step 3: Implementieren.**

  `device_sync_strings.rs`:

```rust
pub const EJECT_DEVICE: &str = N_!("Eject device");
pub const EJECT_BLOCKED_SYNCING: &str = N_!("Eject device — Sync in progress");

/// TIP-2a: a disabled eject keeps its tooltip and appends the reason.
pub fn eject_tooltip(syncing: bool) -> String {
    text(if syncing { EJECT_BLOCKED_SYNCING } else { EJECT_DEVICE })
}
```

  `device_view.rs` Z. 160–168: das Inline-`if syncing {…}` im
  `.tooltip_text(…)` durch `device_sync_strings::eject_tooltip(syncing)`
  ersetzen. `preference_sync_planned.rs` Z. 34–38: statisches
  `"Eject device"`-Literal durch denselben Aufruf mit der dortigen
  Syncing/Finishing-Bedingung ersetzen (dieselbe Variable, die dort
  `set_sensitive` steuert).

  `strings.rs`:

```rust
pub const TOOLTIP_PREVIOUS_UNAVAILABLE: &str = N_!("Play previous track — nothing queued");
pub const TOOLTIP_NEXT_UNAVAILABLE: &str = N_!("Play next track — nothing queued");
```

  `player_bar.rs`:

```rust
fn transport_tooltips(enabled: bool) -> (&'static str, &'static str) {
    if enabled {
        (strings::TOOLTIP_PREVIOUS, strings::TOOLTIP_NEXT)
    } else {
        (
            strings::TOOLTIP_PREVIOUS_UNAVAILABLE,
            strings::TOOLTIP_NEXT_UNAVAILABLE,
        )
    }
}
```

  und in `set_transport_enabled` nach den beiden `set_sensitive`-Zeilen:

```rust
let (prev_tip, next_tip) = transport_tooltips(enabled);
self.prev_button
    .set_tooltip_text(Some(&strings::text(prev_tip)));
self.next_button
    .set_tooltip_text(Some(&strings::text(next_tip)));
```

- [ ] **Step 4: `de.po` + `po/reprise.pot`:**

```po
msgid "Eject device"
msgstr "Gerät auswerfen"

msgid "Eject device — Sync in progress"
msgstr "Gerät auswerfen — Synchronisierung läuft"

msgid "Play previous track — nothing queued"
msgstr "Vorherigen Titel wiedergeben — nichts in der Warteschlange"

msgid "Play next track — nothing queued"
msgstr "Nächsten Titel wiedergeben — nichts in der Warteschlange"
```

- [ ] **Step 5: Grün + Gates.** Der TIP-1a-Walk aus T3 bleibt grün (Tooltips
  weiter vorhanden, nur Text wechselt).
- [ ] **Step 6:** `**TIP-2a** [geplant]` → `[aktiv]` in `docs/ux-rules.md`.
- [ ] **Step 7: Commit:**

```bash
git add -A
git commit -m "feat: disabled icon-only controls name their reason (TIP-2a)"
```

### Task 6: TIP-2b-Vorarbeit — Disabled-Gründe in den Preferences

**Files:**
- Modify: `crates/reprise-gnome/src/ui/strings_scrobbling.rs`,
  `strings_rhythmbox.rs`, `preferences/preference_listenbrainz.rs`,
  `preferences/preference_lastfm.rs`, `preferences/preference_rhythmbox.rs`,
  `po/de.po` + `po/reprise.pot`

- [ ] **Step 1: Konstanten.** `strings_scrobbling.rs`:

```rust
/// TIP-2b: reason shown while the connect button is disabled.
pub const CONNECT_REQUIRES_TOKEN: &str = N_!("Requires your ListenBrainz user token");
pub const BROWSER_REQUIRES_CREDENTIALS: &str = N_!("Requires API key and shared secret");
```

  `strings_rhythmbox.rs`:

```rust
/// TIP-2b: prescan failure keeps the import button disabled — say why.
pub const PRESCAN_FAILED: &str = N_!("Could not read the Rhythmbox library — import stays disabled");
```

- [ ] **Step 2: ListenBrainz** (`preference_listenbrainz.rs`): direkt nach
  dem Bau der `connect_row` (Z. ~121) initial
  `connect_row.set_subtitle(&strings::text(strings::CONNECT_REQUIRES_TOKEN));`
  und den bestehenden `token.connect_changed`-Handler (Z. ~148) erweitern:

```rust
move |token| {
    let has_token = !token.text().trim().is_empty();
    connect.set_sensitive(has_token);
    connect_row.set_subtitle(if has_token {
        ""
    } else {
        &strings::text(strings::CONNECT_REQUIRES_TOKEN)
    });
}
```

  (Borrow-Anpassung nach Compiler; `connect_row` mit in die Closure klonen.)
- [ ] **Step 3: Last.fm** (`preference_lastfm.rs`): gleiche Mechanik auf
  `browser_row` mit `BROWSER_REQUIRES_CREDENTIALS`, im bestehenden
  `entry.connect_changed`-Handler (Z. ~192–197), der `open_browser`
  schaltet.
- [ ] **Step 4: Pending-Zustand** (`set_activation_pending` in BEIDEN
  Scrobbling-Dateien): während `pending` die Row-Subtitle auf den
  bestehenden Connecting-Text setzen (`LISTENBRAINZ_CONNECTING` bzw. das
  Last.fm-Pendant; Konstantennamen in `preference_dependencies.rs:103`
  nachschlagen). Vorherige Subtitle vor dem Überschreiben lesen und bei
  `pending == false` wiederherstellen; läuft nach Abschluss ohnehin ein
  Row-Refresh, genügt das Setzen im pending-Zweig — kurz prüfen, was die
  Aufrufer nach Abschluss tun, und die einfachere korrekte Variante nehmen.
- [ ] **Step 5: Rhythmbox** (`preference_rhythmbox.rs`): über dem
  Seiten-`stack` (Dialog-Aufbau, Z. ~325–335) ein
  `adw::Banner`(`set_revealed(false)`) einfügen; in beiden Fehlerzweigen des
  Prescan (`Ok(Err(e))` / `Err(_)`, Z. ~438–450) zusätzlich zu
  `tracing::warn!`:

```rust
error_banner.set_title(&strings::text(strings::PRESCAN_FAILED));
error_banner.set_revealed(true);
```

- [ ] **Step 6: `de.po` + `po/reprise.pot`:**

```po
msgid "Requires your ListenBrainz user token"
msgstr "Erfordert den ListenBrainz-Benutzertoken"

msgid "Requires API key and shared secret"
msgstr "Erfordert API-Schlüssel und Shared Secret"

msgid "Could not read the Rhythmbox library — import stays disabled"
msgstr "Rhythmbox-Bibliothek konnte nicht gelesen werden — der Import bleibt deaktiviert"
```

- [ ] **Step 7:** Gates. **Kein Statusflip** (TIP-2b bleibt `[geplant]`,
  Flip-Kriterium steht in Sektion L). Bereits konforme Stellen (Scan-Label,
  MB-Hint, Dependencies-Subtitle, „Sync ratings"-Subtitle, Sync-now via
  sichtbarer „Everything in sync ✓"-Anzeige) brauchen nichts.
- [ ] **Step 8: Commit:**

```bash
git add -A
git commit -m "feat: disabled labeled preference controls name their reason (TIP-2b prep)"
```

### Task 7: Tooltip-Literale katalogisieren + Sync-ETA sichtbar (TIP-3-Fix)

**Files:**
- Modify: `crates/reprise-gnome/src/ui/strings.rs`,
  `device_sync/device_sync_strings.rs`, `device_sync/device_sync_feedback.rs`,
  `device_view/device_view.rs`, `compact/compact_player_layouts.rs`,
  `compact/compact_mode_controls.rs` (Test-Assertions),
  `scan/scan_progress.rs`/`scan_controls.rs` (nur falls Import nötig),
  `po/de.po` + `po/reprise.pot`

- [ ] **Step 1: ETA sichtbar machen (der eigentliche TIP-3-Fix).**
  `device_sync_strings.rs`: `fn remaining_hint` → `pub fn remaining_hint`.
  `device_view.rs::phase_copy`, Syncing-Arm: Subtitle von `current_track`
  auf „Track · ETA" erweitern:

```rust
PlannedSyncPhase::Syncing {
    done,
    total,
    current_track,
    bytes_done,
    bytes_total,
    ..
} => {
    let mut subtitle = current_track.clone();
    if let Some(eta) = device_sync_strings::remaining_hint(*bytes_done, *bytes_total) {
        subtitle = if subtitle.is_empty() {
            eta
        } else {
            format!("{subtitle} · {eta}")
        };
    }
    (
        format!("Synchronizing {done} of {total}"),
        subtitle,
        if *bytes_total == 0 {
            0.0
        } else {
            *bytes_done as f64 / *bytes_total as f64
        },
    )
}
```

  Pure Test in `device_view.rs::tests` (Fixture nach Muster der
  `storage_summary`-Tests, `bytes_total`/`bytes_done` so wählen, dass
  \>60 s Rest entstehen — **bewusst OHNE `tip_`-Präfix**, TIP-3 bleibt
  `[manuell]`):

```rust
#[test]
fn phase_copy_subtitle_names_eta_during_sync() {
    // DeviceView with PlannedSyncPhase::Syncing { done: 28, total: 82,
    // bytes_done: small, bytes_total: large, current_track: "Immortal" }
    let (_, subtitle, _) = phase_copy(&device);
    assert!(subtitle.contains("min left"), "{subtitle}");
    assert!(subtitle.contains("Immortal"), "{subtitle}");
}
```

- [ ] **Step 2: Literale in Kataloge.**
  - `strings.rs`:

```rust
pub const TOOLTIP_RESTORE_FULL_WINDOW: &str = N_!("Restore full window (Ctrl+M)");
pub const TOOLTIP_CLOSE_MINI_PLAYER: &str = N_!("Close mini-player");
```

    `compact_player_layouts.rs` Z. 118/125 auf die Konstanten umstellen;
    Assertions in `compact_mode_controls.rs`-Tests, die die Literale
    prüfen, auf die Konstanten umstellen.
  - `device_sync_strings.rs`:

```rust
pub const KEPT_ON_DEVICE: &str = N_!("Kept on device");
pub const STORAGE_TOTALS_UNKNOWN: &str =
    N_!("GVfs did not report total capacity; the bar shows known music and free space.");

/// Sidebar spinner tooltip while syncing, e.g. "Syncing Pixel 8 · 42%".
pub fn syncing_spinner_tooltip(name: &str, percent: u8) -> String {
    formatted(
        N_!("Syncing {name} · {percent}%"),
        &[("name", name), ("percent", &percent.to_string())],
    )
}
```

    `device_view.rs` Z. 196/311 auf die Konstanten umstellen; das lokale
    `sync_tooltip` in `device_sync_feedback.rs` (Z. ~138) durch
    `syncing_spinner_tooltip` ersetzen (Prozentrechnung dort belassen).
  - `strings.rs`-Scan-Tooltips gettext-fähig machen: `scan_card_tooltip`
    (Z. ~321) und `scan_tooltip_progress` (Z. ~358) von nacktem `format!`
    auf `formatted(N_!("…"), …)` umstellen (msgids:
    `"Covers & lyrics: {count} queued"`, `"Scanning · {percent}%"` — an die
    exakten heutigen Ausgabetexte halten, nur übersetzbar machen).
- [ ] **Step 3: `de.po` + `po/reprise.pot`:**

```po
msgid "Restore full window (Ctrl+M)"
msgstr "Vollständiges Fenster wiederherstellen (Strg+M)"

msgid "Close mini-player"
msgstr "Mini-Player schließen"

msgid "Kept on device"
msgstr "Auf dem Gerät behalten"

msgid "GVfs did not report total capacity; the bar shows known music and free space."
msgstr "GVfs hat keine Gesamtkapazität gemeldet; der Balken zeigt bekannte Musik und freien Speicher."

msgid "Syncing {name} · {percent}%"
msgstr "Synchronisiere {name} · {percent} %"

msgid "Covers & lyrics: {count} queued"
msgstr "Cover & Songtexte: {count} in der Warteschlange"

msgid "Scanning · {percent}%"
msgstr "Scanne · {percent} %"
```

  (Platzhalter-msgids exakt an die tatsächlichen Format-Strings der
  jeweiligen Funktion angleichen.)
- [ ] **Step 4:** Rot→grün für den neuen pure Test, dann Gates +
  Display-Tests + msgfmt/msgcmp. Kein Statusflip (TIP-3 flippt in T8).
- [ ] **Step 5: Commit:**

```bash
git add -A
git commit -m "feat: catalog tooltip literals and surface sync ETA without hover (TIP-3)"
```

### Task 8: RELEASING-Checkliste + Flips TIP-3/4/5

**Files:** Modify: `RELEASING.md` (Sektion „Manual GNOME QA"),
`docs/ux-rules.md`

- [ ] **Step 1:** Neuen Bullet in „Manual GNOME QA" (vor dem
  Abschluss-Absatz „Record the OS …"):

```markdown
- Tooltip discipline (UX TIP-1b, TIP-2b, TIP-3, TIP-4, TIP-5): hover every
  icon-only button in both window modes — wording is verb + object, with the
  shortcut in parentheses where one exists (TIP-1b). Every disabled control
  names its reason: visibly for labeled controls, in the tooltip for
  icon-only ones (TIP-2b). Information shown in a tooltip must also be
  reachable without hovering — for the sync card check count, size, and ETA
  in the device view (TIP-3). No menu item shows a tooltip (TIP-4). Tooltips
  use stock GTK behavior: no custom delays, no rich content; dynamic values
  are fine (TIP-5).
```

- [ ] **Step 2:** Im SELBEN Commit in `docs/ux-rules.md`:
  `**TIP-3**`, `**TIP-4**`, `**TIP-5**` jeweils `[geplant]` → `[aktiv]`.
  (TIP-1b/TIP-2b bleiben `[geplant]` — ihre Nennung in der Checkliste ist
  erlaubt, das Gate verlangt Nennung nur für `[aktiv] [manuell]`.)
- [ ] **Step 3:** `bash scripts/check-ux-traceability.sh` → grün — das ist
  der Nachweis, dass die T2-Erweiterung die drei Flips über die Checkliste
  deckt.
- [ ] **Step 4: Commit:**

```bash
git add RELEASING.md docs/ux-rules.md
git commit -m "docs: add tooltip checklist to RELEASING and activate TIP-3/4/5"
```

### Task 9: Abschluss — volle Gate-Batterie, Ledger, Handoffs

- [ ] **Step 1:** Volle Batterie: alle Gates aus den Globalen Constraints +
  `scripts/check-display-tests.sh` (komplett) + die drei
  msgfmt/msgcmp/msgattrib-Kommandos.
- [ ] **Step 2:** `.superpowers/sdd/progress.md`: Eintrag mit Datum
  2026-07-17, Branch `feat/ux-rules-tooltips`, Kurzfassung: „Sektion L (TIP)
  angelegt; TIP-1a/2a/3/4/5 aktiv; TIP-1b/2b geplant mit Flip-Kriterien
  (Ownership tag_edit/browse); Gate um [manuell]-Ebene erweitert."
- [ ] **Step 3:** Abschlussbericht (Chat/Handoff) MUSS die offenen Übergaben
  auflisten, damit sie nicht verloren gehen:
  - **an feat/tag-editor-rework:** Prev/Next-Tooltips verbalisieren
    (TIP-1b), Save-ohne-Grund und dauerhaft totes „Change cover…" (TIP-2b —
    HIG: dauerhaft Unverfügbares ausblenden statt deaktivieren).
  - **an feat/global-search-rework:** `BACK` („Back" → „Go back", TIP-1b);
    deaktivierter „Add filter" ohne Grund (TIP-2b — Vorschlag: nicht
    deaktivieren, das Popover zeigt bereits „No filters available").
  - **Funde außerhalb des Tooltip-Scopes** (nur melden, nicht fixen):
    `scan_complete_toast`/`fetch_progress` in `strings.rs` umgehen gettext;
    `device_view.rs`-Labels („Sync now", „Cancel", „Sync settings…",
    „Device full", Storage-Legende) sind unkatalogisierte Literale.
- [ ] **Step 4: Commit:**

```bash
git add .superpowers/sdd/progress.md
git commit -m "docs: record tooltip audit completion in progress ledger"
```

---

## Selbstreview-Notizen (beim Schreiben geprüft)

- Spec-Deckung: TIP-1a→T3, TIP-1b→T4+Flip-Kriterium, TIP-2a→T5,
  TIP-2b→T6+Flip-Kriterium, TIP-3→T7+T8, TIP-4/5→T8 (bestandskonform,
  Audit belegt), Gate→T2, Inventar/Audit→Beschlussteil + Tasks.
- Typkonsistenz: `TOOLTIP_*`-Namen sind zwischen T4 (Produzent) und T5
  (Konsument) identisch; `eject_tooltip`/`transport_tooltips`/
  `remaining_hint`-Signaturen jeweils an Definition und Aufruf gleich.
- Bewusste Rest-Unschärfen (Codex passt minimal an, Verträge bleiben):
  Test-Fixture-Konstruktionen (`ScanControls`, Album-Karte, Mini-Layout,
  `DeviceView`-Struct) folgen den je vorhandenen tests-Mods; Borrow/Clone
  in GTK-Closures nach Compiler.
