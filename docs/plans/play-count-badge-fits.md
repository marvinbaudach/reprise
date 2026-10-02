---
slug: play-count-badge-fits
worktree: /home/marvin/Projects/reprise-play-count-badge-fits
branch: feature/play-count-badge-fits
phase: reviewed
codex_session:
created: 2026-10-02
---
# Play counts fit their badge at every font scale (#1045)

Android-only. No cargo. Planned against `origin/dev` `5325244eb4`.

## Problem

`PlayCountBadge` in `android/app/src/main/java/io/github/marvinbaudach/reprise/LibraryTrackRows.kt`
sits in a fixed `48.dp` trailing column above the duration. After 10 dp of padding and the 12 sp
`play_arrow` symbol, about 26 dp remain for the number at fontScale 1.0. Compose breaks a count
that does not fit between digits, so the badge grows to two lines and pushes the duration down.
Robolectric (#1044): 27 wraps at 2.0, 127 at 1.3, 1234 already at 1.0.

## Decisions (grilled with the user, 2026-10-02)

1. **Compact counts ≥ 1000, locale-independent.** A fixed English format: `.` as the decimal
   point, suffixes `k`, `M`, `B`. It matches `reprise_core::format::format_thousands` (fixed `,`
   grouping, already shown on Android via `reprise-view`) and the English-only UI. A
   locale-aware separator would put `1,2k` beside `1,686` on the user's `en-DE` phone. ICU
   `CompactDecimalFormat` was rejected: `de_DE` does not abbreviate thousands at all.
2. **Round half up**, and drop a trailing `.0`.
3. **The trailing column has a minimum width of `48.dp × fontScale`**, not a fixed one. Ordinary
   rows look exactly as at a fixed width. A rare wider badge (`100k`, `999M`) widens only its own
   row by a few dp, and that row's title ellipsizes slightly earlier. Badge and duration stay
   end-aligned across rows.
4. **TalkBack announces exactly one thing**: the exact count ("1234 plays"). The visible number
   is silent. This also removes today's duplicate "27 plays, 27".
5. **No UX rule.** This is a layout-defect fix like #1044. The first `[android]`-level rule would
   be a rulebook decision of its own.
6. **Device check after landing**, at font_scale 1.3 and 2.0 (see Verification).

## Design

### Formatter

`internal fun formatPlayCount(count: Long): String`. Pure: no Compose, no `Locale`.

- `count` is coerced into `0..999_499_999_999`. Below 0 becomes 0. The upper bound is the
  largest value that still renders as `999B`, so `Long.MAX_VALUE` renders `999B`.
- `0..999`: `count.toString()`.
- Otherwise take the largest unit `u` in `1_000 (k)`, `1_000_000 (M)`, `1_000_000_000 (B)` with
  `count ≥ u`:
  - `count < 10·u`: round half up to tenths of `u`, in integer arithmetic:
    `tenths = (count + u/20) / (u/10)`. If `tenths ≥ 100`, the result is `"10" + suffix`.
    Otherwise it is `tenths/10`, followed by `"." + tenths%10` unless that digit is 0, followed
    by the suffix.
  - `count ≥ 10·u`: `whole = (count + u/2) / u`. If `whole ≥ 1000`, the result is `"1"` plus the
    next suffix. Otherwise it is `whole` plus the suffix.
- No floating point anywhere. Every output has at most 4 characters.

| count | output |
|---|---|
| 7 / 999 | `7` / `999` |
| 1 000 / 1 049 / 1 050 | `1k` / `1k` / `1.1k` |
| 1 234 / 1 949 / 1 950 | `1.2k` / `1.9k` / `2k` |
| 9 949 / 9 950 | `9.9k` / `10k` |
| 12 345 / 12 500 | `12k` / `13k` |
| 99 500 / 999 499 / 999 500 | `100k` / `999k` / `1M` |
| 1 250 000 / 999 499 999 / 999 500 000 | `1.3M` / `999M` / `1B` |
| `Long.MAX_VALUE` / `-5` | `999B` / `0` |

### Badge, column, semantics

- `PlayCountBadge` moves out of `LibraryTrackRows.kt` (737 lines on origin/dev) into the new
  file `PlayCountBadge.kt`, together with `formatPlayCount`.
- The badge `Text` shows `formatPlayCount(normalizedPlayCount)`.
- **Forbidden on the badge text: `maxLines`, `softWrap = false`, `TextOverflow`, autosize, and
  any font-size reduction.** Each of them either hides digits (wrong numbers) or undercuts the
  user's font scale. The text must lay out in full on one line because the column is wide
  enough, never because it is clipped.
- The trailing `Column` in the track row changes from `Modifier.width(48.dp)` to
  `Modifier.widthIn(min = TrailingColumnMinWidth * LocalDensity.current.fontScale)`, with
  `TrailingColumnMinWidth = 48.dp` as a named constant. The title column already has
  `weight(1f)`, so the trailing column is measured first and takes what it needs.
- Semantics: the badge `Surface` carries exactly one node. For a count > 0 that is
  `clearAndSetSemantics { contentDescription = description }`, with `description` from the
  existing `R.plurals.play_count_description` and the exact count. For 0 it stays
  `alpha(0f).clearAndSetSemantics {}`, as before. The inner `MaterialSymbol` gets a blank
  description, so it adds nothing. The row merges descendants, so the row announces
  "…, 1234 plays, 4:02".

## Tasks (test-first; see each new test fail before implementing)

1. **`PlayCountFormatTest.kt`** (plain JVM unit test): every row of the table above, plus a check
   that `formatPlayCount(n).length ≤ 4` for a sweep of values up to `Long.MAX_VALUE` (powers of
   ten and their neighbours ±1, and the rounding edges `x949`/`x950`, `x499`/`x500`).
2. **Layout regression in `PlayCountBadgeTest.kt`** (Robolectric; reuse `showTrackRows`,
   `durationTopWithinRow`): for each `fontScale ∈ {0.85, 1.0, 1.3, 2.0}` and each
   `playCount ∈ {27, 127, 999, 1_234, 1_950, 99_500, 999_499, 999_499_999}`, the duration's top
   and its right edge equal those of the same row with `playCount = 7` at the same scale (±0.5).
   Add a right-edge helper beside `durationTopWithinRow` if none exists. Run this against the
   unchanged code first: it must fail, at least for 127 at 1.3 and 1234 at 1.0. Update the
   comment in `badgeSlotMatchesSingleDigitAtDoubleFontScaleWithoutMovingTheDuration` that calls
   multi-digit wrapping a known, separately tracked issue.
3. **Semantics test:** a row with 1234 plays has a node with content description "1234 plays".
   No node, in either the merged or the unmerged tree, has the text `1.2k` or `1234`. A row with
   27 plays no longer exposes a separate text node "27".
4. **Implement**: create `PlayCountBadge.kt` (badge + formatter), delete the old badge from
   `LibraryTrackRows.kt`, apply the column min width and the semantics. Tasks 1–3 go green.
5. **Gates**, in this worktree only, with the env prefix below:
   - `scripts/check-android-suite.sh` gives the verdict. A filtered gradle run hits
     `UnsatisfiedLinkError`, so it is not evidence.
   - `npm --prefix android run lint`
   - `scripts/check-{android-theme,accessibility-semantics,input-parity,ux-traceability,shared-literals,duration-format-parity,ai-hygiene,architecture}.sh`

   ```
   export ANDROID_HOME=/home/marvin/.local/share/android-sdk ANDROID_SDK_ROOT=$ANDROID_HOME \
     ANDROID_USER_HOME="$PWD/.cache/android-user-home" XDG_DATA_HOME="$PWD/.cache/xdg-data" \
     GRADLE_USER_HOME="$PWD/.gradle-user-home" JAVA_HOME=/usr/lib/jvm/java-21-openjdk TMPDIR=/tmp
   printf 'sdk.dir=%s\n' "$ANDROID_HOME" > android/local.properties
   ```

Android-only, no cargo. The implementer is the sole writer in this worktree.

## Verification after landing (session, not Codex)

1. Build the APK with `~/.cache/reprise-apk/build-apk.sh` (built from dev). Check
   `apksigner verify --print-certs` against the installed APK (cert `e594235b…`).
2. `device-lock acquire --wait 1800 play-count-badge "font-scale check" && …`:
   - `adb install -r`
   - Read `settings get system font_scale` and remember the value.
   - Set the font scale to 1.3, then 2.0. At each, take a screenshot of a library list with
     multi-digit play counts: one line per badge, durations aligned.
   - Restore the original font_scale, release the lock.

## Parallelität

No cut. One composable, one formatter and their tests. Tasks 2–4 all touch `PlayCountBadge.kt`
and `LibraryTrackRows.kt`, so there is no disjoint file group. Single strand.
