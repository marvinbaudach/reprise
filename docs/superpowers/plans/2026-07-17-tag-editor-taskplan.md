# Tag-Editor-Rework — Taskplan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development
> (empfohlen) oder superpowers:executing-plans. Checkboxen (`- [ ]`) fürs Tracking.
> Normativer Kontext: [2026-07-17-tag-editor-beschluesse.md](2026-07-17-tag-editor-beschluesse.md)
> — bei Widerspruch gewinnt das Beschlussdokument.

**Ziel:** Tag-Editor nach Designs 3a/4a: navigationsneutraler Save (TAG-1, zentral in
`reload()`), 3a-Layout, direkte Mixed-Felder, ‹›-Navigation mit per-Track-Pending,
Änderungs-Review, Ghost-Autocomplete, TAG-8-Tastatur, Save-Progress im offenen Dialog,
FB-3-Fehlerpfad.

**Architektur:** Pending-Session-Modell als pure Rust in `reprise-core`
(`library/tag_edit_session.rs`); die GUI bindet nur. Per-Track-Writes mit
Progress-Callback im Kern (`tag_edit.rs`), Watcher-Ignore pro Datei unmittelbar vor
dem Write. Autocomplete-Ranking (Präfix vor Substring) und Ghost-Query im Kern
(`queries/autocomplete.rs`). UI-Schicht: Form-Umbau auf GtkEntry-Grid,
Autocomplete-Popover-Erweiterung + Ghost-Overlay, Tastatur-/Esc-Kaskade,
Review-Footer, Save-Progress, Fehler-Dialog, Listen-Snapshot + ‹›.

## Globale Constraints

- Gates vor JEDEM Commit: `cargo fmt --check` · `cargo clippy --all-targets
  --workspace -- -D warnings` · `cargo test --workspace` · `cargo audit`
  (akzeptiert nur RUSTSEC-2024-0436).
- reprise-core bleibt dependency-pur (kein gtk4/gstreamer/zbus):
  `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` nach Kern-Änderungen.
- Dateien < 800 Zeilen; Sibling-Module extrahieren statt Doku kürzen.
- RefCell-Disziplin: nie ein `borrow()` über einen GTK-/Callback-Aufruf halten
  (Skill building-gtk4-rust-apps).
- Alle sichtbare Copy via `strings.rs`-`N_!`-Konstanten. `strings.rs` ist
  **append-only** (Kollisionsvermeidung zwischen Paketen).
- **Regelwerk:** TAG-1–8 stehen in `docs/ux-rules.md` Abschnitt K (`[geplant]`).
  Statusflip `[geplant] → [aktiv]` passiert **im Regelwerk, im selben Commit**, der
  das Verhalten implementiert — und nur, wenn ein regelbenannter Test grün ist.
  Halb umgesetzte Regeln werden in Teilregeln gesplittet (a/b) statt halb geflippt.
  `docs/ux-rules.md` ist geteilte Datei: **nur die eigene TAG-Zeile** anfassen.
- **Testnamen (Gate-Format!):** genau eine primäre Regel-ID pro Test, Rust
  `fn tag_1_…`, cua-e2e `tag-1-…` — Unterstrich zwischen Prefix und Nummer, sonst
  zählt `scripts/check-ux-traceability.sh` den Test nicht. Nicht-Regel-Tests tragen
  **keine** ID im Namen. `#[ignore]` nur auf `[geplant]`-Regeln, Format
  `#[ignore = "UX TAG-4 [geplant] — …"]`.
- Gate zusätzlich vor jedem Commit: `./scripts/check-ux-traceability.sh`.
- Ein Commit pro Task, englische Message, kein Attribution-Footer, kein Push.
- TDD: erst rote Tests, dann Implementierung.

## Parallel-Schnitt (Datei-Ownership)

| Welle | Paket | Ownership (exklusiv) |
|---|---|---|
| 1 | **A** Reload-Neutralität | `ui/track_list/track_list_reload.rs`, neu `ui/track_list/reload_restore.rs`, `ui/track_list/mod.rs` (nur eigene Zeile) |
| 1 | **B** Kern | `reprise-core/src/library/tag_edit.rs`, neu `tag_edit_session.rs`, `queries/autocomplete.rs`, `library/watcher.rs` |
| 2 | **C** Form-Layout | `ui/tag_edit/tag_editor_form.rs`, `tag_editor_widgets.rs`, `tag_editor_style.rs` |
| 2 | **D** Autocomplete-UI | `ui/tag_edit/autocomplete_entry.rs` |
| 3 | **F** Session-Integration + Review + Save-Flow (**allein**) | `ui/tag_edit/tag_editor.rs`, `tag_editor_dirty.rs`, `tag_editor_form.rs`, `tag_editor_save.rs` (nur Signatur), `tag_edit_flow.rs`, neu `tag_editor_failures.rs`, `ui/toasts.rs` |
| 4 | **E** Tastatur | `ui/tag_edit/tag_editor_save.rs` (Innereien), `tag_editor_state.rs` |
| 4 | **G** Navigation + MB | `ui/tag_edit/tag_editor.rs`, `tag_editor_lookup.rs`, `tag_edit_flow.rs`, `tag_editor_form.rs` (Subzeile) |
| 5 | **H** Abnahme | CUA-Szenarien, `docs/ux-rules.md` (TAG-Zeilen), Beschluss-Ledger |

Pakete innerhalb einer Welle laufen parallel (disjunkte Dateien).
`strings.rs` und `tag_edit/mod.rs` sind geteilt: nur append/eigene Zeilen.

**Korrektur nach Welle 2 (2026-07-17):** E und F waren als parallel geplant — das
geht nicht. `tag_editor.rs` ist der einzige Verdrahtungspunkt (ruft `dirty::wire`,
`lookup::wire`, `save::wire`), und `save::wire` nimmt heute das Dirty-Array
`&[Rc<Cell<bool>>]` entgegen: genau den Zustand, den F auf `TagEditSession` umbaut.
Dazu braucht Es Esc-Kaskade (Stufe 2 = Feld-Revert) die Revert-API, die erst F
schafft. F läuft deshalb allein und stabilisiert die Signaturen; E und G bauen
danach parallel darauf auf (disjunkte Dateien: E = Innereien von `save.rs`,
G = `tag_editor.rs`/`lookup.rs`).

---

## Paket A — TAG-1: Reload wird navigationsneutral (Welle 1)

### Task A1: Restore-Helfer (pure Logik)
**Files:** Create `ui/track_list/reload_restore.rs` (+ `mod.rs`-Zeile)
**Interfaces:** `pub(in crate::ui) struct ReloadAnchor { selected_ids: Vec<i64>, anchor: Option<(i64, f64)> /* Track-ID + Offset zur Viewport-Oberkante */ }`; `capture(...) -> ReloadAnchor`; `positions_for_ids(ids, current) -> Vec<u32>`; `scroll_target(anchor, current_ids, row_height, viewport) -> Option<f64>`.
- [ ] Rote Tests: `tag_1_positions_for_ids_maps_surviving_ids_only`, `tag_1_scroll_target_follows_anchor_row_after_resort`, `tag_1_scroll_target_none_when_anchor_gone`, `tag_1_deleted_ids_drop_silently`.
- [ ] Grün implementieren. Gates. Commit `feat(track-list): add id-based selection and scroll anchor restore helpers (TAG-1)`

### Task A2: `reload()` integriert Capture/Restore
**Files:** Modify `track_list_reload.rs`
- [ ] `reload()` sichert vor dem Model-Swap `ReloadAnchor`, stellt danach Selektion + Scroll wieder her (Scroll via `glib::idle_add_local_once`, geklemmt). Aufrufer-Sweep: kein Aufrufer erwartet den Reset als Feature (sonst dort explizit `clear_selection()`).
- [ ] Smoke: bestehende Tests grün; manueller Check Sort-Klick/Rating-Edit behält Selektion.
- [ ] Gates. Commit `fix(track-list): preserve selection and scroll anchor across reload (TAG-1)` — **kein** Statusflip hier: TAG-1 verlangt zusätzlich die Selektion-nach-Save (G2); flippen erst dort, sonst stünde die Regel halb auf `[aktiv]`.

## Paket B — Kern (Welle 1)

### Task B1: Effektiver Diff + No-op-Skip + per-Track-Writes mit Progress
**Files:** Modify `library/tag_edit.rs`
**Interfaces:** `pub struct TrackWrite { pub id: i64, pub path: PathBuf, pub patch: TrackEditPatch }`; `pub fn apply_track_writes(conn, writes: &[TrackWrite], progress: &mut dyn FnMut(usize, usize)) -> TagBatchReport`; `pub fn classify_write_error(...) -> WriteErrorKind` (PermissionDenied/NotFound/UnsupportedFormat/Io — Vermerk: nach Missing-Umbau-Merge mit dessen Klassifikation zusammenführen).
- [ ] Rote Tests: `tag_5_noop_write_is_skipped_file_untouched` (mtime unverändert), `tag_5_rating_only_counts_but_writes_db_only`, `tag_5_progress_reports_written_over_total`, `write_error_classification_maps_permission_denied`.
- [ ] Watcher-Ignore wandert: pro Datei unmittelbar vor ihrem Write (`ignore_path`), nicht upfront für alle; stale `#[allow(dead_code)]`+Doc an `ignore_path` bereinigen (Ownership Paket B: `watcher.rs`).
- [ ] Gates (+ core-purity-grep). Commit `feat(core): per-track tag writes with progress, no-op skip, and error classification (TAG-5)`

### Task B2: TagEditSession
**Files:** Create `library/tag_edit_session.rs` (+ `library/mod.rs`-Zeile)
**Interfaces:** `pub enum TagField { Title, Artist, Album, AlbumArtist, Genre, Year, TrackNo, Rating }`; `pub struct TagEditSession` mit `new(tracks: Vec<SessionTrack>, mode: SessionMode)`, `set_pending(scope, field, value)` (Multi: alle; SingleNav: aktueller Track), `revert(scope, field)`, `mixed_placeholder(field) -> MixedPlaceholder` (≤2 → Werte inkl. „empty", ab 3 → Anzahl; Zähler), `summary() -> ReviewSummary { fields, tracks_affected }`, `review_lines() -> Vec<ReviewLine>` (nur effektive Änderungen zählen), `write_batch() -> Vec<TrackWrite>` (No-op-Tracks raus), `old_value_line(scope, field) -> Option<String>` (nur wenn alt ≠ neu), `mb_uniform_artist_album() -> Option<(String,String)>` (effektiv: Original + pending), `pending_track_count()`.
- [ ] Rote Tests: `tag_2_placeholder_lists_two_distinct_values_including_empty`, `tag_2_placeholder_counts_three_or_more`, `tag_2_clear_for_all_is_a_normal_pending_change`, `tag_4_pending_survives_track_switch`, `tag_5_summary_counts_fields_and_affected_tracks`, `tag_5_review_lines_count_only_real_changes`, `tag_5_exact_compare_no_trim`, `tag_5_all_pending_but_zero_effective_yields_empty_batch`, `mb_uniformity_uses_effective_values`.
- [ ] Gates. Commit `feat(core): tag edit session model — pending, effective diffs, review data (TAG-2/4/5)`

### Task B3: Autocomplete-Ranking + Ghost-Query
**Files:** Modify `queries/autocomplete.rs`
**Interfaces:** Ranking: Präfix vor Substring (`CASE WHEN col LIKE 'x%'`), dann Count desc, dann NOCASE; `pub fn query_ghost_completion(conn, column, input) -> Option<String>` (bester Präfix-Treffer, Count-Tiebreak); Konstanten `MAX_SUGGESTIONS = 6`, `MIN_DROPDOWN_CHARS = 2` (Export für UI).
- [ ] Rote Tests: `tag_6_prefix_ranks_before_substring`, `tag_6_limit_is_six`, `tag_7_ghost_is_best_prefix_by_track_count`, `tag_7_ghost_none_without_prefix_match`.
- [ ] Gates. Commit `feat(core): prefix-first autocomplete ranking and ghost completion query (TAG-6/7)`

## Paket C — Form-Layout 3a + Mixed-UX (Welle 2)

### Task C1: GtkEntry-Grid-Layout
**Files:** Modify `tag_editor_form.rs`, `tag_editor_widgets.rs`, `tag_editor_style.rs`
- [ ] Umbau: Label-über-Feld-GtkEntrys, Cover links neben Title/Artist/Album, 2-Spalten-Grid (Album Artist/Genre · Year/Track/Rating); Stift-Optik und „Change cover…" entfallen; reservierte „was:"-Zeile unter jedem Feld (P-4, leer aber platziert); Header-Subzeile (Single: „Track 3 of 12 · FLAC · 987 kbit/s" — Format aus Extension, Bitrate aus `bitrate_kbps`, Fehlendes entfällt; Multi: „Only changed fields will be written to all selected tracks"); Title/Track-No im Multi „—" + Tooltip (TAG-3).
- [ ] Test (Logik headless): `tag_3_per_track_fields_render_dash_readonly_in_multi`, Subzeilen-Formatter `subtitle_omits_missing_bitrate`.
- [ ] Gates. Commit `feat(tag-editor): 3a layout — entry grid, side cover, reserved diff lines, header subtitle (TAG-3)`

### Task C2: Mixed-Felder direkt tippbar
**Files:** wie C1
- [ ] Click-to-unlock raus; Platzhalter-Copy aus `TagEditSession::mixed_placeholder` (+ Zähler-Label, Ellipsize); erstes Zeichen/Backspace/Entf macht scharf (Session `set_pending`), Akzent-Border, ↺ im Feld, „will be applied to all N".
- [ ] Test: `tag_2_first_keystroke_arms_field`, `tag_2_backspace_in_placeholder_arms_as_clear_for_all`.
- [ ] Gates. Commit `feat(tag-editor): directly typable mixed fields with in-field revert (TAG-2)` — Statusflips TAG-2/TAG-3 → `[aktiv]` in `docs/ux-rules.md` (Tests `tag_2_…`/`tag_3_…` grün).

## Paket D — Autocomplete-UI + Ghost (Welle 2)

### Task D1: Popover-Erweiterung
**Files:** Modify `autocomplete_entry.rs`
- [ ] 6 Zeilen, Dropdown ab 2 Zeichen, „FROM YOUR LIBRARY"-Header, letzte Zeile „Use ‚X' as new artist…" (übernimmt Text wörtlich), erste Zeile vormarkiert, Ranking aus B3.
- [ ] Test: `tag_6_dropdown_needs_two_chars`, `tag_6_use_as_new_row_always_last`.
- [ ] Gates. Commit `feat(tag-editor): autocomplete dropdown — section header, use-as-new row, prefix ranking (TAG-6)`

### Task D2: Ghost-Overlay
**Files:** Modify `autocomplete_entry.rs`
- [ ] Ghost hinter dem Cursor (weiß 35 %) aus `query_ghost_completion`, auch < 2 Zeichen; Tab übernimmt nur sichtbaren Ghost, sonst Fokuswechsel (stille Erst-Zeilen-Übernahme entfernen!); Tab-Badge nur bei sichtbarem Ghost; Abschalt-Konstante `GHOST_ENABLED` (Fallback laut Beschluss); Ghost nie im Pending-State.
- [ ] Test: `tag_7_tab_accepts_only_visible_ghost`, `tag_7_tab_moves_focus_without_ghost`, `tag_7_ghost_disabled_hides_badge`.
- [ ] Gates. Commit `feat(tag-editor): inline ghost completion with tab accept and kill switch (TAG-7)` — Statusflips TAG-6/TAG-7 → `[aktiv]` in `docs/ux-rules.md` (TAG-6 braucht auch D1 grün).

## Paket E — Tastatur (Welle 3)

### Task E1: Enter-Kette + Save-Shortcuts
**Files:** Modify `tag_editor_save.rs`, `tag_editor_state.rs`
- [ ] `activates_default` raus; Enter: Dropdown offen → Übernahme (bleibt in D), zu → nächstes editierbares Feld (read-only/Rating übersprungen), letztes Feld → Save-Button fokussieren; Ctrl+Enter = Save, Ctrl+S Alias, beide über EINE Action (gemeinsamer Disabled-/Saving-Zustand); Shortcuts-Overlay-Eintrag.
- [ ] Test: `tag_8_enter_never_saves_from_text_field`, `tag_8_enter_skips_readonly_fields`, `tag_8_last_field_enter_focuses_save`, `tag_8_ctrl_enter_and_ctrl_s_share_one_action`.
- [ ] Gates. Commit `feat(tag-editor): TAG-8 enter semantics and ctrl+enter save`

### Task E2: Esc-Kaskade + Discard-Frage
**Files:** wie E1
- [ ] Kaskade (1) Popover (2) Feld-Revert (3) Dialog; Discard-Frage 2 Antworten „Discard changes to N tracks?" · Keep editing (Default) / Discard (destruktiv) — Save-Response entfernen.
- [ ] Test: `tag_8_esc_cascade_dropdown_then_revert_then_discard`, `tag_8_discard_prompt_counts_tracks_two_answers`.
- [ ] Gates. Commit `feat(tag-editor): esc cascade and two-answer discard prompt (TAG-8)` — Statusflip TAG-8 → `[aktiv]` in `docs/ux-rules.md` (setzt E1 voraus: Enter-Kette + Esc-Kaskade sind eine Regel).

## Paket F — Session-Integration + Review + Save-Flow (Welle 3, läuft allein)

### Task F0: Session-Verdrahtung (neu, entdeckt in Welle 2)

Paket C konnte TAG-2 nicht fertigstellen, weil die Daten nicht bis zum Formular
reichen — beide Lücken sind reine Verdrahtung, keine fehlende Kern-API:

1. `tag_editor.rs::present()` reicht nur das **kollabierte** `EditableTagSummary`
   durch (`MixedValue::Mixed` trägt keine Werte). Der reiche Platzhalter
   („Mixed — Ambient, Post-Rock" / „8 different values") braucht die
   Pro-Track-Distinct-Werte, die `TagEditSession::mixed_placeholder()` liefert.
2. Das In-Feld-↺ ist bewusst nicht verdrahtet: `tag_editor_dirty.rs` hält das
   Dirty-`Cell<bool>`-Array privat. Ein Revert von außen würde den sichtbaren Text
   zurücksetzen, ohne das Flag zu löschen — bei einem Mixed-Feld schriebe der Save
   dann einen Leerstring über tatsächlich unterschiedliche Werte.

Ebenso fehlt `bitrate_kbps` im Formular (nur `(id, PathBuf)` kommt an), weshalb die
Header-Subzeile bis heute nur das Format zeigt.

**Files:** Modify `tag_editor.rs`, `tag_editor_dirty.rs`, `tag_editor_form.rs`, `tag_edit_flow.rs`

- [ ] `TagEditSession` (aus `reprise-core`) wird in `present()` gebaut und bis
  `TagEditorForm::build` + `tag_editor_dirty::wire` durchgereicht; das Dirty-Array
  aus `Cell<bool>` weicht der Session als einziger Wahrheit (kein Doppel-State!).
  `SaveWidgets`/`save::wire` verlieren dabei den `dirty`-Parameter — **Signatur
  hier festlegen und stabil lassen**, Paket E baut in Welle 4 darauf auf.
- [ ] Pro-Track-Tags **und** `bitrate_kbps` bis zum Formular durchreichen (Subzeile
  zeigt dann Format + Bitrate; die Position „3 of 12" liefert erst Paket G).
- [ ] Reicher Mixed-Platzhalter aus `mixed_placeholder()`; In-Feld-↺ verdrahtet
  (revert über die Session, Flag und Text immer gemeinsam).
- [ ] Feld-Revert als **öffentliche API** für Paket E (Esc-Kaskade Stufe 2)
  bereitstellen — z. B. `pub(in crate::ui) fn revert_field(...)` oder ein Handle im
  Rückgabewert von `dirty::wire`. Ohne das kann E seine Kaskade nicht bauen.
- [ ] Tests: `tag_2_rich_placeholder_lists_values_from_session`,
  `tag_2_in_field_revert_clears_text_and_pending_together`.
- [ ] Gates. Commit `feat(tag-editor): thread the edit session through the dialog (TAG-2)`
  — Statusflip **TAG-2 → `[aktiv]`** in `docs/ux-rules.md`, sobald reicher
  Platzhalter + Revert stehen (sonst weiter `[geplant]` + Begründung).

### Task F1: Review-Footer
**Files:** Modify `tag_editor_dirty.rs` (→ Review-Projektion aus `TagEditSession`)
- [ ] Summary-Zeile „2 fields · 30 tracks affected"; Expander „Review changes" (Multi immer bei pending; Single sobald `pending_track_count() > 1`) mit `review_lines()`; „was:"-Zeilen befüllen (nur alt ≠ neu); Save-Label „Save N"/„Save · N tracks"/„Save"; disabled + Tooltip „No changes yet"/„No effective changes" (P-2).
- [ ] Test: `tag_5_summary_line_and_expander_track_currency`, `tag_5_save_disabled_with_tooltip_when_zero_effective`.
- [ ] Gates. Commit `feat(tag-editor): review footer — summary, expander, effective-diff save label (TAG-5)`

### Task F2: Save-Progress + FB-3-Fehlerpfad
**Files:** Modify `tag_edit_flow.rs`, `toasts.rs`; Create `tag_editor_failures.rs`
- [ ] Save hält Dialog offen: Spinner „Saving… 12/30", Felder + Cancel disabled, Progress via Channel aus `apply_track_writes`; danach Dialog zu. Toast: ohne Fehler aktionslos 4 s ersetzbar; mit Fehlern „Tags updated · 33 tracks · 2 failed [Details]" 10 s unverdrängbar (FB-1). Details-Dialog: Dateiname + klassifizierter Grund + „Edit failed tracks…" (öffnet Editor mit diesen Tracks frisch). Toast/Progress/Button in Track-Währung.
- [ ] Test: `fb_3_failures_collected_into_single_toast`, `fb_3_details_reopens_editor_with_failed_tracks` (Logik-Ebene).
- [ ] Gates. Commit `feat(tag-editor): in-dialog save progress and FB-3 failure path with details dialog`

## Paket G — Navigation + MB (Welle 4, läuft allein)

### Task G1: Listen-Snapshot + ‹›
**Files:** Modify `tag_editor.rs`, `tag_edit_flow.rs`, `tag_editor_form.rs` (Integration)
- [ ] N=1: Snapshot der sichtbaren Liste (Track-IDs) beim Öffnen; ‹›/Ctrl+PgUp/PgDn blättern (Subzeile „Track 3 of 12" aus Snapshot-Position, stabil gegen Re-Sort); per-Track-Pending über `TagEditSession` (SingleNav); invalides Zahlenfeld blockt Blättern; N>1: kein ‹›.
- [ ] Test: `tag_4_snapshot_positions_stable_across_resort`, `tag_4_invalid_number_blocks_navigation`.
- [ ] Gates. Commit `feat(tag-editor): browse selection snapshot with per-track pending (TAG-4)` — Statusflip TAG-4 → `[aktiv]` in `docs/ux-rules.md`.

### Task G2: Selektion nach Schließen + MB-Multi
**Files:** Modify `tag_edit_flow.rs`, `tag_editor_lookup.rs`
- [ ] Nach Save: Library-Selektion = geschriebene Track-IDs (Teilfehler: die gelungenen), Scroll-Anker bleibt (nutzt Paket A); Cancel: unverändert. MB-Multi: ein Lookup bei `mb_uniform_artist_album()`, füllt leere aggregierte Felder als pending; sonst disabled + Tooltip „Requires same artist & album across selection"; Hint „fills only empty fields"; Spinner im Button.
- [ ] Test: `tag_1_selection_after_save_is_written_tracks`, `mb_multi_requires_effective_uniform_artist_album`.
- [ ] Gates. Commit `feat(tag-editor): post-save selection, uniform-album MusicBrainz lookup (TAG-1)` — Statusflip TAG-1 → `[aktiv]` in `docs/ux-rules.md` (deckt jetzt A2 + G2).

## Paket H — Abnahme (Welle 5)

### Task H1: CUA-Abnahmeszenarien
- [ ] Die fünf Abnahme-Blöcke aus dem Beschlussdokument als CUA-Szenarien im
  bestehenden Harness (headless; Regel-IDs in Szenario-Namen). Fehlschläge fixen.
- [ ] Gates. Commit `test(gui): tag editor acceptance scenarios (TAG-1..8)`

### Task H2: Abschluss
- [ ] `docs/ux-rules.md`: alle TAG-Regeln `[aktiv]`? `./scripts/check-ux-traceability.sh` grün? Sonst splitten (a/b) statt halb flippen. Beschluss-Ledger:
  Abweichungen dokumentieren. Offene v2-Punkte (Per-Track-MB, Multi-Genre-Chips)
  als Vermerk.
- [ ] Commit `docs: close tag editor rework — status ledger and v2 notes`
