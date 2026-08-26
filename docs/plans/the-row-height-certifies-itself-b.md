---
slug: the-row-height-certifies-itself-b
worktree: /home/marvin/Projects/reprise-the-row-height-certifies-itself-b
branch: feature/the-row-height-certifies-itself-b
phase: coded
codex_session:
created: 2026-08-26
---
# Strand B — the persisted number and the record

Mother plan: [`the-row-height-certifies-itself.md`](the-row-height-certifies-itself.md).
Read its sections 2–3 and 8 before starting.

**File ownership.** This strand writes only to:

```
crates/reprise-core/src/library/settings_geometry.rs
crates/reprise-core/src/db.rs
crates/reprise-core/src/library/settings_geometry_migration_tests.rs   (new)
docs/plans/the-table-follows-the-music-again.md
```

Nothing in `crates/reprise-gnome`. The geometry fix is strand A's.

**Lands after strand A.** This is not a preference. Clearing the setting while
the still-broken build is in the field only makes room for it to persist another
wrong height on the next launch. Strand A removes the writer; this strand removes
the residue.

---

## Task 1 — schema v80 clears the poisoned setting

`crates/reprise-core/src/library/settings_geometry.rs`,
`crates/reprise-core/src/db.rs`

Every database that ran the affected nightly holds `ui.row_height = 30`
(confirmed on the live one: `user_version = 79`, `ui.row_height|30`). Strand A
stops it being re-written and stops it being *believed* once GTK has spoken — but
nothing removes it, and it still seeds `upper` in the window before GTK's first
allocation.

Add `migrate_v80(conn)` deleting the `ui.row_height` and
`ui.section_header_height` rows. Follow the exact shape of `migrate_v79` in the
same file: read `PRAGMA user_version`, return early when already ≥ 80, open
`conn.unchecked_transaction()`, `DELETE FROM settings WHERE key = ?1` per key,
`transaction.pragma_update(None, "user_version", 80)`, commit.

`ui.section_header_height` does not exist in the live database — that delete is a
no-op there. It is included because the sectioned path persists through the same
poisoned mechanism and a database that used the queue view may carry one.

Register it in `db.rs`: one line after
`crate::library::settings::migrate_v79(conn)?;` (currently line 761) inside
`migrate_with_cache_dirs`, and bump `SUPPORTED_SCHEMA_VERSION` from 79 to 80
(`db.rs:26`).

**No older migration re-creates a settings row**, so the `has_column`
self-healing trap from `memory/reprise-self-healing-migrations-fight-drops.md`
does not apply. The only thing that can re-create this key is the production
writer, which is strand A's subject — hence the merge order.

**Why dropping is safer than correcting.** With no persisted height,
`load_row_height` falls back to `ROW_MIN_HEIGHT` marked `Assumed`, and
`preseed_upper`'s `Assumed` arm only ever *grows* a range below its lower bound —
it cannot shrink GTK's own `upper` the way the current `Measured` 30 does. The
absent value is the safe state; a guessed replacement would be a fourth
self-certified number.

**Test 1a** (new `settings_geometry_migration_tests.rs`, following the
`db_*_migration_tests.rs` convention): a v79 database carrying `ui.row_height`
loses it at v80 and `user_version` becomes 80.
**Test 1b**: running the migration twice is a no-op — the second run changes
nothing and does not error.
**Test 1c**: an unrelated settings row (`ui.column_widths`, say) survives
untouched. The delete must be keyed, not a table sweep.
**Test 1d**: a database already at v80 is left alone by the early return.

## Task 2 — correct the predecessor's record

`docs/plans/the-table-follows-the-music-again.md`

That plan records cross-check 1 — the closing manual check — as **"still owed"**.
It is not owed: it was performed, by the user, and it **failed**. Left as is it
reads in a month as "never checked" instead of "checked and red", which is how
this whole family of bugs stayed invisible for a day.

Correct that line to say it was performed on 2026-08-25, that it failed, and
point it at this plan as the successor. Do not rewrite the rest of the file — it
is an accurate record of what that plan did.

---

## Verification

- The four migration tests, each with its **mutation probe** recorded: revert the
  production change, run the test, confirm it fails, paste the failure output
  here, discard the reversion.
- The gate list from `scripts/check-merge-readiness.sh` — never hand-assembled.
- A schema bump touches a line other in-flight branches may also touch. Before
  landing, check `git log --oneline HEAD..origin/dev` for another migration that
  arrived in the meantime and rebase onto it rather than merging past it — two
  branches both claiming v80 is a silent corruption, not a conflict.

No display tests and no measurement arm belong to this strand: it changes no
geometry and produces no viewport behaviour. Its effect on the running app is
verified in the mother plan's post-merge cross-checks 1 and 2, which need both
strands present.

## Acceptance evidence

All probes ran against one exact filtered Core test at a time. Each temporary
production regression was restored before the next probe.

### Test 1a — registered v80 cleanup

Mutation: removed the `migrate_v80` call from `migrate_with_cache_dirs`,
leaving the v79 database and its poisoned row untouched.

```text
running 1 test
test db::settings_geometry_migration_tests::v80_clears_persisted_list_geometry_and_records_the_version ... FAILED

thread 'db::settings_geometry_migration_tests::v80_clears_persisted_list_geometry_and_records_the_version' (292) panicked at crates/reprise-core/src/library/settings_geometry_migration_tests.rs:47:5:
assertion `left == right` failed
  left: Some("30")
 right: None

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2603 filtered out; finished in 0.08s
mutation_probe_exit=101
```

### Test 1b — second run is a no-op

Mutation: removed `migrate_v80`'s version guard, so its second invocation
deleted a row written after the completed first invocation.

```text
running 1 test
test db::settings_geometry_migration_tests::v80_is_a_no_op_when_run_a_second_time ... FAILED

thread 'db::settings_geometry_migration_tests::v80_is_a_no_op_when_run_a_second_time' (292) panicked at crates/reprise-core/src/library/settings_geometry_migration_tests.rs:66:5:
assertion `left == right` failed
  left: [("online-sources-enabled", "0")]
 right: [("online-sources-enabled", "0"), ("ui.row_height", "45")]

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2601 filtered out; finished in 0.08s
mutation_probe_exit=101
```

### Test 1c — unrelated settings survive

Mutation: replaced the two keyed deletes with an unkeyed table delete.

```text
running 1 test
test db::settings_geometry_migration_tests::v80_preserves_unrelated_settings ... FAILED

thread 'db::settings_geometry_migration_tests::v80_preserves_unrelated_settings' (292) panicked at crates/reprise-core/src/library/settings_geometry_migration_tests.rs:82:5:
assertion `left == right` failed
  left: None
 right: Some("{\"title\":320,\"artist\":180}")

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2602 filtered out; finished in 0.10s
mutation_probe_exit=101
```

### Test 1d — an already-current database is untouched

Mutation: removed `migrate_v80`'s version guard, so a database already stamped
v80 lost both geometry rows.

```text
running 1 test
test db::settings_geometry_migration_tests::v80_leaves_an_already_current_database_alone ... FAILED

thread 'db::settings_geometry_migration_tests::v80_leaves_an_already_current_database_alone' (292) panicked at crates/reprise-core/src/library/settings_geometry_migration_tests.rs:98:5:
assertion `left == right` failed
  left: [("online-sources-enabled", "0")]
 right: [("online-sources-enabled", "0"), ("ui.row_height", "45"), ("ui.section_header_height", "49")]

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2603 filtered out; finished in 0.08s
mutation_probe_exit=101
```

### Restored green control

```text
running 4 tests
test db::settings_geometry_migration_tests::v80_clears_persisted_list_geometry_and_records_the_version ... ok
test db::settings_geometry_migration_tests::v80_is_a_no_op_when_run_a_second_time ... ok
test db::settings_geometry_migration_tests::v80_leaves_an_already_current_database_alone ... ok
test db::settings_geometry_migration_tests::v80_preserves_unrelated_settings ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 2600 filtered out; finished in 0.37s
```

### Ownership-gap follow-up

The mother plan's ownership list for strand B was one file short: it omitted
`crates/reprise-core/src/db_concerts_migration_tests.rs`, whose deliberately
hard-coded schema-version tripwire still pinned v79 after this strand raised
`SUPPORTED_SCHEMA_VERSION` to 80. Ownership was extended by exactly that file
so the test name and independent literal could advance to v80 without weakening
the tripwire into a tautological comparison.

The complete `reprise-core` suite ran with fresh worktree-local XDG data and
cache roots and exited 0:

```text
test result: ok. 2601 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 146.29s
```
