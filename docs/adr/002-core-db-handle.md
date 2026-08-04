# ADR 002: reprise-core hands out a `Db` handle, not a `rusqlite::Connection`

## Status

Accepted on 2026-07-29.

## Context

`crates/reprise-core/src/db.rs` hands out a bare `rusqlite::Connection` through
`open_migrated`. The GTK frontend wraps it in `Rc<RefCell<Connection>>` (76
files) and borrows it in 575 places across 130 files. There is no core-side
handle.

`scripts/check-frontend-thinness.sh` caps the `rusqlite` category at 538 — a
number whose name is misleading. Measured with the gate semantics
(`#[cfg(test)]` skipped, comment lines excluded), `params!` stands at 0,
`.prepare(` at 0 and `.query_row(` at 3, while `Connection` occurs 370 times
and `rusqlite::` 165 times. The frontend writes no SQL; it merely passes
connections through its own signatures. The budget therefore does not measure
what its name claims, and every future real violation is lost in the baseline
level.

The second consequence weighs heavier. `AGENTS.md` calls RefCell discipline the
"#1 recurring panic class". The 575 `borrow()` calls are exactly that class: an
expression such as `get_color_scheme(&context.conn.borrow())` holds a temporary
`Ref` for the entire call; if the called function triggers a GTK callback that
accesses the connection again, that is a `BorrowMutError` — a crash that only
appears under a particular timing.

The portability of `reprise-core` — a second frontend is meant to inherit the
domain logic — also remains a claim as long as the boundary between frontend
and core is a convention and not a type.

## Decision

`reprise-core` owns a handle type `Db` that keeps the `Connection` private, and
takes it in its **public** API everywhere `&Connection` stood before. The
handle lives in `crates/reprise-core/src/db_handle.rs` and is included from
`db.rs` as a private submodule (not directly in `db.rs` — which stood at 779
lines at the time of the decision).

```rust
pub struct Db { conn: Connection }

impl Db {
    pub fn open_migrated(path: Option<&Path>) -> Result<Self, DbError>;
    pub fn open_in_memory() -> Result<Self, DbError>;
    pub fn path(&self) -> Option<PathBuf>;
    pub(crate) fn conn(&self) -> &Connection;
}
```

Three commitments carry the design:

**No `Deref<Target = Connection>`.** That would expose the connection again
through the back door and defeat the purpose.

**No interior mutability.** `Db` holds the `Connection` without a `RefCell`.
The 62 core functions that take `&mut Connection` today do so solely because of
`conn.transaction()`; they switch to `unchecked_transaction()`, which gets by
with `&Connection` and is already used in 13 places in the core
(`concerts/pipeline.rs`, `podcasts/store.rs`, `scrobbling/queue.rs` among
others). This makes the 575 `borrow()` calls disappear without replacement
instead of hiding behind a handle method — a handle that merely encapsulates
the `RefCell` would make the panic class invisible instead of eliminating it,
and would be worse than the starting state.

**Only the public layer moves.** 386 of the 587 connection-taking core
functions are `pub`; the remaining ~200 private ones stay on `&Connection`. A
`pub fn` fetches `let conn = db.conn();` in its first line and calls the
private layer unchanged.

During the migration, `Db::conn()` is temporarily `pub` so that every stage
keeps compiling and stays testable; the migration only counts as finished once
`conn()` has been downgraded to `pub(crate)`. From then on the compiler finds
every caller outside the core that still wants to touch the connection.

## Consequences

- The frontend can no longer reach the raw connection. The measured
  `rusqlite` budget in `check-frontend-thinness.sh` fell from 538 to 112; the
  remaining hits are error vocabulary and domain type names, not database
  accesses. A dedicated gate check additionally forbids every `Db::conn()`
  call in the GTK frontend; for the remaining crates, `pub(crate)` enforces
  the boundary at compile time. The budget is ceiling **and** floor.
- The 575 `borrow()` calls go away. The project's most frequent panic class is
  structurally excluded for the database path, not merely rarer.
- `unchecked_transaction()` gives up Rust's compile-time protection against
  nested transactions: a nesting becomes a runtime error. The 25 affected
  places are checked individually during the migration for whether they call
  another transactional core function.
- A partial conversion is not a possible end state. As long as `conn()` is
  `pub`, the state is visibly unfinished; once it is `pub(crate)`, a half
  conversion does not compile. Two idioms side by side — part on `Db`, the
  rest on `&Connection` — cannot be merged by accident.
- Worker threads continue to open their own `Db` via the path instead of
  sharing one across threads. The handle does not change that.
- `reprise-cli`, `reprise-mcp` and `reprise-runtime` hang off the core surface
  as well and migrate along with it.

## Alternatives considered

- **Facade: the handle offers methods instead of a connection.** Rejected
  after measurement. A first estimate was at 58 necessary methods; in fact the
  frontend calls **172** distinct public core functions with a connection. A
  facade of that width would be a pure pass-through dummy, and the names are
  generic and spread across modules (`load` alone from 11 frontend files, plus
  `list`, `get_setting`, `is_enabled`), so a flat facade would have name
  collisions.
- **Closure access `db.with(|conn| …)`.** Rejected, because it does not solve
  the measured problem: the frontend still names `Connection`, only now in the
  closure parameter. The number barely drops, and the boundary remains a
  convention.
- **Leave everything as is and only lower the budget.** Rejected, because the
  number would then still measure something other than what its name says, and
  the panic class remains.
