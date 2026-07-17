# Abnahme: selbstheilende Problem-Listen

Stand: Paket 6, Task 6.3. Diese Abnahme ordnet die geforderten Szenarien den
bereits gelandeten Verhaltens- und Regeltests zu. Bewusst wird kein zweiter
Test derselben Ebene angelegt, wenn P1–P5 die Aussage bereits vollständig
beweisen.

## Automatisierte Abnahmematrix

| Szenario | Automatisierter Beweis |
|---|---|
| Rename + Scan relinkt mit Ratings und Toast | `move_via_rename_preserves_metadata` beweist gleiche Track-ID, Rating, Play-Count und `moved == 1`; `heal_toast_omits_zero_parts_and_is_absent_when_nothing_healed` beweist daraus „N moved files relinked" und die Aggregation. |
| Root-Unmount + Scan ergibt `RootUnavailable`, ohne Markierung | `scan_folder_root_guard_b_empty_root_with_mismatched_device_reports_root_unavailable` beweist `RootUnavailable` und unverändertes `missing_since`; `completed_scan_persists_relinks_and_runs_auto_clean_but_unavailable_does_neither` beweist zusätzlich, dass dieser Ausgang kein Auto-Clean startet. |
| Teil-Mount weg ergibt unavailable und ist nicht löschbar | `p_6_mount_evidence_heals_existing_marks_ejected_and_never_deletes_guesses` beweist Mount-Evidenz, `unmounted` und fehlende Auto-Clean-Eignung; `deleted_card_is_the_only_actionable_missing_group` und `sidebar_bulk_cleanup_selects_only_proven_deleted_tracks` beweisen, dass unavailable/unknown keine Löschaktion und keine Bulk-Lösch-IDs liefern. |
| Delete + Scan ergibt deleted; Remove all hat exaktes Undo | `scan_folder_folds_a_deleted_file_into_the_same_scan` beweist die `deleted`-Klassifikation; `bulk_cleanup_routes_keep_issue_rows_reversible` beweist den Sidebar-Weg; `fb_7_tombstone_undo_is_exact_and_expiry_commits_cascades` beweist Tombstone, exaktes Undo und die spätere Kaskade. |
| Tags reparieren + Re-Read entfernt Fehlerzeile | `real_tags_on_a_later_scan_heal_the_hint_and_clear_untagged` und `tag_editor_save_rereads_tags_and_clears_the_untagged_import_hint_immediately` beweisen Scan- und Tag-Editor-Weg; `healed_import_hint_refreshes_in_place_without_a_success_toast` beweist den stillen UI-Abschluss. |
| Dismiss + Datei ändern reaktiviert den Eintrag | `dismissed_file_with_changed_mtime_starts_a_new_episode` beweist Rückkehr, neue Episode und `seen_count == 1`; `fb_4_badges_count_new_since_viewed_and_reactivated_episode_is_new` beweist, dass diese Episode wieder badgt. |
| Fünf Scans erzeugen keine Duplikate | `repeated_scans_of_same_broken_file_produce_one_episode_row` führt fünf echte Scans aus und beweist genau eine Zeile mit `seen_count == 5`. |
| Playlist hält Missing-Track grau an fester Position | `playlist_window_keeps_missing_tracks_at_their_playlist_positions` beweist Position und Missing-Zustand; `missing_title_css_uses_half_opacity` beweist die graue Darstellung, während `apply_missing_title` den Strikethrough bei jedem Bind aus dem Zustand neu setzt. |

Die vollständige Workspace-Suite ist grün: 1290 bestanden, 0 fehlgeschlagen,
89 display-gebunden ignoriert. `scripts/check-ux-traceability.sh` bestätigt
alle 15 aktiven Regeln dieses Features.

## Headless-Smoke-Entscheidung

Die bestehende `scripts/cua-e2e/run.sh`-Harness kann nur First-run und eine
befüllte Library-Suche aufbauen. Sie besitzt weder einen isolierten Seeder für
Missing-/Import-Error-Episoden noch Aktionen für die neue Issue-Card,
Badge-Quittierung oder den Remove-all-Undo-Toast. Der ältere
`REPRISE_SMOKE_MENU_ACTION=remove-from-library` treibt den Tracklistenpfad und
nicht den neuen Tombstone-/Toast-Pfad; er wäre daher kein Beweis für diese
drei Abnahmepunkte. Paket 6 erweitert die Harness nicht um eine nur scheinbar
passende Parallelstrecke. Der deterministische Fake-Driver-Vertrag bleibt über
`scripts/tests/cua-e2e.sh` im QA-Gate gedeckt.

## Manuelle Restliste für den Maintainer

- Echtes NAS-Mount-Event: Root und Teil-Mount auswerfen/einhängen; unavailable
  muss sofort erscheinen und beim Mount ohne Scan heilen.
- `GVolumeMonitor` auf echter Hardware einschließlich Eject/Remount und
  laufender Wiedergabe prüfen; der spielende Track darf nie proaktiv stoppen.
- Optik-Review der Missing- und Import-Error-Cards gegen 18a, einschließlich
  Grau/Strikethrough und Collapse/Paging.
- Im echten Widget-Baum Issues-Sichtbarkeit, new-since-viewed-Badges und den
  10-Sekunden-Remove-all-Toast samt Undo visuell und per Klick prüfen.
