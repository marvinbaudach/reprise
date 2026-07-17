# Tag-Editor-Rework — Abnahme-Matrix

Stand: 2026-07-17. Die fünf Zeilen entsprechen den fünf Abnahme-Blöcken im
[Beschlussdokument](2026-07-17-tag-editor-beschluesse.md#abnahme). Die Ebenen sind
additiv: Ein CUA-Szenario ersetzt die bestehenden GTK-/Core-Tests nicht.

| Abnahme-Block | e2e (laufende App, CUA) | gtk (reprise-gnome) | core (reprise-core) | manuell |
|---|---|---|---|---|
| 1. Multi-Editor, Mixed-Felder, Autocomplete, Ghost und Review | `tag-3-multi-dialog-structure`: öffnet den echten Zwei-Track-Dialog und prüft Titel, Subzeile, Save/Cancel, alle Feldlabels sowie die per-Track-Projektion von Title. Keine CUA-Abdeckung für Tippen, Dropdown-Auswahl, Ghost oder Review-Dynamik. | `tag_2_mixed_placeholder_sits_in_the_entry`, `tag_2_counter_annotation_shows_distinct_values`, `tag_2_rich_placeholder_lists_values_from_session`, `tag_2_first_keystroke_arms_field`, `tag_2_backspace_in_placeholder_arms_as_clear_for_all`, `tag_3_per_track_fields_render_dash_readonly_in_multi`, `tag_5_summary_line_and_expander_track_currency`, `tag_6_dropdown_needs_two_chars`, `tag_6_use_as_new_row_always_last`, `tag_7a_tab_accepts_only_visible_ghost`, `tag_7a_tab_moves_focus_without_ghost`, `tag_7a_ghost_disabled_hides_badge` | `tag_2_placeholder_lists_two_distinct_values_including_empty`, `tag_2_placeholder_counts_three_or_more`, `tag_2_clear_for_all_is_a_normal_pending_change`, `tag_5_summary_counts_fields_and_affected_tracks`, `tag_5_review_lines_count_only_real_changes`, `tag_6_prefix_ranks_before_substring`, `tag_6_limit_is_six`, `tag_7a_ghost_is_best_prefix_by_track_count`, `tag_7a_ghost_none_without_prefix_match` | Ghost-Erscheinung und Pixel-Optik: manuell (TESTING.md), weil `GHOST_ENABLED = false` und Xvfb kein finales Rendering beweist. Der vollständige 35-Track-Interaktionsfluss bleibt ebenfalls manuell (TESTING.md). |
| 2. Enter-Kette, Ctrl+Enter und Ctrl+S | Keine e2e-Abdeckung: Der Harness bietet keine rohen Tastendrücke oder Modifier. | `tag_8_enter_never_saves_from_text_field`, `tag_8_enter_skips_readonly_fields`, `tag_8_last_field_enter_focuses_save`, `tag_8_ctrl_enter_and_ctrl_s_share_one_action` | — | Tastaturfluss und Fokus-Sichtbarkeit: manuell (TESTING.md). |
| 3. Save-Progress, Toast, Selektion/Scroll und Fehlerpfad | `tag-1-no-jump-after-save`: führt den echten Fixture-Write über `REPRISE_SMOKE_TAG_EDIT=title:…` aus, verlangt Batch-/Reload-/Sidebar-Marker und hält einen Snapshot der editierten Library-Row fest. Keine e2e-Abdeckung für den transienten Progress oder den injizierten Teilfehlerpfad. | `tag_1_positions_for_ids_maps_surviving_ids_only`, `tag_1_scroll_target_follows_anchor_row_after_resort`, `tag_1_scroll_target_none_when_anchor_gone`, `tag_1_deleted_ids_drop_silently`, `tag_1_selection_after_save_is_written_tracks`, `tag_5_summary_line_and_expander_track_currency`, `fb_3_failures_collected_into_single_toast`, `fb_3_details_reopens_editor_with_failed_tracks` | `tag_5_progress_reports_written_over_total`, `tag_5_noop_write_is_skipped_file_untouched`, `tag_5_rating_only_counts_but_writes_db_only` | Progress-Optik: manuell (TESTING.md), weil zwei lokale Fixtures im Sub-Sekunden-Bereich fertig sind. Toast-/Selektions-Rendering und der sichtbare Teilfehlerdialog bleiben manuell (TESTING.md). |
| 4. Einzeltrack-Snapshot, Subzeile, Blättern und verteiltes Pending | Keine e2e-Abdeckung: Ctrl+Page Up/Down und Pfeiltasten sind im Harness nicht verfügbar. | `tag_4_snapshot_positions_stable_across_resort`, `tag_4_invalid_number_blocks_navigation`, `subtitle_omits_missing_bitrate` | `tag_4_pending_survives_track_switch` | Reales Blättern, Fokus und Subzeilen-/Layout-Rendering: manuell (TESTING.md). |
| 5. Esc-Kaskade und Discard-Frage | Keine e2e-Abdeckung: Der Harness kann Esc nicht senden. | `tag_8_esc_cascade_dropdown_then_revert_then_discard`, `tag_8_discard_prompt_counts_tracks_two_answers`, `tag_2_in_field_revert_clears_text_and_pending_together` | — | Esc-Kaskade, Default-Fokus und Dialog-Rendering: manuell (TESTING.md). |

## Grenzen der CUA-Abnahme

- Der Harness nutzt ausschließlich `click`, `double_click` und `type_text` auf
  AT-SPI-Labels. Enter, Esc, Tab, Pfeile und Modifier-Kombinationen werden nicht
  künstlich über einen neuen Helfer simuliert.
- TAG-7b bleibt `[geplant] [manuell]`: Der sichtbare Ghost ist abgeschaltet und
  headless nicht belastbar prüfbar. TAG-7a deckt nur seine Mechanik.
- Der lokale Zwei-Fixture-Save ist zu kurzlebig für eine stabile
  Progress-Snapshot-Assertion.
- Screenshots werden als Diagnose-Evidenz behalten. Pixel-Rendering gilt nach
  TESTING.md nicht als durch Xvfb/CUA automatisiert abgenommen.
