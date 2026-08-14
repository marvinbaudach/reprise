---
slug: dev-gate-back-to-green
worktree: /home/marvin/Projects/reprise-dev-gate-back-to-green
branch: feature/dev-gate-back-to-green
phase: shipped
codex_session:
created: 2026-08-13
---
# Two pre-existing failures keep the dev gate red

> Measured against `origin/dev` at `e1fac8ce80` / `7d41e0cb38` on 2026-08-13.
> Neither failure was introduced by the display-gate work (#463); both were
> found by it and verified on a clean `origin/dev` worktree.

## Why

`check-merge-readiness.sh` cannot pass on `dev` right now. Two independent
failures, neither of them caused by the branch that surfaced them.

### 1. `src_11_channel_header_stays_on_the_fallback_when_images_are_not_allowed`

Red in 3 of 3 runs on a clean `origin/dev` worktree with no commits from #463.
Measured panic:

```
youtube_channel_detail_tests.rs:417:10: source image stack
```

It is an `.expect`, not an assertion — the test cannot find the widget it wants,
so it never gets as far as checking behaviour.

The test walks the header positionally:

```rust
header.first_child().and_then(|back| back.next_sibling()).and_downcast::<gtk4::Stack>()
```

`b5e97dd36b` ("Switching artwork on starts the cover pass instead of waiting for
an occasion that never comes", #461, 2026-08-13 22:56) rewrote `build_header`
(+46/−10). The header's second child is now an artwork **host** container that
the image is appended into, so that the image can be re-bound later — which is
the whole point of #461. The `Stack` still exists, one level deeper. The
positional walk is what broke, not the behaviour.

This test is `src_11_`-prefixed, so it was already covered by the previous
`--rule-named` gate. It landed red because landing deliberately does not wait
for CI; that is working as designed, and fixing forward is the intended
response.

**Do not** restructure `build_header` to satisfy the test. #461's structure is
deliberate. Fix the test — and fix it so the next legitimate wrapper does not
break it again: locate the source-image stack by identity rather than by its
position among siblings. Then confirm the assertion it was always about still
holds: with images not allowed, the stack shows `fallback`.

### 2. `scripts/cua-e2e/run.sh` exceeds the 800-line limit

`scripts/tests/qa-linters.sh` fails with:

```
scripts/cua-e2e/run.sh must remain below the 800-line code-file limit
```

The file is 971 lines, identically so on `origin/dev`. Nothing in #463 touched
either the file or the rule.

Split it so each part has one clear job, following whatever structure the
surrounding `scripts/cua-e2e/` and `scripts/lib/` code already uses for shared
shell code — do not invent a new convention, and do not simply move lines into a
second file to get under a number. If a genuine reason exists why this file
cannot be meaningfully split, raising the limit is an acceptable outcome, but
then the raise must be argued in the commit message and the limit must be raised
to the measured value, not to a round number.

The e2e harness must still work afterwards. It is the project's screenshot
harness, so a broken split is expensive and quiet.

## Verification

- The whole ignored display suite green, run as the gate now runs it:
  `scripts/check-display-tests.sh` with no arguments. Count tests by their own
  `test result: ok. 1 passed;` lines — that guard exists precisely because an
  exit code alone hid a gap.
- `src_11_channel_header_stays_on_the_fallback_when_images_are_not_allowed`
  green in at least 3 dedicated runs, and reaching its assertion rather than
  dying in the `expect`.
- `bash scripts/tests/qa-linters.sh` exits 0.
- `scripts/cua-e2e/run.sh` still runs: execute at least one group
  (`CUA_E2E_ONLY=<group>`) and report what it did, not just that it started.

## Out of scope

- The dev → main promotion. `dev` is 9 commits ahead; that is a separate
  decision.
- Anything else the widened display gate may surface later. This plan is about
  these two failures only.
