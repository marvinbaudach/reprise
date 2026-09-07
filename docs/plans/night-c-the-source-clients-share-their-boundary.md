---
slug: night-c-the-source-clients-share-their-boundary
worktree: /home/marvin/Projects/reprise/.worktrees/night-c-sources-http
branch: refactor/the-source-clients-share-their-boundary
phase: refactored
created: 2026-09-07
base: origin/dev
owns: crates/reprise-core/src/{podcasts/http.rs,radio/http.rs,concerts/http.rs,sources_http.rs,lib.rs}
---
# Night package C — the three source clients share what is actually shared

## Autonomy

**Run this end to end without asking.** Plan → code → check → refactor → land,
including the merge. Autonomous landing is authorised for this package. Do not
ask for `/check`, `/refactor` or `/ship` between phases. Stop only for a listed
**stop condition**; then leave the worktree, write a `## Findings` section into
this file, set `phase: blocked`, and stop.

Own worktree `.worktrees/night-c-sources-http`, branch
`refactor/the-source-clients-share-their-boundary`, off `origin/dev`. Four
sibling packages run in parallel tonight. **Touch only the files in `owns:`.**

## Why, and what this package is NOT

A note from 2026-08-20 says `podcasts/http.rs`, `radio/http.rs` and
`concerts/http.rs` "clone the same HTTP boundary idiom" and asks that the
consolidation not be allowed to evaporate.

**That note overstates the prize, and this plan is deliberately smaller than it
sounds.** Measured on 2026-09-07: of 1,297 combined lines, roughly 90 collapse.
Do not attempt the large consolidation. Specifically, these look duplicated and
are **not**:

- **The rate limiter.** `concerts` polls in 50 ms slices so a caller can cancel
  mid-wait; `podcasts` and `radio` cannot be cancelled. That is a behavioural
  difference, not a copy.
- **The status-code mapping.** Only `podcasts` distinguishes source-gone
  (404/410) and `304 Not Modified`; `concerts` checks 429 and a success range
  and nothing else.
- **The error classification target.** `PodcastError`, `RadioError` and
  `ProviderError` are unrelated types — `ProviderError::Transport` is a unit
  variant while the other two carry a `String`. A shared classifier would have
  to become generic over the error type: more machinery than saving.
- **Fixture route matching.** Different hosts, paths and query keys per source.

Extracting those would trade three readable files for one clever one. If while
working you conclude otherwise, that is a stop condition — say what you found,
do not widen the scope.

## What genuinely is identical

These four, and only these:

| Piece | Copies | Where |
| --- | --- | --- |
| `user_agent()` — `format!("Reprise/{} ( {} )", CARGO_PKG_VERSION, CONTACT_URL)` | 3, byte-identical | `podcasts/http.rs`, `radio/http.rs`, `concerts/http.rs` |
| `lock_unpoisoned()` | 3, byte-identical | same three |
| The `ureq` agent-builder chain (`config_builder().timeout_global(..).user_agent(..).http_status_as_error(false).build().new_agent()`) | 4 call sites | `podcasts/http.rs` ×2, `radio/http.rs` ×2, `concerts/http.rs` ×1 |
| The fixture scaffolding: a `TEST_FIXTURE_DIR` thread-local, a `FIXTURE_DIR_ENV` name, `fixture_directory()` and `with_fixture_dir()` | 3 | same three; `radio`'s `with_fixture_dir` additionally calls `super::servers::reset_cache_for_tests()` |

Expected saving: roughly 85 to 95 lines. Blast radius: the three files plus a
new module. That is the whole job.

## Tasks

### C.1 — the new module

Create `crates/reprise-core/src/sources_http.rs` and declare it in `lib.rs`.
Give it a module doc that says what belongs in it **and what deliberately does
not**, naming the four per-source differences above. A future reader who does
not find the rate limiter there must learn why in one paragraph, or they will
"finish the job" and break cancellation.

It holds:

- `pub(crate) fn user_agent() -> String`
- `pub(crate) fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T>` —
  keep the existing behaviour exactly, including how it treats a poisoned lock.
- `pub(crate) fn build_agent(timeout: Duration) -> ureq::Agent` — the builder
  chain, with the timeout as the one parameter that actually varies.
- The fixture scaffolding, parameterised by the env-var name, with a hook so
  `radio` can still run its cache reset on enter and exit. A closure parameter
  or a small `FixtureScope` struct both work; pick one and say why in the doc.

### C.2 — point the three files at it

Replace each local definition with a call. Keep the per-source timeout constants
(`DOWNLOAD_TIMEOUT` 15 s in podcasts, `CLICK_TIMEOUT` 5 s in radio) where they
are — they are call-site knobs, not shared state.

### C.3 — the last `fnv1a_64` is already done

Do not look for it. Five definitions existed in August; four were consolidated
onto `artist_news_refresh::fnv1a_64` and the last one landed on 2026-09-07. If
you find a sixth, that is a finding worth reporting.

## Acceptance

- `user_agent`, `lock_unpoisoned` and the builder chain each have exactly one
  definition in `crates/reprise-core/src`.
- The three `http.rs` files together shrink by 80 to 100 lines. If the number
  comes out far above that, you extracted something per-source; check it against
  the "not identical" list.
- Every existing test in `reprise-core` still passes, unchanged. **You may not
  edit a test to make it pass** — these files carry fixture-driven tests, and a
  fixture that stops matching means the extraction changed behaviour.
- The fixture files under `crates/reprise-core/tests/fixtures/` are untouched.

## Gates

```
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p reprise-core
scripts/check-architecture.sh
```

`check-architecture.sh` counts `ureq` agents in `reprise-core` against
`http_boundary_budget=16`, and that budget **fails in both directions**. If
routing the four builder call sites through one helper lowers the count, lower
the budget in the same commit and say why in the commit message — the script
tells you the number to write. Do not raise it.

## Stop conditions

- The extraction pulls in the rate limiter, the status mapping or the route
  matching. Stop; the plan says those are not shared.
- A fixture test fails and the only way to green is editing the fixture or the
  test. Stop and report — that is behaviour change wearing a refactor's clothes.
- `reprise-core` is already red on unmodified `origin/dev`. Check the control
  arm first; a red base is not yours to fix here.

## Landing

Squash-merge into `dev`. The PR title is taken verbatim, so write prose, not a
refactoring label. Something like *"The three source clients build one HTTP
agent"*. Say in the body that the larger consolidation was measured and
deliberately not done, and why — otherwise someone reopens it in a month.
