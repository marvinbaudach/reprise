---
slug: android-saf-absence-misread-as-provider-failure
worktree: /home/marvin/Projects/reprise-android-saf-absence-misread-as-provider-failure
branch: feature/android-saf-absence-misread-as-provider-failure
phase: shipped
codex_session:
created: 2026-08-22
---
# Android reads a deleted file as a provider failure, never as absent

Diagnosis and device measurements:
`docs/plans/android-saf-absence-misread-as-provider-failure.HANDOFF.md`
(Pixel 10 Pro XL, app 0.1.25, 2026-08-22). This plan is the grilled result of
that handoff; where it departs from the handoff's recommendation, it says so and
gives the reason.

## The defect in one sentence

Android's SAF bridge has no way to say *"the provider answered, and the answer
is: it does not exist"* — every vanished document arrives as
`LibraryPathPresence::Unknown`, and `scanner_vanish.rs:171` correctly refuses to
write a missing verdict on `Unknown`, so deleted tracks stay `PRESENT` forever
and a rescan cannot help.

Confirmed side effect of the same gap: `read_analysis_sidecar`
(`mobile_import.rs:44-52`) guards on `error.kind() != NotFound`, but
`BridgedSource::open_read` can never produce `NotFound` — so every deleted
sidecar logs a warning the guard exists to suppress.

Confirmed consequence of the fix: the library window query is built on
`PRESENT` (`crates/reprise-core/src/queries/clauses.rs:26`,
`build_track_ids_query_base`), so a marked row leaves the list and the header
count drops. Writing the verdict *is* the visible fix, not just a bookkeeping
correction.

## Three layers

Each layer alone leaves a hole the handoff names.

| Layer | Where | What it buys |
|---|---|---|
| 1 — exception classification | `AndroidSafSource.kt` | The measured case: `enforceTree` wraps a `FileNotFoundException` in a `RuntimeException`. One call, no extra I/O. |
| 2 — a not-found error variant | `SafSourceError`, `source_error.rs` | `open_read` can finally produce `io::ErrorKind::NotFound`, so the sidecar guard stops misfiring. The channel layer 1 needs. |
| 3 — walk evidence | `scanner.rs`, `scanner_vanish.rs`, `LibrarySource` | Provider-independent confirmed absence: *the parent directory was listed and this child was not in it.* Zero extra provider calls. |

### Deviation from the handoff, decided in the grill

The handoff recommends the provider-independent fallback as *"one directory
listing per unresolved candidate"*. This plan implements the same **rule** from
different **evidence**: the scan already listed every reachable directory under
the root during its walk. Recording which directories it listed, and which
entries came out of them, answers "is the parent readable and does it not list
this child?" for every candidate at once, for zero extra provider round trips,
in Core code that unit-tests without any Android.

Both shapes need the same new primitive — a way to derive a `content://` URI's
parent address, because `Path::parent()` cannot — so the handoff's version is
not actually cheaper in moving parts.

What this costs: layer 3 only helps callers that ran a walk, i.e. the scan. A
`probe` in isolation still answers `Unknown` for a provider that signals absence
in a third way. The scan is where this bug lives, and the handoff itself
withdrew the app-start presence check.

## Hard constraints

Violating any of these re-opens the bug or creates a worse one.

1. **Never key detection on message text.** `"Failed to determine if … is child
   of …"` belongs to one provider on one Android version. Matching it is how
   this bug returns on the next device.
2. **Never widen "any `RuntimeException` means absent."** The
   Present/Unknown/Absent split is what the root guard relies on to tell "your
   library folder is unreachable" from "your library is empty".
3. **`listChildren` must keep failing loudly.** Giving it the same absence
   treatment would turn a *failed* listing into "this directory exists and is
   empty" — the exact false confirmation that would mass-mark a subtree. A
   failed listing stays a `LibraryWalkItem::Error`.
4. **Marking is one-way on Android.** No relink or purge path exists in the FFI;
   recovery is `pm clear org.reprise`, which also destroys scan permission,
   settings, queue and position. Every tie resolves toward `Unknown`.
5. **`Present` is never overridden.** Walk evidence may only upgrade `Unknown`.
   A file the source can see right now outranks a listing from earlier in the
   same scan.

---

## Tasks

Ordered so the tree compiles after every task. Rust before Kotlin: the Kotlin
`NotFound` throw site needs the UniFFI-*generated* variant to exist.

### T1 — `SafSourceError::NotFound` and its two mappings

`crates/reprise-android-ffi/src/source.rs`, `crates/reprise-android-ffi/src/source_error.rs`

```rust
/// The provider answered, and the answer is that the document does not
/// exist. Distinct from `Unknown`: this one licenses a missing verdict.
#[error("not found: {detail}")]
NotFound { detail: String },
```

- `source_io_error`: `NotFound => io::ErrorKind::NotFound`.
- `walk_error`: `NotFound => LibraryWalkErrorKind::Unknown`, **with a comment**
  recording why there is no new walk-error kind: `walk_error` is reached only
  from `list_children`, and constraint 3 forbids `listChildren` from ever
  producing `NotFound`. A `LibraryWalkErrorKind::NotFound` would ripple into the
  desktop enum and `scanner.rs`'s `ImportErrorKind` match for a value that
  cannot occur.

### T2 — `BridgedSource::probe` honours `NotFound`

`crates/reprise-android-ffi/src/source.rs`

```rust
Ok(None) => LibraryPathPresence::Absent,
Err(SafSourceError::NotFound { .. }) => LibraryPathPresence::Absent,
Err(_) => LibraryPathPresence::Unknown,
```

Kotlin's `probe` signals absence with `null`, but a provider path that throws
`NotFound` must not be downgraded.

The existing test `probe_keeps_confirmed_absence_distinct_from_provider_failure`
(`source.rs:399`) **stays valid and is extended, not weakened** — contrary to
the handoff's suspicion, it pins `Io → Unknown`, which is still exactly right.
Add a third arm: `NotFound → Absent`.

### T3 — `LibrarySource::parent_of`

`crates/reprise-core/src/library/source.rs`

```rust
/// The container `at` would sit in, as an address — **never a source query**,
/// and no statement that either `at` or the result exists.
///
/// A path-backed source answers with `Path::parent`. A DocumentsProvider
/// source cannot: a tree document URI's parent is not its `Path` parent, so it
/// rebuilds the address from the document id it encoded itself, and answers
/// `None` whenever it cannot do so with certainty. Callers must treat `None`
/// as "no evidence", never as "no parent".
fn parent_of(&self, at: &Path) -> Option<PathBuf> {
    at.parent().map(Path::to_path_buf)
}
```

The default covers `UnixLibrarySource` and every existing test double unchanged.

### T4 — the walk records its evidence

`crates/reprise-core/src/library/scanner.rs`, inside `scan_folder_inner`'s
visitor.

1. **Widen `observed_audio_paths` to `observed_paths`** and insert *every* entry
   path the walk delivers — files and directories alike — **immediately after
   `mobile_sync.observe(...)`, before the `!entry.is_file` and `is_audio_file`
   early returns** (today's insert sits after both, at line 341).

   Two reasons. Layer 3 must see a non-audio child of a listed directory:
   otherwise a track row whose extension is no longer in `is_audio_file` would
   be marked missing although its file is right there. And the existing
   "already delivered by this walk → known present" skip in `mark_vanished_with`
   is equally correct for any entry.

   **`audio_files_seen` stays exactly where it is** — the root guard's input
   must remain "did the walk find any *audio* file".

2. `observed_directories: HashSet<PathBuf>` — every entry with `!entry.is_file`,
   root entry included.

3. `failed_directories: HashSet<PathBuf>` — for every `LibraryWalkItem::Error`,
   insert `error.path` (falling back to `root` when `None`, matching the
   existing `err_path` handling right beside it) **and** `source.parent_of` of
   that path.

   The parent poisoning is load-bearing and was found in the grill. `walkdir`'s
   `error.path()` is *not* guaranteed to be a directory: a failed `DirEntry`
   (a stat error on a child) reports the **child's** path. Without poisoning the
   parent, a present-but-unstattable file would leave its directory counting as
   cleanly listed, and layer 3 would mark that file missing. Since the two cases
   are indistinguishable at this seam, distrust both. Cost: one directory loses
   layer-3 coverage per walk error — the safe direction.

Bundle for the mark phase:

```rust
/// What the walk proved about the shape of the tree, for the mark phase to
/// reason about candidates the walk never delivered.
pub(super) struct WalkEvidence {
    /// Every path the walk delivered — proof of presence.
    pub(super) observed: HashSet<PathBuf>,
    /// Directories the walk entered **and listed without error** — proof that
    /// a child's absence from `observed` is real and not a gap.
    pub(super) listed_directories: HashSet<PathBuf>,
}
```

`listed_directories = observed_directories \ failed_directories`, computed once
after the walk. The set difference is the whole safety argument: a directory the
source could not fully read contributes no evidence about anything inside it.

**The evidence is only handed over when the walk found audio:**

```rust
// A walk that saw no audio file at all is exactly the situation Android
// cannot distinguish from "the storage went away" — its residence token is
// derived from the tree URI string and confirms the root even when nothing
// is reachable, so the root guard cannot catch it. An empty walk is a
// question, not a proof: layer 3 stays silent and only a real `Absent`
// from the source still marks.
let evidence = (audio_files_seen > 0).then_some(WalkEvidence { .. });
```

No partial-walk hazard exists to guard against: the walk's only
`LibraryWalkControl::Stop` (`scanner.rs:628`) is followed by `return Err(...)`,
so the mark phase never runs on a truncated walk.

### T5 — walk-evidence absence in the mark phase

`crates/reprise-core/src/library/scanner_vanish.rs`

```rust
/// Confirmed absence derived from the walk that just ran, for a candidate the
/// walk never delivered.
///
/// Climbs from `path` toward `root` until it meets an ancestor directory the
/// walk listed without error. The child of that directory on the way down is
/// then decided by evidence: not in `observed` means the whole subtree from
/// there down does not exist, which is exactly `Absent`. Present in `observed`
/// means that container exists but the walk never got inside it — no evidence.
///
/// Returns `false` — never a missing verdict — for every uncertainty: no
/// evidence at all, no derivable parent, a parent outside `root`, or the climb
/// leaving `root` without meeting a listed directory.
fn absence_confirmed_by_walk(
    source: &dyn LibrarySource,
    evidence: Option<&WalkEvidence>,
    path: &Path,
    root: &Path,
) -> bool
```

Shape:

```
evidence?                                       // None -> false
child = path
repeat at most MAX_ANCESTOR_CLIMB times:
    parent = source.parent_of(child)?           // None -> false
    if !(parent == root || parent.starts_with(root)) { return false }
    if evidence.listed_directories.contains(parent) {
        return !evidence.observed.contains(child)
    }
    child = parent
return false                                    // depth cap reached
```

`MAX_ANCESTOR_CLIMB` is a named constant (64) purely so a source with a
pathological `parent_of` cannot spin.

In `mark_vanished_with` — which gains a `root: &Path` parameter and takes
`Option<&WalkEvidence>` in place of the bare `&HashSet`:

```rust
// This write needs confirmed absence. `Present` always keeps the row live.
// `Unknown` is not a verdict either — but the walk that just ran may hold the
// evidence the source itself could not produce, so ask it before giving up.
let absent = match source.probe(path, LibraryLinkMode::Follow) {
    LibraryPathPresence::Absent => true,
    LibraryPathPresence::Present(_) => false,
    LibraryPathPresence::Unknown => {
        absence_confirmed_by_walk(source, evidence, path, root)
    }
};
if !absent {
    continue;
}
```

`reclassify_missing_with` gets the same treatment for the same reason: it probes
identically, and a row stuck on `unknown` from an earlier scan deserves the same
correction. It already receives `root`; it needs the evidence.

**Logging.** Keep the message `"scan: marked vanished track missing"` byte for
byte — the device verification greps it. Add one field naming which evidence
licensed the write: `verdict = "probe"` or `verdict = "walk"`. That field is the
only way to tell the two layers apart in logcat, and the verification below
depends on it.

### T6 — `BridgedSource::parent_of`

`crates/reprise-android-ffi/src/source.rs`, `crates/reprise-android-ffi/src/lib.rs`

A tree document URI is

```
content://AUTHORITY/tree/<enc tree-doc-id>/document/<enc doc-id>
```

and Reprise builds every one of them itself via
`DocumentsContract.buildDocumentUriUsingTree`, which percent-encodes the `/`
inside a document id as `%2F` (a literal `%` in a file name encodes as `%25`, so
`%2F` is unambiguous). The parent address is the same URI with the last
`%2F`-separated segment of the **encoded** document id removed — no decode round
trip, no message text, no provider call.

Rules, all failing to `None`:

- the URI must contain `/document/`;
- the encoded document id must contain `%2F` (case-insensitive);
- when the resulting parent document id equals the **tree's own** document id,
  answer with the configured tree URI itself — **not** the `/document/` form.

That last rule is not cosmetic. `MusicLibrary::set_tree_uri` stores the *tree*
form as the scan root (`lib.rs:149`) and passes it to
`scan_folder_with_source_and_progress` (`lib.rs:167`), so the walk's root entry
and `listed_directories` carry the tree form while every child carries the
document form. Without the normalisation, every track in a top-level album
folder loses its evidence at the last step of the climb.

`BridgedSource` therefore needs its tree URI: add
`BridgedSource::with_tree_root(source, tree_uri)` and keep `new(source)`
delegating with `None`. One production call site (`lib.rs:157`) switches; the
six test sites in `source.rs` stay as they are.

A provider whose document ids are opaque blobs answers `None` and simply gets no
layer-3 coverage — the fail-safe direction.

### T7 — Kotlin: the confirmed-absence classifier

New file `android/app/src/main/java/de/reprise/spike/SafAbsence.kt`

```kotlin
/**
 * Whether this failure is the provider stating that the document does not
 * exist, as opposed to a failure to reach it.
 *
 * Android's DocumentsProvider.enforceTree() calls isChildDocument(), and
 * ExternalStorageProvider raises a RuntimeException whose *cause* is the
 * FileNotFoundException — measured 2026-08-22 on a Pixel 10 Pro XL. So the
 * whole cause chain is inspected and the message text never is: that string
 * belongs to one provider on one Android version, and matching it is how this
 * bug comes back.
 */
internal fun Throwable.confirmsAbsence(): Boolean
```

Walks `cause` with a visited-set (a self-referencing chain is legal in Java) and
a depth cap, returning true when any link `is FileNotFoundException`.

Deliberately **not** matched: bare `RuntimeException`, `IllegalStateException`,
`IllegalArgumentException`, `SecurityException`. A `SecurityException` is
checked *before* this classifier at every call site, so a revoked grant can
never read as a deletion even if a provider nests an FNF inside it.

It lives in its own file precisely so it unit-tests as plain Kotlin — no
Robolectric, no `ContentResolver`.

### T8 — Kotlin: `probe` and `openReadFd`

`android/app/src/main/java/de/reprise/spike/AndroidSafSource.kt`

**`probe`** — restructure so a **null cursor** and an **empty cursor** are
distinguishable. They are not today: `resolver.query(...)?.use { ... } ?: throw`
collapses both into the same `null`, so the second hole the handoff names is
literally unexpressible in the current shape.

- null cursor → `throw SafSourceException.Unknown(...)` (genuine unknown);
- empty cursor (`moveToFirst()` false) → return `null` (absent).

Catch order, which is load-bearing:

1. `SecurityException` → `PermissionDenied` (it is itself a `RuntimeException`,
   so it must come first);
2. `SafSourceException` → rethrow;
3. `IOException` → `null` if `confirmsAbsence()`, else `Io`;
4. `RuntimeException` → `null` if `confirmsAbsence()`, else `Unknown`.

The existing bare `catch (_: FileNotFoundException)` disappears into branch 3,
which subsumes it.

**`openReadFd`** — same classification, reported as
`SafSourceException.NotFound(error.detail())`.

**`listChildren`** — unchanged, per constraint 3. Add a comment saying so and
naming the mass-marking hazard, so the next reader does not "fix" the apparent
inconsistency.

---

## Tests

**Kotlin, plain JVM** — `android/app/src/test/java/de/reprise/spike/SafAbsenceTest.kt`

1. The **real measured shape**: `RuntimeException("Failed to determine if … is
   child of …: java.io.FileNotFoundException: Missing file for …")` **with a
   `FileNotFoundException` cause** → true. Without this test the exact
   misclassification comes straight back.
2. The **same message text with no FNF anywhere in the chain** → false. This is
   the test that keeps string matching from creeping back in.
3. A bare `FileNotFoundException` → true.
4. A plain `RuntimeException` and an `IllegalStateException` → false.
5. A `SecurityException` wrapping an FNF → the classifier says true; the *call
   site* order is what keeps it a `PermissionDenied`. Assert the classifier
   honestly and state the ordering contract in the test name.
6. A self-referencing cause chain terminates.

**Kotlin, Robolectric** (available: `org.robolectric:robolectric:4.16.1`,
`unitTests.isIncludeAndroidResources = true`)

7. Empty cursor → `probe` returns `null`.
8. Null cursor → `probe` throws `Unknown`.

If a `ContentResolver` double proves disproportionate here, fold 7–8 into the
Rust fakes instead and say so in the commit — but do not drop them.

**Rust — `crates/reprise-android-ffi/src/source.rs`**

9. `NotFound` from `probe` → `Absent`; `Io` → `Unknown` (extends the existing
   test at line 399).
10. `open_read` against a source whose `open_read_fd` returns `NotFound` →
    `io::ErrorKind::NotFound`.
11. `parent_of` against a real tree URI: a nested document → its parent document
    URI; a **top-level** document → the **tree-form** root URI; a URI without
    `/document/` → `None`; an opaque document id without `%2F` → `None`.

**Rust — Core**

12. `read_analysis_sidecar` with a source returning `io::ErrorKind::NotFound`
    logs **nothing** and returns `None` — the guard at `mobile_import.rs:44-52`
    finally reachable.
13. `absence_confirmed_by_walk`, one case each: parent listed + child unobserved
    → true; parent listed + child observed → false; parent not listed → false;
    grandparent listed with the intermediate directory unobserved → true (the
    whole-folder-deletion case); `parent_of` → `None` → false; parent outside
    `root` → false; `evidence` is `None` → false.
14. **The third-provider pair the handoff demands.** A fake source whose
    vanished document yields neither an FNF nor an empty cursor — it answers
    `Unknown` — and whose parent directory the walk listed: `mark_vanished_with`
    must mark it. With the parent **not** listed, it must not. This pair covers
    file managers and third-party providers and proves layer 3 does real work.
15. **The parent-poisoning regression.** A walk error naming a *file* must cost
    that file's directory its layer-3 coverage: a candidate in the same
    directory, probing `Unknown`, must **not** be marked.
16. **The empty-walk guard.** `audio_files_seen == 0` with a listed root and
    unobserved album folders must mark nothing through layer 3. This is the
    mass-marking hazard; assert it explicitly.
17. `unknown_probe_does_not_mark_vanished_track_missing`
    (`scanner_vanish.rs:324`) keeps its assertions **unchanged** — only its call
    site gains the new `root` argument and `None` evidence. If an assertion has
    to move, the change went too far.
18. `mark_vanished_with` does mark once the source reports `Absent` — guards the
    seam from the other side.
19. A scan whose walk delivered nothing under an unreachable root still reports
    `RootUnavailable` and marks nothing.

## Gate

`scripts/check-merge-readiness.sh` in the strand's own worktree. The stages that
actually bear on this change:

- `cargo clippy --locked --all-targets --workspace -- -D warnings`
- `cargo test --locked --workspace --exclude reprise-platform-linux`
- the Android unit suite (`scripts/check-android-suite.sh`; needs JDK 21 and the
  UniFFI bindgen step)
- `cargo doc` with `RUSTDOCFLAGS="-D warnings"` — every new public item above
  carries a doc comment

Read the gate's verdict directly. Never through a pipe: `skript | tail` reports
`tail`'s exit status, which is always 0.

## Device verification — the only thing that closes this

Unit tests cannot prove the wrapped-exception shape, and they cannot prove that
the real `parent_of` meets the real walk evidence. Only the phone can.

**The order matters, because marking is one-way.** Both arms are aimed at rows
that are already dead, so the proof costs nothing irrecoverable.

**Arm 1 — layer 3 on real hardware.** Build a *throwaway* APK with the
`confirmsAbsence()` branch in `probe` commented out, so layer 1 can resolve
nothing and every candidate must go through the walk evidence. Install, ⋮ →
Rescan, capture `adb logcat -s Reprise:V`.

- expect `scan: marked vanished track missing … reason=deleted verdict=walk` for
  the 17 emptied album folders;
- **control arm:** a track whose file is still present must **not** be marked.
  Without this arm the run measures nothing.

Then discard the patch. It never reaches a commit.

**Arm 2 — layer 1 on real hardware.** Install the regular build. Arm 1 already
consumed the existing phantoms, so create a fresh one: `adb shell rm` one track
under `/sdcard/Music/…` — the same thing a file manager does. Pick a track the
desktop can re-sync afterwards. Rescan.

- expect `… reason=deleted verdict=probe` for exactly that track;
- **control arm:** again, a still-present track must not be marked.

This arm doubles as the handoff's third-party-deleter arm.

**Arm 3 — the visible symptom.** After arm 1, the header count must have dropped
from 780 and `999 • Anchors & Hearts • Deathlist` must be gone from the list.
*Establish first whether `Music/Reprise-YouTube` (46 files) is inside the scanned
tree* before pinning an exact expected number.

**Arm 4 — the sidecar.** Across arm 1's logcat, `could not read analysis sidecar`
must not appear for a deleted sidecar.

**Recovery, if an arm goes wrong.** There is no relink or purge in the FFI.
`pm clear org.reprise` is the only way back, and it also destroys scan
permission, settings, queue and position. Never seed deletions beyond arm 2's
single re-syncable track.

## Follow-up work this plan deliberately does not do

- **Android's residence token is inert.** `stableTreeToken` (`AndroidSafSource.kt`)
  hashes the authority and the tree document id — it is derived from the URI
  string and never from the state of the storage, so
  `any_candidate_confirms_root_with` confirms the root even when nothing is
  reachable and the root guard cannot fire on Android. Today that is harmless
  (nothing is ever marked); after this plan the `audio_files_seen > 0` gate is
  what stands in for it. Making the token reflect reachability is a plan of its
  own, with a migration question for the `tracks.device` values already on
  devices.
- **Automatic rescan after a sync or on app start.** Genuinely missing
  (`rescan()` is wired only to `LibraryFrame.kt:113` and
  `settings/SettingsNavigation.kt:79`; `MainActivity.onStart`/`onResume` trigger
  nothing), genuinely worth having *because* deletions also come from file
  managers — and genuinely not this bug: a manual rescan was measured and
  changed nothing.
- **A cheap presence check on app start.** It would call the same `probe`.
  Proposed before the measurement, then withdrawn.
- **A Media3-side fix** (mark the row missing when playback fails). Treats the
  symptom, leaves untouched phantom rows in the list, does nothing for the
  sidecar path. Reasonable as a later second layer.
- **`reprise-track-metadata.rpl`'s stale date** — unrelated; noted only so it is
  not rediscovered as a lead.
- **Cover download on Android** — raised in the same session, a different
  subsystem (`cover_download`, its own FFI surface, Compose UI), zero file
  overlap. Its own plan.

## Parallelität

**No cut. One strand.**

The reason is a compile-order chain, not a preference:

- T8 (Kotlin throwing `SafSourceException.NotFound`) cannot compile until T1 has
  added the variant to `SafSourceError` — the Kotlin type is UniFFI-*generated*
  from the Rust enum.
- T6 (`BridgedSource::parent_of`) cannot compile until T3 has put `parent_of` on
  the `LibrarySource` trait.
- Tests 13–16, the only tests that prove layer 3 works, need T4's evidence
  plumbing and T5's climb in the same tree.

Two cuts were considered and rejected in the grill:

1. **`android/**` vs. `crates/**`.** The file groups are disjoint and both
   halves compile alone, but the Kotlin strand would ship without the `NotFound`
   throw site and the Rust strand would add a variant nothing produces — so the
   sidecar-warning fix, one of the three holes this plan exists to close, would
   land in *neither* strand and become a post-merge task. A cut whose payoff is
   only post-merge is worse than no cut.
2. **`reprise-core` vs. bridge (`reprise-android-ffi` + `android`).** Fails on
   `parent_of`: the trait method and its only real implementation are one design
   decision, and test 11 pins an address contract that means nothing without
   T5's climb.

Roughly 400 lines across 9 files. Splitting buys no wall-clock worth the seam
risk, and this seam is exactly the kind the cap exists to protect: a wrong
Present/Unknown/Absent decision mass-marks a library that cannot be un-marked.

**File ownership (the single strand owns all of it):**

```
android/app/src/main/java/de/reprise/spike/SafAbsence.kt            (new)
android/app/src/main/java/de/reprise/spike/AndroidSafSource.kt
android/app/src/test/java/de/reprise/spike/SafAbsenceTest.kt        (new)
android/app/src/test/java/de/reprise/spike/AndroidSafSourceTest.kt  (new, tests 7–8)
crates/reprise-android-ffi/src/source.rs
crates/reprise-android-ffi/src/source_error.rs
crates/reprise-android-ffi/src/lib.rs
crates/reprise-core/src/library/source.rs
crates/reprise-core/src/library/scanner.rs
crates/reprise-core/src/library/scanner_vanish.rs
```

**Post-merge cross-checks:** none — there is no second strand to compare
against. The device verification above is the post-merge step, and it reads only
the running app.
