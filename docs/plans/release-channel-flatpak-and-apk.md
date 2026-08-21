---
slug: release-channel-flatpak-and-apk
worktree: /home/marvin/Projects/reprise-release-channel-flatpak-and-apk
branch: feature/release-channel-flatpak-and-apk
phase: shipped
codex_session:
created: 2026-08-21
---
# Release channel: Flatpak bundle and signed APK on every promotion to main

## Goal

Every weekly promotion of `dev` into `main` publishes one GitHub Release
carrying both applications: a single-file Flatpak bundle for the GNOME desktop
app and a signed universal APK for the Android app. Today the repository has no
tags and no release workflow, so the sidebar reads "No releases published".

## Measured baseline (2026-08-21, from `git show origin/dev:<path>`)

`origin/main` already contains every `origin/dev` commit and is four merge
commits ahead. The checkout this plan was drafted in was seven patch versions
stale, so every number below was read from `origin/dev`, not from a worktree.

| Fact | Value | Source |
|---|---|---|
| Desktop version | `0.1.42` | `Cargo.toml` `[workspace.package]` |
| Android version | `versionName 0.1.29`, `versionCode 29` | `android/app/build.gradle.kts:27-28` |
| Meson project version | `0.1.1` | `meson.build:4` — stale by 41 patch steps |
| Newest AppStream `<release>` | `0.1.1`, 2026-07-25 | `data/io.github.marvinbaudach.Reprise.metainfo.xml:114` |
| About dialog version | `CARGO_PKG_VERSION` (unaffected) | `crates/reprise-gnome/src/ui/about.rs:71` |
| Release signing | `signingConfigs.getByName("debug")` | `android/app/build.gradle.kts:53` |
| `applicationId` | `org.reprise` | `android/app/build.gradle.kts` |
| Mobile licence constant | `"All Rights Reserved"` | `android/app/build.gradle.kts:31` |
| R8 keeps for JNA/UniFFI | present and correct | `android/app/proguard-rules.pro` |
| Android ABIs | `arm64-v8a`, `x86_64` | `android/app/build.gradle.kts:38-39` |
| Flatpak manifest | GNOME Platform/SDK 50, offline Cargo | `io.github.marvinbaudach.Reprise.yml` |
| Android CI coverage | `cargo check` only, no APK is built anywhere | `.github/workflows/cross-target.yml:118` |
| CI on main | `push: branches: [main]`, aggregate job `Quality gate` | `.github/workflows/ci.yml:3-8, 253` |
| Default branch | `main` (so `workflow_dispatch` works once landed) | `gh repo view` |
| Commit mix, last 105 non-merge commits on main | android-only 10, desktop-only 26, both 25, neither 44 | measured |

The last row is why one shared release works: android-only work exists but is a
tenth of the commits, and a weekly promotion bundles enough of them that the
desktop version moves essentially every time.

## Decisions (owner, 2026-08-21)

| Question | Decision |
|---|---|
| APK licence | **GPL-3.0-or-later**, same as the repository |
| `applicationId` | change to **`io.github.marvinbaudach.reprise`** now, while it is still free |
| Release shape | **one** release per promotion, tag `v<desktop-version>`, both assets |
| Trigger | push to `main`, but build **only when `Quality gate` is green on that SHA** |
| Release text | curated: `CHANGELOG.md` (full, English) + metainfo `<release>` (short, EN+DE) |
| Where the text is enforced | in the release workflow only — **never** in `Quality gate` on a dev merge |
| Partial failure | **both or nothing** — draft first, verify, then publish |
| Linux artifact | `Reprise-<v>.flatpak` bundle; no self-hosted Flatpak repo (named follow-up) |
| Android artifact | one universal APK (arm64-v8a + x86_64), signed with the owner's upload key |
| Flathub | not a target (AI-authorship policy since 2026-05-29); nothing here prepares a submission |

Two corrections this plan makes to its own first draft, both from the grill:

- The consistency gate must **not** demand a changelog entry on every dev merge.
  `bump-version.sh` raises the patch version on every landing that touches
  desktop paths — 41 steps since `0.1.1`. A gate in `Quality gate` would demand a
  bilingual paragraph per pull request and would be switched off within a day.
  Only the equality of the Meson and Cargo versions belongs there; that one stays
  green by construction once `bump-version.sh` writes both.
- Two separate release trains (one per app) were dropped in favour of one shared
  release. The cost is accepted and stated: a promotion that carries **only**
  Android changes leaves the desktop version still, and then nothing is
  published until the next desktop bump. The measurement above says that is rare.

## Owner pre-conditions before the first real release

Implementation does not depend on these, but publication does. None of them are
Codex tasks.

1. **Create the upload keystore and set the secrets.** Exact commands go into
   `docs/releasing-android.md` (task 8). Four secrets:
   `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`,
   `ANDROID_KEY_PASSWORD`. Losing this keystore means no installed copy of the
   app can ever be updated again — it belongs in a password manager *and* an
   offline backup before the first upload.
2. **Fill `android/signing/upload-key-sha256.txt`** with the real certificate
   fingerprint. While it holds the placeholder, the release job fails with a
   message saying so. This is deliberate: it is what turns "a secret was set"
   into "the artifact carries the identity we expect".
3. **Write the first `CHANGELOG.md` and metainfo entry** for the version being
   released.

## Design

### Trigger: promotion, gated on green

`.github/workflows/release.yml`:

- `on: push: branches: [main]` — the weekly promotion is the event.
- `on: workflow_dispatch:` with a `dry_run` boolean input, default **true**.
- `on: pull_request:` with a `paths` filter covering this workflow, `flatpak/**`,
  `android/**`, `io.github.marvinbaudach.Reprise.yml`, `meson.build`,
  `Cargo.lock`, `data/*.metainfo.xml` — build-only, never publishes.
- `concurrency: group: release-${{ github.ref }}`, **`cancel-in-progress: false`**.
  A publishing run must never be killed halfway by the next push.
- `permissions: contents: read` at workflow level; `contents: write` only on the
  publishing job.

The first job, `gate`, decides everything downstream and publishes its reasoning
into the run summary:

1. Read the desktop version from `Cargo.toml` `[workspace.package]` and the
   Android `versionName`/`versionCode` from `android/app/build.gradle.kts`.
   Reuse `scripts/bump-version.sh current` for the desktop number rather than a
   second parser — the script already knows that the first `version =` in
   `Cargo.toml` is not the right one.
2. **Wait for `Quality gate` on this exact SHA** via
   `gh api repos/{repo}/commits/<sha>/check-runs`, polling with a bounded
   timeout. Three states that are easy to get wrong and must be handled by name:
   - `in_progress` / `queued` → keep waiting. A running check is not a red one.
   - `cancelled` → do not build. `ci.yml` runs in a concurrency group with
     `cancel-in-progress: true`, so a second push to `main` kills the first run,
     and a killed run reports `cancelled`, which reads exactly like a test
     failure. Report it as "superseded", not as a regression.
   - `skipped` → not green. Do not build.
   Only `success` proceeds.
3. Ask whether the tag `v<desktop-version>` already exists
   (`gh api repos/{repo}/git/ref/tags/v<version>`, 404 = publish). Tag absence,
   not a diff against the previous commit, is the condition: it is idempotent
   across re-runs, it survives the merge-commit shape of a promotion, and a run
   that failed halfway retries on the next push with nothing to clean up.
4. Outputs: `publish`, `tag`, `desktop_version`, `android_version`,
   `android_code`.

When the gate declines (not green, or tag already present) the run **succeeds**
with an explanatory summary rather than failing. A broken `main` already shows as
a red CI run; a second red run for the same cause is noise. A red *release* run
therefore always means the release machinery itself broke.

### Both or nothing

Jobs `flatpak` and `apk` run in parallel, both `needs: gate`. The `publish` job
`needs: [gate, flatpak, apk]` and runs only when both succeeded and `dry_run` is
false. Publication is three steps, in this order:

1. `gh release create <tag> --draft --target <sha> --title … --notes-file … <assets…>`
2. verify via `gh release view --json assets` that both assets are attached and
   non-zero in size
3. `gh release edit <tag> --draft=false --latest`

GitHub creates the git tag only when a draft is published, so a failure anywhere
before step 3 leaves **no tag** and the next push retries cleanly. An
`if: failure()` step deletes the stale draft.

> Assumption to verify during implementation, not to assert: that a *draft*
> release truly leaves no tag ref behind. Confirm it in the dry-run dispatch
> (verification step 5) by creating and deleting a draft and checking
> `git ls-remote --tags`. If GitHub does create the ref, fall back to explicit
> tag deletion in the failure step.

### Desktop job

Runs on `ubuntu-24.04` **directly, not in a container** — `flatpak-builder`
needs user namespaces, and this repository has already hit a CI container that
refuses them.

1. Add the Flathub remote; install `org.gnome.Platform//50`, `org.gnome.Sdk//50`
   and `org.freedesktop.Sdk.Extension.rust-stable`.
2. `flatpak-builder --repo=repo --force-clean build-dir io.github.marvinbaudach.Reprise.yml`
3. `flatpak build-bundle repo Reprise-<version>.flatpak io.github.marvinbaudach.Reprise`
4. `sha256sum` beside it; upload both as a workflow artifact.

The manifest's `type: dir, path: .` source is exactly right for CI and stays
unchanged. Expect a long job — a full Rust release build inside the sandbox. Add
no caching yet; record the measured wall clock in the PR and decide afterwards.

### Android job

1. JDK 21 (the toolchain constraint that already governs the Android suite),
   Android SDK, NDK.
2. `scripts/android-build.sh` **twice**, once per ABI — the script takes exactly
   one ABI per run from `ANDROID_TARGET` / `ANDROID_ABI` / `ANDROID_API`, and the
   second run regenerates the UniFFI bindings from the second `.so`. Both
   libraries end up under `android/app/src/main/jniLibs/<abi>/`, so one universal
   APK carries both. One file that works on phones and on the emulator beats two
   splits a tester can pick wrong from; record the measured size.
3. `./gradlew :app:assembleRelease` with `REPRISE_REQUIRE_RELEASE_SIGNING=1`.
4. `apksigner verify --print-certs`; assert the certificate SHA-256 equals
   `android/signing/upload-key-sha256.txt`. A wrong, rotated or truncated key
   still produces a *valid* signature — over the wrong identity. Only this
   comparison catches that, and it catches it before publication instead of
   after the first user cannot update.
5. `aapt2 dump badging`; assert `versionName` and `versionCode` equal what the
   gate job read from `build.gradle.kts`.
6. `sha256sum`; upload as a workflow artifact.

The keystore arrives as `ANDROID_KEYSTORE_BASE64`, is decoded into a
`RUNNER_TEMP` path, never into the workspace, and is removed in an `always()`
step. No secret value is ever echoed — `base64 -d` writes to a file.

### Signing configuration in Gradle

`android/app/build.gradle.kts` gains a `release` signing config resolved in this
order: the four `REPRISE_KEYSTORE_*` environment variables → a gitignored
`android/keystore.properties` → nothing. When nothing is found:

- `REPRISE_REQUIRE_RELEASE_SIGNING=1` set (the CI job always sets it) →
  `error(...)`, failing the build with a message naming the missing variable;
- otherwise → fall back to the debug config exactly as today, so a local
  `assembleRelease` for size measurement keeps working.

### Release text

One curated source per audience, checked for presence before anything is built:

- `CHANGELOG.md` at the repository root — a section per released version,
  English, the full entry. Its section for the version being released **is** the
  GitHub release body, after a generated header (see below).
- `data/io.github.marvinbaudach.Reprise.metainfo.xml` — a `<release>` block for
  the same version with both `<p>` and `<p xml:lang="de">`, matching the existing
  bilingual style. This is what every software centre shows.

The generated header prepended to the release body states, every time:

- both version numbers, so nobody reads the APK as the desktop version;
- the install line **including the GNOME 50 runtime** — a bundle fails to install
  on a machine that has never seen a Flatpak, and the error does not say why;
- that the bundle does **not** self-update, and that a self-hosted repository is
  the named follow-up;
- that the Flatpak ships **without stems separation** (no onnxruntime; the Rust
  side dlopens it and degrades to "feature unavailable"), while AUR, COPR and
  local builds keep the feature;
- Android minimum 8.0 (`minSdk 26`), and that the Android app is GPL-3.0-or-later
  like the rest.

### Version metadata becomes maintained instead of hoped-for

- `scripts/bump-version.sh` learns to write `meson.build`'s `version:` whenever it
  writes the desktop version, in both `set` and `--base` mode, under the same
  "never lower a version" rule as the existing writers. `meson.build` is already
  classified as a desktop-selecting path in the script's own path table.
- `scripts/check-release-metadata.sh` (new) has two modes:
  - `--gate` — only "Meson version == Cargo workspace version". Wired into
    `ci.yml` so it runs on every PR. Cheap, and green by construction.
  - full (default) — additionally requires a `CHANGELOG.md` section and a
    bilingual metainfo `<release>` for the workspace version. Wired into
    `scripts/check-release.sh` and called by the release workflow's gate job.

## Tasks

Order matters: 1–3 make the repository releasable, 4–6 build the machinery, 7–9
document it.

**1. Repair the desktop version metadata.** Set `meson.build`'s `version:` to the
current `[workspace.package]` version — read it at implementation time, do not
copy `0.1.42` from this plan; the branch will have moved. Add a `<release>` entry
for that version at the top of the metainfo release list, dated the
implementation date, with `<p>` and `<p xml:lang="de">` describing what happened
since `0.1.1`, derived from the git log rather than invented. Keep the `0.1.1`
and `0.1.0` entries. `appstreamcli validate` must stay exactly as green as it is
today; the two informational conditions documented in `RELEASING.md` remain the
only ones accepted, and no new one may appear.

**2. Create `CHANGELOG.md`.** A section for the current workspace version and one
for each already-released version (`0.1.1`, `0.1.0`, from the metainfo). English,
newest first, version and date in the heading. Content for the current section
comes from the git log since `0.1.1`, grouped by area — not a raw commit dump.

**3. Teach `scripts/bump-version.sh` about `meson.build`,** and add
`scripts/check-release-metadata.sh` with the two modes above. Wire `--gate` into
`ci.yml` so it runs on every pull request, and the full mode into
`scripts/check-release.sh`. Failure messages name the file and both values.

**4. Android identity and licence.** In `android/app/build.gradle.kts`: set
`applicationId = "io.github.marvinbaudach.reprise"` and
`REPRISE_MOBILE_LICENSE = "GPL-3.0-or-later"`. The licence value flows into
`AboutSettingsPage.kt:45`, and `SettingsContentTest.kt:86` asserts against the
constant rather than a literal, so no test needs changing — confirm that rather
than assuming it. Leave `namespace = "de.reprise.spike"`, the Java package paths,
`versionCode`/`versionName` handling, minify/shrink and the proguard rules
untouched: the JNA/UniFFI keeps are already correct and are precisely what a
minified release build depends on.

**5. Release signing in Gradle,** as specified above, plus `.gitignore` entries
for `keystore.properties`, `*.jks` and `*.keystore`. Add
`android/signing/upload-key-sha256.txt` as a tracked placeholder whose content
explains what it is and that the release job compares against it.

**6. Add `.github/workflows/release.yml`** — the full design above: triggers,
concurrency, `gate` (version read, green-check wait with the three named states,
tag-absence check, release-metadata check), `flatpak`, `apk`, `publish`
(draft → verify → publish), `if: failure()` draft cleanup, `dry_run` input, and
build-only pull-request mode. In pull-request mode the gate job does not run;
both build jobs run and upload artifacts; nothing publishes. If the keystore
secret is absent in pull-request mode the APK job builds unsigned and says so in
its summary, while release mode still fails hard — this must be **one explicit
condition**, not two divergent code paths.

**7. `README.md`: a "Downloads" section** near the top linking to the latest
release for both the `.flatpak` and the `.apk`, with the install line for each.

**8. `docs/releasing-android.md`:** the `keytool -genkeypair` invocation with every
argument spelled out, how to read the certificate SHA-256 for
`upload-key-sha256.txt`, the four `gh secret set` lines, how to run a local
signed build via `keystore.properties`, and — plainly — what losing the keystore
costs.

**9. Extend `RELEASING.md`** with the release channel: what triggers it, what the
gate waits for, what the assets are, where the release text comes from and that
it must exist before a promotion, and the named follow-up (a self-hosted Flatpak
repository for a real update path) with the reason it was deferred. Link to
`docs/releasing-android.md`.

**The file lists in these tasks are a starting point, not a fence.** If a task
cannot be honoured without touching a file it does not name, stop and say which
file and why — do not guess, and do not silently widen the scope.

## Verification — what counts as evidence

1. **Pull-request mode runs green with both artifacts downloadable.** This is the
   mandatory evidence and it is not optional: `actionlint` passing is not the
   same as the job having run. The introducing PR must show a `.flatpak` and an
   `.apk` produced by the very jobs that will later publish them.
2. **Signature and version assertions, with output.** `apksigner verify
   --print-certs` and `aapt2 dump badging`, pasted into the PR, showing the
   fingerprint and the version pair the gate job computed.
3. **The release APK installed and launched on a real target** (emulator or the
   connected device). Debug builds never exercise R8; a stripped UniFFI class
   surfaces only here. Evidence: the install succeeded and the app reached its
   first screen.
4. **The metadata check bites.** Set `meson.build` to a wrong version, show
   `scripts/check-release-metadata.sh --gate` failing, revert. Then remove the
   `CHANGELOG.md` section, show the full mode failing, restore. A gate that has
   only ever been green proves nothing about what it catches.
5. **Dry-run dispatch on `main` after landing**, with `dry_run: true`, before the
   first real publication. This is the only run that exercises the gate job
   against real check-runs and real tags, and it is where the draft/tag
   assumption above gets confirmed.
6. **Recommended, not mandatory: a local Flatpak build** (`flatpak-builder`,
   `flatpak build-bundle`, `flatpak install --user ./Reprise-<v>.flatpak`, one
   launch). It proves the recipe independently of CI, but it is a full release
   build on the development host and nobody starts it without the owner asking
   for it.

## Out of scope, named on purpose

- Self-hosted Flatpak repository on GitHub Pages (the real update path). Deferred
  by the owner; the showroom's Pages deploy is the collision risk.
- Flathub submission — policy-blocked.
- AUR publication of the existing `packaging/aur/PKGBUILD`.
- Play Store, F-Droid or any store submission.
- Windows/macOS artifacts. `cross-target.yml` only `cargo check`s Windows.
- Renaming the `de.reprise.spike` Java namespace — cosmetic, and a large
  mechanical refactor that buys nothing here.
- Build caching for the Flatpak job. Measure first.
- Publishing as a pre-release. **Assumption:** the release is a normal one, with
  the Android half described as a preview in the notes, because the desktop app
  is the mature half and carries the `--latest` marker. Flip this if the Android
  app should not be offered as a finished thing at all.

## Parallelität

**No cut. One strand.** The single release workflow is the spine of this work:
tasks 4–6 all converge on `.github/workflows/release.yml`, and tasks 1–3 produce
exactly the files that workflow's gate job reads. A cut along the two apps —
which the first draft proposed, back when there were two workflow files — stopped
being disjoint the moment the owner chose one shared release. Any remaining split
would put the workflow file in one strand while the other strand creates the
files that workflow references, which is the cross-strand read that leaves
correct work uncommitted.

The alternative split (versioning groundwork vs. Android identity, with the
workflow as a third strand) fails for a different reason: the third strand cannot
start until both others have landed, and strands run concurrently or not at all.

There is therefore no merge order and no post-merge cross-check list. Everything
this plan verifies, it verifies inside its one branch — with one exception that
belongs in the landing notes rather than a cross-check: after the branch lands,
run `git ls-files | grep -Ei 'keystore|\.jks$|local\.properties'` and confirm it
returns nothing, so no key material ever became tracked.
