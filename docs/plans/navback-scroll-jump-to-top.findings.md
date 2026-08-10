# Review-Befunde zu fix/navback-anchor (Stand 2026-08-10)

Reviewer: `rust-reviewer` (Korrektheit) + `worker` (Testqualität), beide gegen
`ec58514bf9...HEAD` im Worktree `~/Projects/reprise-navback`.
`cargo check --all-targets` und `clippy -D warnings` sind sauber.

## KRITISCH — 1. Der Zeilenhöhen-Cache friert nach der ersten Messung ein

`crates/reprise-gnome/src/ui/track_list/track_list_geometry.rs:32`

```rust
if let Some(height) = row_height(column_view, n_rows) {   // height == upper / n_rows
    let cached = last_row_height.get();
    if cached <= 0.0
        || restore_geometry_is_ready(height * f64::from(n_rows), n_rows as usize, cached)
```

`height * n_rows` ist per Definition wieder `upper` — die Prüfung reduziert sich
auf `|upper − n_rows·cached| < cached·0.5`. Die erlaubte Abweichung ist also
eine halbe **alte** Zeile, absolut, während der Fehler mit der Zeilenzahl
wächst. Bei einem echten Höhenwechsel (Dichte Standard 34 px → Compact 24 px)
und 200 Zeilen stehen 2000 px Abweichung gegen ~17 px Toleranz: dauerhaft
abgelehnt. Selbst nachgerechnet und bestätigt.

Folge: `last_row_height` behält die allererste gemessene Höhe für die gesamte
Sitzung (kein Reset-Pfad; Initialisierung nur in `track_list_builder.rs:96`).
`list_density::apply` wechselt nur die CSS-Klasse und ruft nichts davon auf.

Schaden je Aufrufer:
- `track_list_reload.rs:428-450` (mit `AdjustmentHold`): `restore_geometry_is_ready`
  schlägt fehl → der neue `adjustment.set_upper(current_ids.len() * height)`-Zweig
  greift mit der **veralteten** Höhe und überschreibt GTKs korrekt berechnete
  Grenze. Der Hold verteidigt das falsche Ziel anschließend aktiv gegen jede
  Layout-Korrektur → **dauerhaft erzwungene falsche Scrollposition** nach jedem
  Reload, der auf einen Dichtewechsel folgt. Also genau die Fehlerklasse, die
  dieser Branch beseitigen soll, über einen neuen Auslöser wieder eingeführt.
- `view_state_memory.rs:200-217` (ohne Hold): die 8 Idle-Runden laufen still ins
  Leere → Scroll-Wiederherstellung nach Dichtewechsel bis zum Neustart tot.
- `track_list_reload.rs:600-604` (`run_query`, vor dem Modelltausch, Geometrie
  konsistent): wäre die gefahrlose Stelle zum Nachführen — dasselbe Gate
  verhindert es.

## WICHTIG — 2. `adjustment.set_upper(...)` steht nicht im Plan und ist fragil

`crates/reprise-gnome/src/ui/track_list/track_list_reload.rs:436-440`

Auch mit korrekter Höhe wird eine GTK-eigene, layoutabhängige Eigenschaft
außerhalb des Allocation-Passes von Hand überschrieben. Die beiden Schutzwälle
(`model_matches_ids`, `has_no_section_headers`) decken genau zwei bekannte Fälle
ab; jeder künftige nicht gleichverteilte Listeninhalt (Banner, Footer), der
nicht über `queue_sections` läuft, verletzt die Annahme lautlos.
`has_no_section_headers` ist zudem in **keinem** Test der Codebasis je `false` —
alle bauen mit leerem `QueueViewModel`.

## WICHTIG — 3. Der zweite Fix (nav_10b) hat keinen eigenen Test

Commit `46e29c2a07` ändert `adjustment_hold.rs:141-146` und verschärft
`MAX_READY_DRIFT_IN_ROWS`, ohne einen einzigen neuen oder geänderten Test.
`delete_follow_display_tests.rs` ist unverändert. Die Absicherung ist allein der
vorbestehende `nav_10b`-Test, dessen Gegenprobe laut Codex' eigenem Protokoll
nicht sauber trennte (auch ohne Fix rot: 3942 statt 3922) — anders als beim
Navback-Fix, wo der Kontrast eindeutig ist (361,5 → 37400).

## GERING

4. `remember_row_height` hat keine direkte Testabdeckung; ein Unit-Test mit
   `n_rows=200, cached=34.0, gemessen 24.0` hätte Befund 1 sofort gezeigt.
   Nur `restore_geometry_is_ready` hat zwei Tests (`track_list_geometry.rs:68-83`).
5. `row_height_for_restore` (`track_list_geometry.rs:41-55`) ist eine reine
   Funktion ohne GTK-Bezug und trotzdem ungetestet (`n_rows == 0`, `upper <= 0`,
   Cache leer vs. befüllt).
6. `usize::try_from(shared.model.n_items())` (`track_list_reload.rs:428`) —
   `n_items()` ist `u32`, der Fehlerzweig ist unerreichbar.
7. Die Leerheitsprüfung der Sample-Reihe (`navback_anchor_display_tests.rs:246`)
   verlangt keine Mindestanzahl; unter Last könnte ein Lauf mit 2 Samples grün
   werden und deutlich weniger beweisen.

## Als korrekt geprüft

- `adjustment_hold.rs:140-149`: Die Umstellung auf ein verschachteltes
  `idle_add_local_once` ist stichhaltig — Timeout läuft bei Priorität 0, die
  Korrektur bei `HIGH_IDLE` (100), das Release jetzt bei `DEFAULT_IDLE` (200),
  also nach der Korrektur. Idempotent, kein Leak.
- `track_list.rs` / `track_list_builder.rs`: reine Feldergänzung.
- Retry-Ketten: begrenzt, trampoliniert, keine Rekursion, keine Leaks.
- Keine neuen Panics, keine Division durch null, keine Borrow-Konflikte.
- Die vier Navback-Display-Tests selbst: der Sampler startet vor dem Auslöser,
  die Erwartungswerte stammen nicht aus dem fehlerhaften Rechenweg, die
  Vorbedingungen fangen einen degenerierten Aufbau ab, und `focus_in_table` ist
  ein echter eigener Zweig (`view_state_memory.rs:185`, `grab_focus()` nur bei
  `TrackFocus::Track`).
