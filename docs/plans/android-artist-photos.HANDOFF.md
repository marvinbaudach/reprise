# Handover — Android artist photos, 14.08.2026 16:25

**Mother plan:** `docs/plans/android-artist-photos.md` — two strands, `merge_order: core,ui`.

| Strand | State |
| --- | --- |
| `core` (PR 1, pure Rust) | **shipped.** PR #482, merged as `8b87ae8ada`. Worktree and branch removed by `land.sh`. |
| `ui` (PR 2, Kotlin + one Rust enum) | **in flight.** Codex is working tasks 1–9 right now. |

The mother plan's checkpoint between the two landings is **cleared**: dev CI run
`31803151200` on `8b87ae8ada` finished `success` with both `Android JVM unit
suite` and `Quality gate` green. That is the proof that the additive FFI
extension left the Kotlin translation compiling.

---

## What is running right now

**Codex, headless, on the `ui` strand.**

- Worktree: `/home/marvin/Projects/reprise-android-artist-photos-ui`
- Branch: `feature/android-artist-photos-ui` — 24 commits ahead of `origin/dev`
- PID file: `<scratchpad>/codex-ui.pid`, log `<scratchpad>/codex-ui.log`
- Prompt it was given: `.pipeline-task.md` in that worktree (gitignored)
- Its summary will land in `.pipeline-codex.md` when it finishes
- A wake lock named `artist-photos-ui` is held — **release it when the run ends**
  (`wake-lock release artist-photos-ui`)

Committed so far (task 1 through 6):

```
be0da6b413 feat(android): add online sources settings page          task 6
e3ceed6db7 feat(android): add artist portrait detail header         task 5
47bfa8181a feat(android): show artist avatars in browse rows        task 4
c1b5a78763 feat(android): connect artist portrait bridge            task 3
fe9c7dc791 feat(android): add artist artwork resolution path        task 2
531d28a23b feat(android-ffi): add artist detail artwork size        task 1
0535b861a7 docs: pin the ui strand worktree and branch              (setup)
```

Uncommitted at the time of writing: `BrowseScreen.kt`, `MainActivity.kt`,
`settings/SettingsNavigation.kt`, `settings/SettingsOverview.kt`,
`MainActivitySettingsNavigationTest.kt` — that is task 7 in progress.

---

## The four things a fresh session will get wrong

### 1. `ui` is stacked on `core`, not branched off `dev`

The strand file opens with "PR 1 must have landed first". It had not, so the
worktree was branched off the `core` branch tip `c0da59c9f7` instead. That is why
the UniFFI bindings generate with the new methods at all.

`core` then landed **squashed**. So the 14 `core` commits still sit in this
branch's history while `dev` carries them as one commit. Therefore:

```sh
# CORRECT — drops core's commits, keeps only ui's
git rebase --onto origin/dev c0da59c9f7 feature/android-artist-photos-ui

# WRONG — replays core's 14 commits onto a dev that already has them
git rebase origin/dev
```

`c0da59c9f7` stays reachable through this branch, so the reference does not rot.
Expect one small conflict on `docs/plans/android-artist-photos-core.md`: this
branch has it at `phase: refactored`, dev has it at `phase: shipped` — take dev's.

### 2. Task 10 was deliberately excluded from the Codex run

Task 10 is a device visual pass. A headless Codex run takes over a connected
device if it is allowed to, so the prompt explicitly forbids `adb`, emulators and
screenshots and tells it to stop after task 9. **Task 10 is still owed** and has
to be done under supervision. Its own preparation trap is worth re-reading: the
list never fetches (`allowFetch = false`), so on a fresh install there is no
portrait to photograph — open three or four artists first, then take the shot.

### 3. `land.sh` names the wrong CI run

It prints `gh run list --branch dev --limit 1`, which returns whichever workflow
reports back first. On this repo that is **Cross-target**, a cargo cross-compile
check that proves nothing about Kotlin. The gate lives in `.github/workflows/ci.yml`
(`Android JVM unit suite` + `Quality gate`) and is a **separate run on the same
SHA**. Find it with:

```sh
gh run list --branch dev --limit 8 \
  --json databaseId,workflowName,conclusion,headSha \
  -q '.[] | "\(.databaseId)\t\(.conclusion)\t\(.headSha[0:10])\t\(.workflowName)"'
```

Also: nearly every dev CI run on this repo ends `cancelled`, because the
concurrency group kills it the moment the next merge lands. `cancelled` is not
red — the evidence is the next completed run that still contains the commit.

### 4. Environment facts that are measured, not negotiable

- **JDK 21 or the Android suite dies.** System default here is JDK 26 and it
  kills Robolectric. `JAVA_HOME=/usr/lib/jvm/java-21-openjdk` before *every*
  Gradle call.
- **`BUILD SUCCESSFUL` is not proof.** Gradle reports `:app:testDebugUnitTest`
  up-to-date, exits 0 and runs nothing. The verdict is the freshness of
  `android/app/build/test-results/testDebugUnitTest/*.xml`, which
  `scripts/check-android-suite.sh:33-77` checks.
- `reprise-android-ffi` tests depend on `readdir` order → prefix `TMPDIR=/tmp`.
- Judge cargo runs by `grep -c '^test result: FAILED' <log>`, never by the last
  line — `cargo test --exact` easily runs nothing.
- `reprise-gnome` has no `--lib`; it runs as `--bin reprise`.
- `.pipeline-codex.md` is **tracked** despite being in `.gitignore` (the ignore
  line does not apply retroactively). It goes dirty on every Codex run and
  `land.sh` refuses a dirty tree — `git checkout -- .pipeline-codex.md` first.

---

## Review findings that were deliberately NOT applied

Both reviewers passed `core` with no critical or high findings. Four items were
raised and consciously left alone — do not "fix" them without a decision:

1. **`artist_portrait_fetch` answers every `PortraitError` with `Ok(None)`.** A
   network outage and "this artist has no photo" are indistinguishable at the FFI
   boundary. This is what the plan specifies (core task 5, step 4). **Consequence
   for `ui`:** the UI cannot offer a "retry" affordance. If that turns out to
   matter on screen, it is a design decision for this strand, not a bug in `core`.
2. `path.to_string_lossy().into_owned()` in `artist_portrait.rs` — lossy on
   non-UTF-8 cache paths. Judged acceptable: the paths are the app's own cache
   plus a hex filename.
3. Closing the gate while a fetch is already in flight does not abort it. The
   alternative is holding the DB lock across a network call, which core task 5
   forbids outright.
4. `image::load_from_memory` has no explicit pixel/memory `Limits`. Pre-existing
   shared path, download already capped at 20 MiB and host-pinned to
   `*.dzcdn.net`. Backlog, not this plan.

---

## What `core` actually shipped, in one paragraph

`ThumbnailSize::MobilePortrait` (640 px); `artist_portrait::load_or_fetch_in`
taking an explicit cache directory; `MusicLibrary::artist_portrait_cached`
(cache-only and structurally incapable of a network call — it takes no lock at
all); `MusicLibrary::artist_portrait_fetch` (NET-1a gated, lock released before
the blocking fetch); and the `online_sources_enabled` / `set_online_sources_enabled`
pair whose write order is load-bearing. During review it was rebased onto #480,
which replaced `LibraryState`/`lock()` with `writer()` / `reader()` /
`configured_tree()`: gate reads went to `reader()`, switch writes to `writer()`.

---

## Next steps, in order

1. Wait for the Codex `ui` run. Read `.pipeline-codex.md`, then **verify rather
   than believe it** — check the commits, the `suites=… failures=0 errors=0 …
   verdict=fresh` line, and that `android/app/build/test-results/` is fresh.
2. `/check` on the `ui` worktree — Kotlin/Compose changes plus one Rust file, so
   `rust-reviewer` and a generic Sonnet/high reviewer for the Kotlin. Then
   `/refactor` with whatever is accepted.
3. **Task 10** — the supervised device pass. Release build for frame times; a
   debug build and the emulator cannot answer that question, that is measured.
4. Rebase with `--onto` as in point 1 above, push, PR against `dev`, `land.sh`.
5. After landing, run the mother plan's remaining post-merge item and release the
   wake lock.

---

## Nachtrag — 14.08.2026, 18:40, beim Landen des `ui`-Strangs

Der Strang ist seit diesem Nachtrag durch `check` und `refactor` gegangen; was
oben unter „Next steps" als Schritt 1 und 2 stand, ist erledigt.

- **Review:** fünf Reviewer über `c0da59c9f7..HEAD`, kein kritischer Befund.
  Angenommen und von Codex behoben: eine Zeile, die einmal auf das Album-Cover
  zurückgefallen war, zeigte das später geholte Porträt nie mehr (der
  Kotlin-Cache schloss sie kurz); der Online-Sources-Schalter behielt seinen
  UI-Zustand auch bei fehlgeschlagenem Schreiben; `usesCleartextTraffic` fehlte;
  ein FFI-Test trug einen Namen, den er nicht einlöste.
- **Bewusst abgelehnt:** den Schalter zusätzlich in Kotlin abzufragen
  (`allowFetch = true` bleibt fest) — das Gate liegt in Rust und wird pro Aufruf
  frisch geprüft. Zwei Orte für dieselbe Entscheidung sind der Anfang der Drift.
  Ebenfalls abgelehnt: die Lane des Porträt-Fetch zu ändern. Sie teilt sich
  `fullSizeWorker` mit dem Now-Playing-Cover, ist aber durch 15 s `HTTP_TIMEOUT`
  gedeckelt — ein Punkt für die Beobachtung am Gerät, kein Defekt.

**Diese Datei war zwischenzeitlich weg.** Codex hat beim Aufräumen der Historie
den Commit fallen lassen, der sie angelegt hatte (`97be7d2880`), und damit auch
die Datei auf der Platte. Sie ist aus dem Objektspeicher zurückgeholt. Wer einen
Codex-Lauf auf einen Branch lässt, in dem fremde Commits liegen, sollte danach
`git log` gegen den Stand davor halten — ein Commit weniger fällt sonst nicht
auf.

**Was weiterhin offen ist:** Aufgabe 10, die Sichtprüfung am Gerät. Sie ist
durch das Landen NICHT erledigt und braucht ein echtes Telefon plus
Release-Build. Ihre Vorbereitungsfalle steht oben unter Punkt 2 und gilt
unverändert.
