---
slug: dev-green-static-gates
worktree: /home/marvin/Projects/reprise-dev-green-static-gates
branch: feature/dev-green-static-gates
phase: shipped
codex_session:
created: 2026-08-11
---
# origin/dev wieder grün: statische Gates

`origin/dev` (4f6dfc7cb2) ist gegen die Gate-Liste aus
`scripts/check-merge-readiness.sh` an vier Stellen rot. Alle vier sind ohne
Verhaltensänderung reparierbar. Die Display-Tests sind ein eigenes Paket und
gehören **nicht** in diese Aufgabe.

Gegengeprüft am 2026-08-11 im Worktree `/home/marvin/Projects/reprise-dev` auf
genau diesem Commit — die vier Befunde sind Basis-Schulden, keine Regression
eines Feature-Branches. `cargo fmt`, `cargo clippy`, `check-motion-tokens.sh`,
`check-accessibility-semantics.sh`, `check-input-parity.sh`,
`check-device-sync-gstreamer.sh`, `check-runtime-service-install.sh` und
`scripts/tests/gettext-catalogs.sh` sind grün und dürfen es bleiben.

## 1. Architektur-Lint: zwei Dateien über der 800-Zeilen-Grenze

`scripts/check-architecture.sh` meldet:

```
crates/reprise-android-ffi/src/queue_boundary_tests.rs has 804 lines
crates/reprise-core/src/library/session.rs has 803 lines
```

Beide liegen knapp über der Grenze. **Nicht** durch Zeilenkosmetik (Kommentare
löschen, Zeilen zusammenziehen) unter 800 drücken — das kauft das Gate, ohne
die Ursache anzufassen. Stattdessen entlang einer echten Naht aufteilen:

- `queue_boundary_tests.rs` ist eine reine Testdatei. Eine zusammenhängende
  Gruppe von Testfällen in ein Nachbarmodul ziehen (z. B. nach dem geprüften
  Grenzfall benannt) und über `mod` einhängen.
- `library/session.rs` nach Verantwortlichkeit trennen. Die Aufteilung muss
  sich am Inhalt begründen lassen, nicht an der Zeilenzahl; wenn ein Block
  offensichtlich zusammengehört, wandert er komplett.

Beide neuen Dateien bleiben deutlich unter 800 Zeilen, damit die nächste
Ergänzung nicht sofort wieder anschlägt.

Verifikation: `scripts/check-architecture.sh` läuft ohne Ausgabe durch.

## 2. Dead-Code-Allowlist driftet

`scripts/check-frontend-thinness.sh` meldet einen neuen, nicht gelisteten
Eintrag:

```
> crates/reprise-gnome/src/ui/list_geometry.rs:1
```

Quelle ist `crates/reprise-gnome/src/ui/list_geometry.rs`, ungefähr Zeile 184:

```rust
#[allow(dead_code)] // The G4 readiness migration consumes this pure predicate.
pub(in crate::ui) fn is_settled(upper: f64, n_rows: usize, measurement: RowMeasurement) -> bool {
```

Der einzige Aufrufer dieser freien Funktion ist eine Assertion im Testmodul
derselben Datei (ungefähr Zeile 579). Die gleichnamige **Methode**
`ListGeometry::is_settled` ist etwas anderes: sie ruft
`settled_content_row_height` und hat echte Produktionsaufrufer in
`view_state_memory.rs` und `track_list_reload.rs` — die bleibt unangetastet.

Richtige Reparatur: das `#[allow(dead_code)]` samt Kommentar durch
`#[cfg(test)]` ersetzen, sodass die Funktion nur noch im Testbau existiert.
Der Allowlist-Block in `check-frontend-thinness.sh` bleibt dann unverändert.

**Nicht** stattdessen den Pfad in die Allowlist eintragen: der Kommentar
verspricht einen künftigen Aufrufer („G4 readiness migration"), den es nicht
gibt, und ein Allowlist-Eintrag würde diese Schuld dauerhaft festschreiben.

Verifikation: `scripts/check-frontend-thinness.sh` meldet keine Drift mehr.

## 3. rustdoc verlinkt aus öffentlicher Doku auf ein privates Item

`env RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`
scheitert mit Exit 101:

```
error: public documentation for `AndroidTrashFailure` links to private item `ALREADY_GONE`
  --> crates/reprise-android-ffi/src/playback_session/trash_boundary.rs:21:7
```

Zwei Wege. Entscheide anhand des Codes, nicht nach Bequemlichkeit:

- Ist `ALREADY_GONE` Teil der öffentlichen Oberfläche von
  `AndroidTrashFailure` (also etwas, das ein Aufrufer der FFI-Schicht wirklich
  sieht), dann gehört es öffentlich re-exportiert und der Link bleibt.
- Ist es ein internes Detail, dann darf die öffentliche Doku nicht darauf
  verlinken: aus dem Intra-Doc-Link eine schlichte Code-Spanne machen
  (`` `ALREADY_GONE` `` ohne eckige Klammern) und den Satz so umformulieren,
  dass er ohne Sprungziel verständlich bleibt.

Prüfe, ob dieselbe Konstruktion in benachbarten Dateien der
`playback_session`-Schicht noch einmal vorkommt, und ziehe sie mit.

Verifikation: `env RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace
--no-deps` ist grün.

## 4. Tests verweisen auf ersetzte UX-Regeln

`scripts/check-ux-traceability.sh` meldet:

```
ERROR: test references replaced rule NR-20 — re-point it
ERROR: test references replaced rule NR-25 — re-point it
```

`docs/ux-rules.md` markiert `NR-20` als `[replaced by NR-30]` und `NR-25` als
`[replaced by NR-31]`. Das Gate liest **Testnamen**: `fn nr_25_foo()` gilt als
Beleg für NR-25.

Betroffen sind 23 Tests. Sie wurden einzeln am Testkörper gegen den Text der
heute aktiven Regeln geprüft; das Ergebnis ist die Tabelle unten.
**Nicht** pauschal auf NR-30/NR-31 umbenennen — bei der NR-13-Runde belegten
von zehn Tests nur drei die Nachfolgeregel, und ein Test behauptete sogar das
Gegenteil. Übernimm die Zuordnung wie angegeben.

Umbenannt wird nur der Funktionsname (Präfix), nicht der Testkörper. Der
beschreibende Teil des Namens bleibt erhalten, es sei denn er nennt selbst eine
Regelnummer. Doc-Kommentare direkt über den Tests, die `NR-20`/`NR-25`
erwähnen, auf dieselbe Zielregel nachziehen — auch in
`releases_filter_bar.rs` (dort steht `NR-25/FIL-2a` in einem Doc-Kommentar)
und in `columns/release.rs`.

| Datei | Test | neues Präfix |
|---|---|---|
| `reprise-core/src/artist_news_links.rs` | `nr_20_bandcamp_purchase_url_accepts_only_real_bandcamp_hosts` | `nr_30_` |
| `reprise-gnome/src/ui/releases/releases_presentation.rs` | `nr_20_bandcamp_purchase_target_requires_a_real_bandcamp_relation` | `nr_30_` |
| `reprise-gnome/src/ui/releases/releases_view_tests.rs` | `nr_20_releases_view_exposes_filters_seven_columns_and_footer` | `nr_31_` |
| `reprise-core/src/artist_news_view_tests.rs` | `nr_25_default_window_hides_releases_older_than_five_years` | `nr_31_` |
| `reprise-core/src/artist_news_view_tests.rs` | `nr_25_singles_are_absent_until_their_chip_is_on` | `nr_31_` |
| `reprise-core/src/artist_news_view_tests.rs` | `nr_25_undated_release_survives_every_window` | `nr_31_` |
| `reprise-core/src/artist_news_view_tests.rs` | `nr_25_window_all_shows_the_full_catalog` | `nr_31_` |
| `reprise-core/src/artist_news_view_tests.rs` | `nr_25_empty_type_selection_shows_every_type` | `nr_31_` |
| `reprise-core/src/artist_news_view_tests.rs` | `nr_25_all_selected_types_with_all_window_is_the_widest_scope` | `nr_31_` |
| `reprise-core/src/artist_news_view_tests.rs` | `nr_25_count_line_never_exceeds_its_total` | `nr_31_` |
| `reprise-core/src/artist_news_view_tests.rs` | `nr_25_a_fresh_library_loads_exactly_the_default_filter` | `nr_31_` |
| `reprise-gnome/src/ui/releases/releases_columns.rs` | `nr_25_table_has_the_five_named_columns` | `nr_31_` |
| `reprise-gnome/src/ui/releases/releases_filter_bar.rs` | `nr_25_type_toggles_are_independent_and_empty_means_every_type` | `nr_31_` |
| `reprise-gnome/src/ui/releases/releases_filter_bar.rs` | `nr_25_filter_header_is_permanent_and_reserves_its_height` | `nr_31_` |
| `reprise-gnome/src/ui/releases/releases_empty_state.rs` | `nr_25_gaps_beyond_the_window_offer_show_all` | `nr_31_` |
| `reprise-gnome/src/ui/releases/releases_presentation.rs` | `nr_25_activation_uses_restore_or_external_release_link` | `nr_31_` |
| `reprise-view/src/columns/release.rs` | `nr_25_the_default_release_layout_leads_with_the_cover` | `nr_31_` |
| `reprise-gnome/src/ui/releases/releases_filter_bar.rs` | `nr_25_widest_scope_count_line_names_shown_and_total` | `fil_2a_` |
| `reprise-gnome/src/ui/releases/releases_filter_bar.rs` | `nr_25_the_default_filter_row_offers_no_clear_all` | `fil_2a_` |
| `reprise-gnome/src/ui/releases/releases_empty_state.rs` | `nr_25_releases_empty_state_matrix_has_one_next_step` | `fil_6_` |
| `reprise-core/src/artist_news_view_tests.rs` | `nr_25_release_status_distinguishes_upcoming_incomplete_and_missing` | **Präfix ersatzlos streichen** |
| `reprise-gnome/src/ui/strings_releases.rs` | `nr_25_release_counts_name_discography_gaps` | **Präfix ersatzlos streichen** |
| `reprise-gnome/src/ui/releases/releases_presentation.rs` | `nr_25_status_pills_describe_discography_gaps` | **Präfix ersatzlos streichen** |

Zu den drei letzten: sie prüfen die Status-Werte (`upcoming`, `Missing`,
`Incomplete`, `X of Y tracks`) bzw. reine String-Formatierung. Diese Werte
werden von **keiner** heute aktiven Regel mehr wörtlich festgelegt — nur die
abgelöste NR-17 tat das. Ein Regelpräfix wäre also eine Behauptung von
Abdeckung, die es nicht gibt. Sie behalten ihren beschreibenden Namen ohne
Präfix (z. B. `release_status_distinguishes_upcoming_incomplete_and_missing`).
Diese Dokumentationslücke selbst ist **nicht** Teil dieser Aufgabe — nicht
versuchen, `docs/ux-rules.md` um eine neue Regel zu erweitern.

Falls beim Umbenennen ein Namenskonflikt mit einem bereits existierenden Test
entsteht (etwa in `releases_filter_bar.rs`, wo schon ein `fil_2a_*`-Test
steht), den neuen Namen um sein unterscheidendes Merkmal erweitern statt einen
der beiden zu löschen.

Verifikation: `scripts/check-ux-traceability.sh` läuft grün durch. Danach
zusätzlich prüfen, dass keine aktive Regel ihren Beleg verloren hat — das Gate
meldet das selbst als „`[active]` rule … has no rule-named test".

## Abnahme

Nacheinander, alle vier müssen grün sein:

```
scripts/check-architecture.sh
scripts/check-frontend-thinness.sh
scripts/check-ux-traceability.sh
env RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo fmt --check
cargo clippy --locked --all-targets --workspace -- -D warnings
cargo test --locked --workspace --exclude reprise-platform-linux
```

`scripts/check-merge-readiness.sh` **nicht** starten — es fährt pro UX-Regel
einen eigenen Display-Test-Lauf mit eigenem Xvfb und terminiert praktisch nie.
Display-Tests gehören nicht in diese Aufgabe.

`cargo test -p reprise-platform-linux` läuft nur mit `-- --test-threads=1`
verlässlich; parallel stören sich die GStreamer-Tests gegenseitig.
