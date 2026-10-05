---
slug: refactor-wave-2026-10-w3b
worktree: /home/marvin/Projects/reprise-refactor-wave-2026-10-w3b
branch: feature/refactor-wave-2026-10-w3b
phase: coded
codex_session:
created: 2026-10-05
---
# Refactor wave 2026-10, wave 3 — strand B: one HTTP boundary (`reprise_core::net`)

Mother plan: `docs/plans/refactor-wave-2026-10-w3.md`, and the "Standing rules for every strand" in
`docs/plans/refactor-wave-2026-10.md` bind this strand. Consolidation package 2.1
(`docs/plans/consolidation-plan.md` §4) and finding D3 (`docs/plans/architecture-consolidation.md`
§4.4) are the origin; this plan is the authority where they differ, because it was measured against
the current tree.

This plan is your only channel. Everything you need is written here. When the code disagrees with a
table below, the code wins: keep what the code does and say so in your final message.

## Purpose

Every HTTP agent in `reprise-core` that fetches source metadata is built in one place, every request
spacer is one implementation, and the circuit breaker lives beside them. Nothing on the wire changes:
every provider keeps exactly its timeouts, its user agent, its redirect policy, its request spacing,
its fixture seam and its error type. The gate budget drops from 12 to 5.

**Behaviour-preserving means:** same request timings, same headers, same ureq configuration per
provider, same error values, same test names and asserted numbers. If a step can only be done by
changing one of those, stop that step and report it.

## Evidence (origin/dev @ 0322ae01df, 2026-10-05)

Paths below are relative to `crates/reprise-core/src/`.

**Twelve `ureq::Agent::config_builder` matches**, one per file (the gate counts text matches, not
agents):

| # | File:line | Group |
| --- | --- | --- |
| 1 | `sources_http.rs:45` (`build_agent(timeout)`, called from `podcasts/http.rs:85,125`, `radio/http.rs:36,57`, `concerts/http.rs:36`) | shared builder — becomes `net::client` |
| 2 | `musicbrainz.rs:56` | in scope |
| 3 | `cover_download.rs:485` | in scope |
| 4 | `lyrics/lrclib.rs:454` | in scope |
| 5 | `lyrics/netease.rs:291` | in scope |
| 6 | `artist_portrait/deezer.rs:153` | in scope |
| 7 | `podcasts/source_artwork.rs:55` | in scope |
| 8 | `library/library_doctor/remote/network.rs:86` | in scope |
| 9–12 | `scrobbling.rs:463`, `scrobbling/lastfm.rs:88`, `library/lastfm_stats.rs:84`, `library/listenbrainz.rs:63` | excluded (own user agent without contact URL, own auth rhythm, no limiter) |

`podcasts/stream_proxy.rs:285` builds `ureq::Agent::new_with_defaults()` (not counted by the gate)
with a per-request `.config().timeout_global(Some(ORIGIN_TIMEOUT)).http_status_as_error(false)`
override at `:616-625`. It sends ureq's default user agent (`ureq/3.4.2`). It is **excluded** — see
Decisions.

**Two user-agent functions, one string.** `sources_http::user_agent()` and
`musicbrainz::user_agent()` both produce `Reprise/<CARGO_PKG_VERSION> ( https://github.com/marvinbaudach )`;
`CONTACT_URL` is `musicbrainz.rs:27`. The excluded four use `concat!("Reprise/", env!("CARGO_PKG_VERSION"))`
without the contact URL.

**Eight request spacers**, one static per module, two algorithms:

| Module | Static (file:line) | Interval | Algorithm today | Cancellable | Runs in fixture mode? |
| --- | --- | --- | --- | --- | --- |
| `musicbrainz.rs` | `LAST_REQUEST` :29, `MIN_REQUEST_INTERVAL` :21 | 1 s | lock held through the wait, 50 ms slices, record `now` after the wait, **no record on cancel** (`wait_for_request_slot` :202-219) | yes (`&mut dyn FnMut() -> bool`); real cancel closure only from `network.rs:111` | yes (limiter runs before the fixture branch :52-55) |
| `podcasts/http.rs` | `LAST_REQUEST` :28, `MIN_REQUEST_INTERVAL` :24 | 1 s | lock held across one `thread::sleep`, record after (`respect_rate_limit` :317-326) | no | yes (before the fixture branch) |
| `radio/http.rs` | `LAST_REQUEST` :24, `MIN_REQUEST_INTERVAL` :20 | 1 s | lock held across one sleep (`wait_for_request_slot` :221-227) | no | **no** (fixture branch first, :31-34, :52-55) |
| `concerts/http.rs` | `LAST_REQUEST` :28, `MIN_REQUEST_INTERVAL` :22 | 1 s | as musicbrainz (`wait_for_request_slot` :196-215, `pub(crate)`) | yes; the only caller passes `&mut \|\| false` | yes (before the fixture branch) |
| `lyrics/lrclib.rs` | `LAST_REQUEST: LazyLock<Mutex<..>>` :26, `REQUEST_INTERVAL` :19 | 250 ms | lock held across one sleep (`wait_for_request_slot` :536-546) | no | **no** (fixture check :450-452 first) |
| `lyrics/netease.rs` | `LAST_REQUEST: LazyLock<Mutex<..>>` :20, `REQUEST_INTERVAL` :17 | 250 ms | as lrclib (:319-329) | no | **no** (fixture check in `ProductionFetcher` :272, :282 first) |
| `artist_portrait/deezer.rs` | `LAST_REQUEST` :22, `MIN_REQUEST_INTERVAL` :12 | 300 ms | **reserve under the lock, sleep outside it** (`respect_rate_limit` :177-191) | no | n/a (no fixture hook) |
| `library/library_doctor/remote/network.rs` | `LAST_ACOUSTID` :29, `ACOUSTID_INTERVAL` :21 | 334 ms | reserve under the lock (`rate_limit` :359-372 with `request_delay` :374-382), then `cancellable_sleep` (50 ms `WAIT_SLICE`, polls `control()`); **the reservation stays on cancel** | yes (`ScanControl`) | n/a |

`cover_download.rs:483` and the MusicBrainz path of `network.rs:111` share `musicbrainz::LAST_REQUEST`.
So CAA image fetches and MusicBrainz lookups already share one 1 s budget, and `network.rs:110` passes
a throwaway `&Mutex::new(None)` with `Duration::ZERO` to `rate_limit` on that path.

**Tests that pin spacing** (names and numbers must survive):
`musicbrainz.rs:256 request_delay_enforces_one_second_interval`, `:270 fetch_respects_rate_limit`
(asserts a 750 ms sleep after 250 ms elapsed, via the `cfg(test)` `respect_rate_limit_with`),
`:307 poisoned_limiter_mutex_is_recovered`;
`network_tests.rs:144 source_rate_limits_match_service_contracts` (750 ms and 234 ms via
`request_delay(previous, now, interval)`), `:165 rate_limiter_reserves_three_concurrent_slots_monotonically`
(0, 334, 668 ms), `:263 cancellation_interrupts_wait_and_backoff` (tests `cancellable_sleep`, which
stays in `network.rs`).

**Circuit breaker.** `lyrics/breaker.rs` (101 lines), everything `pub(super)`: `Breaker`,
`BreakerOutcome { Success, NotFound, Failure }`, `HOST_BREAKER: LazyLock<Breaker>` keyed by
`&'static str` host, `FAILURE_LIMIT = 3`, `OPEN_SECONDS = 300`, no half-open state. Users:
`lyrics/lrclib.rs`, `lyrics/netease.rs`, `lyrics/mod.rs:135 all_network_breakers_open()`. Tests:
`lyrics/breaker_tests.rs` (5 tests), plus `lrclib_tests.rs`, `netease_tests.rs`, `batch_tests.rs`
construct `Breaker::new(3, 300)`.

**Fixture seam.** `sources_http.rs:54-93` holds `fixture_directory(env_name)` and the `cfg(test)`
`with_fixture_dir(...)` with a thread-local override; `podcasts/http.rs:223,233`, `radio/http.rs:99,112`
and `concerts/http.rs:60,70` wrap them. MusicBrainz and the lyrics providers read their variables
directly. The lrclib/netease seams are compiled unconditionally (not behind `test-fixtures`).

**Fixture variables are depended on outside this strand's files**, so they are not renamed:

| Variable | Outside users |
| --- | --- |
| `REPRISE_MUSICBRAINZ_FIXTURE_DIR` / `_LOG` | `scripts/check-lyrics-smoke.sh:30`, `scripts/ptr-e2e/run.sh:316-317` |
| `REPRISE_PODCASTS_FIXTURE_DIR` | `scripts/cua-e2e/podcast_backlog.sh`, `scripts/cua-e2e/source_content.sh`, `crates/reprise-mcp/tests/source_discovery.rs:104`, `crates/reprise-mcp/tests/source_management.rs:95,401` |
| `REPRISE_RADIO_FIXTURE_DIR` | the same two cua-e2e scripts, `crates/reprise-mcp/tests/source_discovery.rs:228`, `source_management.rs:198,233,266,499,536` |
| `REPRISE_LRCLIB_FIXTURE_DIR` / `_LOG` (legacy names) | `scripts/check-lyrics-smoke.sh:28-29,40` (asserts 3 request-log lines), `RELEASING.md:214` |
| `REPRISE_CONCERTS_FIXTURE_DIR`, `REPRISE_LYRICS_FIXTURE_DIR`, `REPRISE_CONCERTS_FIXTURE_LOG`, `REPRISE_LYRICS_FIXTURE_LOG` | none in code or scripts |

**Redirect and address checks** exist only in `podcasts/source_artwork.rs` (`validate_remote_url`,
`is_public_ip`, `PublicOnlyResolver`, `.max_redirects(0)`, `.proxy(None)`, built through
`ureq::Agent::with_parts(config, DefaultConnector::default(), PublicOnlyResolver { .. })`). Every other
provider follows ureq's default of 10 redirects with no target check.

**Error types.** Each provider has its own enum (`FetchError`, `PodcastError`, `RadioError`,
`ProviderError`, `LyricsError`, `RemoteProviderError`, `StreamProxyError`). `TransportError` already
exists in `scrobbling.rs:239` and is matched by name in five reprise-gnome files. `RadioError::Parse`
is constructed in `reprise-gnome` and `reprise-mcp`; `PodcastError` variants are matched in gnome tests;
`FetchError::Transport` is constructed in an android-ffi test.

No `include_str!` anywhere in the workspace scans a file this strand touches. No script greps these
files by path except the budget block in `scripts/check-architecture.sh:240-270`.

## Decisions (fixed — do not re-open)

1. **Keyed by provider budget, not by host.** Today's grouping is not a host grouping: CAA
   (`coverartarchive.org`) shares MusicBrainz's 1 s slot, radio-browser is reached through many mirror
   hosts (`radio/servers.rs`) under one slot, and fixture-mode requests have no host at all. A host key
   would split or merge budgets and change timings. The key is an enum with eight variants, one per
   static that exists today, each carrying today's interval. Re-keying by host is a policy change for
   a later wave.
2. **One algorithm: reserve under the lock, sleep outside it.** This is the shape the newest two
   limiters (deezer, AcoustID) already have and the one the tests pin (0/334/668 ms monotonic slots).
   For single callers it produces exactly the lock-held timings (next request ≥ last + interval). The
   lock is held for microseconds, never across a sleep — which also keeps
   `clippy::significant_drop_in_scrutinee` quiet. Cancellation semantics are preserved per caller:
   `wait_for_slot` rolls a cancelled reservation back (today's MusicBrainz/concerts behaviour, "no
   record on cancel"); `reserve_slot` hands the delay back for callers with their own cancellable sleep
   (today's AcoustID behaviour, "reservation stays").
3. **Fixture variables keep their names and their per-provider directories.** Scripts, the ptr-e2e
   harness and the mcp integration tests set them (table above). Only the mechanism moves,
   verbatim. Unification is a later wave, and it needs those scripts in the same change.
4. **No `SourceTransportError`.** The provider error enums are matched and constructed outside
   `reprise-core`, and `TransportError` already exists for scrobbling with its own UI matching. Folding
   them would ripple into GTK and MCP code this strand does not own. Error types stay untouched.
5. **The breaker lifts, it does not spread.** `lyrics/breaker.rs` moves to `net/breaker.rs`
   unchanged (visibility widened to `pub(crate)`). Connecting other providers to it would make them
   skip requests they make today — a behaviour change for a later wave.
6. **`stream_proxy.rs` stays outside the boundary.** It is a byte relay, not a metadata source; it
   deliberately carries ureq's default identity on the wire, and the only way to keep that through a
   shared policy is a "no user agent" knob with one user. The gate allowlist names it explicitly.
7. **The four scrobbling-family agents stay outside**, as the consolidation plan says.
8. **Response-size helpers (`http_body.rs`) stay where they are.** Not part of this cut.

## Owns

This list is a starting point, not a fence. A file that has to change to keep a call site compiling
may be added. Stop only if the contract itself (the tables in this plan) turns out wrong.

- New: `crates/reprise-core/src/net/{mod,client,client_tests,rate,rate_tests,breaker,breaker_tests,fixtures}.rs`
- Deleted (moved): `crates/reprise-core/src/sources_http.rs`, `crates/reprise-core/src/lyrics/breaker.rs`,
  `crates/reprise-core/src/lyrics/breaker_tests.rs`
- `crates/reprise-core/src/lib.rs` — the `mod sources_http;` line only (becomes `pub(crate) mod net;`)
- `crates/reprise-core/src/musicbrainz.rs`, `cover_download.rs`
- `crates/reprise-core/src/lyrics/{mod,lrclib,netease}.rs` and the import lines of
  `lyrics/{lrclib_tests,netease_tests,batch_tests,mod_tests}.rs`
- `crates/reprise-core/src/artist_portrait/deezer.rs`
- `crates/reprise-core/src/podcasts/{http,source_artwork}.rs`
- `crates/reprise-core/src/radio/{http,servers}.rs`
- `crates/reprise-core/src/concerts/http.rs`
- `crates/reprise-core/src/library/library_doctor/remote/{network,network_tests}.rs`
- `scripts/check-architecture.sh` — the `== Engine HTTP boundaries ==` block only (lines 240-270).
  Strand A edits a different block of the same script (a new headless-rusqlite block after line 405).
  Do not touch anything else in the script.

Not owned, do not touch: `scrobbling.rs`, `scrobbling/lastfm.rs`, `library/lastfm_stats.rs`,
`library/listenbrainz.rs`, `podcasts/stream_proxy.rs`, `http_body.rs`, `source_error.rs`, any
`reprise-gnome`/`reprise-mcp`/`reprise-cli` file, any `Cargo.toml`.

## Behaviour table 1 — agent policy per provider (must stay identical)

`timeout` is `timeout_global`. "status errors" is `http_status_as_error`; ureq's default is `true`.
"redirects" is `max_redirects`; ureq's default is 10. `proxy` default is `Proxy::try_from_env()`.
UA "Reprise+contact" is `Reprise/<ver> ( https://github.com/marvinbaudach )`.

| Provider | File | timeout | UA | status errors | redirects | https_only | proxy | Agent lifetime today |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| podcasts feeds/search (`get_with_budget`) | `podcasts/http.rs:85` | `SOURCE_REQUEST_TIMEOUT` = 10 s (`source_error.rs:14`) | Reprise+contact | false | 10 | false | env | per request |
| podcasts downloads (`download_with_progress`) | `podcasts/http.rs:125` | `DOWNLOAD_TIMEOUT` = 15 s (`:23`) | Reprise+contact | false | 10 | false | env | per request |
| radio (`get_with_timeout`) | `radio/http.rs:36` | parameter: `HTTP_TIMEOUT` = 10 s (`:18`), or `CLICK_TIMEOUT` = 5 s (`:19`) from `radio/click.rs:48` | Reprise+contact | false | 10 | false | env | per request |
| radio ICY headers (`icy_headers`) | `radio/http.rs:57` | `HTTP_TIMEOUT` = 10 s | Reprise+contact | false | 10 | false | env | per request |
| concerts (`get`) | `concerts/http.rs:36` | `SOURCE_REQUEST_TIMEOUT` = 10 s | Reprise+contact | false | 10 | false | env | per request |
| musicbrainz (`get`) | `musicbrainz.rs:56` | `HTTP_TIMEOUT` = 15 s (`:20`) | Reprise+contact | **true** | 10 | false | env | per request |
| cover_download CAA (`http_get_bytes`) | `cover_download.rs:485` | `HTTP_TIMEOUT` = 15 s (`:17`) | Reprise+contact | **true** | 10 | false | env | per request |
| lrclib (`fetch`) | `lyrics/lrclib.rs:454` | `HTTP_TIMEOUT` = 8 s (`:18`) | Reprise+contact | false | 10 | false | env | per request |
| netease (`fetch_url`) | `lyrics/netease.rs:291` | parameter: `HTTP_TIMEOUT` = 8 s (`:16`) for `search`, then `HTTP_TIMEOUT.checked_sub(started.elapsed())` for `lyric` (`:97-100`) | Reprise+contact | **true** | 10 | false | env | per request |
| deezer | `artist_portrait/deezer.rs:153` | `HTTP_TIMEOUT` = 15 s (`:11`) | Reprise+contact | **true** | 10 | **true** | env | once, `static AGENT: OnceLock<Agent>` (`:23`) |
| source artwork (`fetch_with_resolver`) | `podcasts/source_artwork.rs:55` | `HTTP_TIMEOUT` = 15 s (`:15`) | Reprise+contact | false | **0** | false | **None** | per call, via `Agent::with_parts(config, DefaultConnector::default(), PublicOnlyResolver { .. })` |
| library doctor (`NetworkProvider::new`) | `library_doctor/remote/network.rs:86` | `HTTP_TIMEOUT` = 15 s (`:18`) | Reprise+contact | false | 10 | false | env | once per provider, field `agent` (`:72`), cloned per request (`:108,:132`) |

Per-request headers stay where they are (`If-None-Match`/`If-Modified-Since` in `podcasts/http.rs:88,91`,
`Icy-MetaData` in `radio/http.rs:59`, `send_form` in `network.rs:293`). Nothing in this strand adds,
removes or reorders a header.

## Behaviour table 2 — request spacing per key

| `RateLimitKey` | Interval | Callers after the strand | Entry point | Order relative to the fixture branch (unchanged) |
| --- | --- | --- | --- | --- |
| `MusicBrainz` | 1 000 ms | `musicbrainz::get`, `cover_download::http_get_bytes` (CAA), `network.rs` MusicBrainz path | `wait_for_slot(key, cancelled)` | before the fixture branch |
| `AcoustId` | 334 ms | `network.rs::acoustid_request` | `reserve_slot(key)` + existing `cancellable_sleep` | n/a |
| `Podcasts` | 1 000 ms | `podcasts/http.rs::{get_with_budget,download_with_progress}` | `wait_for_slot(key, &mut \|\| false)` | before the fixture branch |
| `Radio` | 1 000 ms | `radio/http.rs::{get_with_timeout,icy_headers}` | `wait_for_slot(key, &mut \|\| false)` | **after** the fixture branch (fixtures skip it) |
| `Concerts` | 1 000 ms | `concerts/http.rs::get` | `wait_for_slot(key, cancelled)` | before the fixture branch |
| `Lrclib` | 250 ms | `lyrics/lrclib.rs::fetch` | `wait_for_slot(key, &mut \|\| false)` | after the fixture check |
| `Netease` | 250 ms | `lyrics/netease.rs::fetch_url` | `wait_for_slot(key, &mut \|\| false)` | after the fixture check (in `ProductionFetcher`) |
| `Deezer` | 300 ms | `deezer::{search,download_image}` | `wait_for_slot(key, &mut \|\| false)` | n/a |

## The new module — exact shapes

`crates/reprise-core/src/net/mod.rs`:

```rust
//! The engine's one HTTP boundary: agent construction (`client`), request spacing (`rate`),
//! the host circuit breaker (`breaker`) and the fixture seam (`fixtures`).
pub(crate) mod breaker;
pub(crate) mod client;
pub(crate) mod fixtures;
pub(crate) mod rate;

pub(crate) use client::{user_agent, CONTACT_URL};

/// Locks a mutex and recovers a poisoned one: a panic elsewhere must not take the limiter with it.
pub(crate) fn lock_unpoisoned<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T>;
// moved verbatim from sources_http.rs:33-37; the private copies in musicbrainz.rs:235,
// network.rs:734 and radio/servers.rs:113 are deleted and their callers import this one.
```

`crates/reprise-core/src/net/client.rs` — the ONLY in-scope `ureq::Agent::config_builder`:

```rust
pub(crate) const CONTACT_URL: &str = "https://github.com/marvinbaudach";   // moved from musicbrainz.rs:27

#[must_use]
pub(crate) fn user_agent() -> String;        // "Reprise/{CARGO_PKG_VERSION} ( {CONTACT_URL} )", byte-identical to both old fns

/// Everything a provider decides about its agent. Fields map 1:1 to ureq config options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AgentPolicy {
    pub timeout: Duration,            // -> .timeout_global(Some(timeout))
    pub status_as_error: bool,        // -> .http_status_as_error(status_as_error)
    pub https_only: bool,             // -> .https_only(true) only when true
    pub max_redirects: Option<u32>,   // -> .max_redirects(n) only when Some; None = ureq default (10)
    pub proxy_from_env: bool,         // -> .proxy(None) only when false; true = ureq default
}

impl AgentPolicy {
    /// What `sources_http::build_agent(timeout)` built: status errors off, everything else default.
    pub(crate) const fn source(timeout: Duration) -> Self;
    /// Status errors on, everything else default (musicbrainz, CAA, netease).
    pub(crate) const fn strict(timeout: Duration) -> Self;
}

pub(crate) fn build_config(policy: AgentPolicy) -> ureq::config::Config;   // the one config_builder call; always sets user_agent(user_agent())
pub(crate) fn build_agent(policy: AgentPolicy) -> ureq::Agent;           // build_config(policy).new_agent()
pub(crate) fn build_agent_with_resolver(
    policy: AgentPolicy,
    resolver: impl ureq::unversioned::resolver::Resolver,
) -> ureq::Agent;   // ureq::Agent::with_parts(build_config(policy), DefaultConnector::default(), resolver)
```

`build_config` applies the options exactly as the table says: `timeout_global` and `user_agent`
always; `http_status_as_error(policy.status_as_error)` always (explicitly passing `true` equals the
default); `https_only(true)`, `max_redirects(n)` and `proxy(None)` only when the policy asks. The
resulting `Config` must equal what each provider builds today, option for option.

`crates/reprise-core/src/net/rate.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RateLimitKey { MusicBrainz, AcoustId, Podcasts, Radio, Concerts, Lrclib, Netease, Deezer }

impl RateLimitKey {
    pub(crate) const fn interval(self) -> Duration;   // the table-2 values, nothing else
}

const KEY_COUNT: usize = 8;
const SLICE: Duration = Duration::from_millis(50);
static SLOTS: [Mutex<Option<Instant>>; KEY_COUNT] = [const { Mutex::new(None) }; KEY_COUNT];
// one mutex per key: no cross-provider contention and poison isolation, exactly as the eight statics today

/// Moved verbatim from network.rs:374-382. A future `previous` (a reservation) adds the interval on top.
pub(crate) fn request_delay(previous: Option<Instant>, now: Instant, interval: Duration) -> Duration;

/// Pure core of the limiter: computes the delay from `*previous`, stores the reserved slot, returns the delay.
pub(crate) fn reserve(previous: &mut Option<Instant>, now: Instant, interval: Duration) -> Duration;

/// Reserves the next slot for `key` and returns how long the caller must wait before sending.
/// The reservation stays even if the caller never sends (today's AcoustID semantics).
pub(crate) fn reserve_slot(key: RateLimitKey) -> Duration;

/// Reserves, then sleeps in 50 ms slices, polling `cancelled()` before every slice and once more after
/// the loop (today's MusicBrainz loop shape). Returns `false` on cancellation; a cancelled wait restores
/// the previous slot value if no later reservation was made on this key — so, like today, a cancelled
/// request records nothing.
pub(crate) fn wait_for_slot(key: RateLimitKey, cancelled: &mut dyn FnMut() -> bool) -> bool;
```

`crates/reprise-core/src/net/fixtures.rs`: `fixture_directory(environment_variable: &str) -> Option<PathBuf>`
and the `cfg(test)` `with_fixture_dir(...)` plus `TEST_FIXTURE_DIR`, moved **verbatim** from
`sources_http.rs:15-93`, including the `#[cfg(any(test, feature = "test-fixtures"))]` gates.

`crates/reprise-core/src/net/breaker.rs`: `lyrics/breaker.rs` moved verbatim, every `pub(super)`
becoming `pub(crate)`. `net/breaker_tests.rs` is `lyrics/breaker_tests.rs` moved, with its `use` lines
adjusted.

## Tasks (in order, one commit each)

**B1 — the pinning tests first (red = they do not compile until B2).**

Create `net/rate_tests.rs` and `net/client_tests.rs` as `#[cfg(test)]` sibling modules (declare them
from `rate.rs`/`client.rs` with `#[cfg(test)] mod rate_tests;` — follow how `lyrics/mod.rs` declares
`breaker_tests`). Write:

- `every_key_keeps_its_interval`: the eight `RateLimitKey::X.interval()` values of table 2.
- `request_delay_matches_service_contracts`: moved from `network_tests.rs:144` — 750 ms for
  MusicBrainz after 250 ms elapsed, 234 ms for AcoustID after 100 ms. Use
  `RateLimitKey::MusicBrainz.interval()` / `RateLimitKey::AcoustId.interval()` instead of the
  `MUSICBRAINZ_INTERVAL`/`ACOUSTID_INTERVAL` constants.
- `reserve_spaces_three_concurrent_slots_monotonically`: moved from `network_tests.rs:165` — three
  `reserve(&mut slot, now, interval)` calls return 0, 334, 668 ms.
- `reserve_after_the_interval_elapsed_waits_nothing_and_records_now`: a previous slot `now - 2 s` with a
  1 s interval returns `Duration::ZERO` and stores `Some(now)`.
- `fetch_respects_rate_limit` equivalent: `reserve` on `Some(now - 250 ms)` with the MusicBrainz interval
  returns 750 ms and stores `now + 750 ms` (replaces `musicbrainz.rs:270`, whose `cfg(test)`
  `respect_rate_limit_with` is deleted in B4).
- `cancelled_wait_restores_the_previous_slot`: with a key whose slot is `Some(t0)`, a `wait_for_slot`
  whose `cancelled` returns `true` on the first poll returns `false` and leaves the slot at `Some(t0)`.
  Use a key no other test touches in the same process (`Concerts`), and assert via a `cfg(test)` accessor
  `slot(key) -> Option<Instant>` you add to `rate.rs`. Do not sleep in tests: the pure functions carry
  the timing; `wait_for_slot` is exercised only with an immediately-cancelling closure.
- `poisoned_slot_mutex_is_recovered`: moved from `musicbrainz.rs:307` — poison one key's mutex from a
  panicking thread, then `reserve_slot` still works.
- In `client_tests.rs`: `user_agent_identifies_version_and_maintainer` (moved from `musicbrainz.rs:249`:
  contains `CARGO_PKG_VERSION` and `CONTACT_URL`); `source_policy_turns_status_errors_off_and_leaves_the_rest_default`
  and `strict_policy_turns_status_errors_on` asserting the `AgentPolicy` literals;
  `build_config_applies_exactly_the_policy`: build configs for `AgentPolicy::source(10 s)`, for the
  deezer policy and for the source-artwork policy and read them back through ureq 3.4's `Config`
  getters (`timeout_global()`, `http_status_as_error()`, `https_only()`, `max_redirects()`,
  `user_agent()`; if a getter is missing in 3.4.2, drop that one assertion and say so).
- `provider_agent_policies_are_unchanged`: one table test asserting every provider's policy function
  (introduced in B3–B8 as `pub(crate) fn agent_policy() -> AgentPolicy`, or `agent_policy(timeout)`
  where the timeout is a parameter) equals the table-1 row. Write the expected literals now from
  table 1; the test goes green as the providers convert.

**B2 — the module.** Create `net/{mod,client,fixtures,rate,breaker}.rs` with the shapes above. Move
`lock_unpoisoned`, `user_agent`, the fixture seam and the breaker verbatim (adjust only paths and
visibility). Replace `mod sources_http;` in `lib.rs:110` with `pub(crate) mod net;` (keep the
alphabetical position of the `mod` lines as the file has it). Delete `sources_http.rs`,
`lyrics/breaker.rs`, `lyrics/breaker_tests.rs`; `lyrics/mod.rs:14` drops `mod breaker;` and
`all_network_breakers_open` (`:135`) reads `crate::net::breaker::HOST_BREAKER`. Fix the imports in
`lrclib.rs:9`, `netease.rs:7` and the four lyrics test files. The crate compiles; the B1 tests go green
except `provider_agent_policies_are_unchanged`.

**B3 — podcasts, radio, concerts `http.rs`.** Replace the five `build_agent(timeout)` calls with
`crate::net::client::build_agent(agent_policy(..))`, where each module gains
`pub(crate) fn agent_policy(timeout: Duration) -> AgentPolicy { AgentPolicy::source(timeout) }` (or
a zero-argument one where the timeout is a constant). Replace each module's `LAST_REQUEST`,
`MIN_REQUEST_INTERVAL` and wait function with `crate::net::rate::wait_for_slot(RateLimitKey::X, ..)`
at the **same position** in the call sequence (table 2, last column). `concerts::http::wait_for_request_slot(cancelled)`
is `pub(crate)` — keep it as a one-line wrapper if anything outside `concerts/http.rs` calls it,
otherwise inline. Point the three fixture wrappers at `crate::net::fixtures::{fixture_directory,
with_fixture_dir}`. `podcasts/http.rs:20` re-exports `user_agent` for `source_artwork.rs:57` and the
UA tests at `podcasts/http.rs:358`, `radio/http.rs:290`, `concerts/http.rs:302`: point the re-export
and the `cfg(test)` imports at `crate::net::user_agent` and keep the three tests as they are. The
timeout-contract tests (`podcasts/http.rs:520`, `radio/http.rs:320`, `concerts/http.rs:326`) stay
untouched; the constants they assert stay in their modules.

**B4 — musicbrainz and cover_download.** `musicbrainz::get` builds `build_agent(agent_policy())`
with `agent_policy() = AgentPolicy::strict(HTTP_TIMEOUT)`; `respect_rate_limit()` becomes
`let _ = net::rate::wait_for_slot(RateLimitKey::MusicBrainz, &mut || false);` and
`pub(crate) fn wait_for_request_slot(cancelled)` becomes a one-line wrapper over
`wait_for_slot(RateLimitKey::MusicBrainz, cancelled)` (callers: `cover_download.rs:483`,
`network.rs:111`) — or the callers call `net::rate` directly and the wrapper goes; either way the
observable order stays. Delete `LAST_REQUEST`, `MIN_REQUEST_INTERVAL`, `request_delay`,
`respect_rate_limit_with`, the private `lock_unpoisoned` and the three moved tests
(`:249,:256,:270,:307` — `:256 request_delay_enforces_one_second_interval` is covered by
`request_delay_matches_service_contracts`; if you prefer to keep its name, keep it in `rate_tests.rs`
asserting the same 1 s contract). `musicbrainz::user_agent()` (`:45`, `pub`) and `CONTACT_URL` (`:27`)
move to `net::client`; grep the workspace for `musicbrainz::user_agent` and `musicbrainz::CONTACT_URL`
— the inventory found callers only inside core (`cover_download.rs:484`, `lrclib.rs:456`,
`netease.rs:293`, `deezer.rs:156`, `network.rs:89`); point them at `crate::net::user_agent()`.
`cover_download::http_get_bytes` builds `build_agent(AgentPolicy::strict(HTTP_TIMEOUT))`.

**B5 — lyrics.** `lrclib::fetch` builds `build_agent(agent_policy())` with
`AgentPolicy::source(HTTP_TIMEOUT)` (8 s, status errors off); `netease::fetch_url(.., timeout)`
builds `build_agent(agent_policy(timeout))` with `AgentPolicy::strict(timeout)`. Both limiters become
`wait_for_slot(Lrclib|Netease, &mut || false)` after the fixture check, as today. Delete the two
`LazyLock` statics, the `REQUEST_INTERVAL` constants and the two `wait_for_request_slot` fns. The
breaker calls are unchanged apart from the import path (done in B2).

**B6 — deezer.** `AGENT.get_or_init(|| build_agent(agent_policy()))` with
`agent_policy() = AgentPolicy { timeout: HTTP_TIMEOUT, status_as_error: true, https_only: true,
max_redirects: None, proxy_from_env: true }`. `respect_rate_limit()` becomes
`wait_for_slot(RateLimitKey::Deezer, &mut || false)`. Delete `LAST_REQUEST` and `MIN_REQUEST_INTERVAL`.
`is_deezer_image_url` and its test stay.

**B7 — source artwork.** `fetch_with_resolver` calls
`build_agent_with_resolver(agent_policy(), PublicOnlyResolver { inner: resolver })` with
`agent_policy() = AgentPolicy { timeout: HTTP_TIMEOUT, status_as_error: false, https_only: false,
max_redirects: Some(0), proxy_from_env: false }`. `PublicOnlyResolver`, `validate_remote_url`,
`is_public_ip` and both tests stay in `source_artwork.rs`. Move the `DefaultConnector` import to
`net/client.rs`.

**B8 — library doctor `network.rs`.** `NetworkProvider::new()` builds
`build_agent(agent_policy())` with `AgentPolicy::source(HTTP_TIMEOUT)` (15 s, status errors off).
`acoustid_request`: replace `rate_limit(&LAST_ACOUSTID, ACOUSTID_INTERVAL, control)` with
`cancellable_sleep(net::rate::reserve_slot(RateLimitKey::AcoustId), control)` — the reservation stays
on cancel, as today. The MusicBrainz path (`:110-111`): today it calls `rate_limit(&Mutex::new(None),
Duration::ZERO, control)?` (a no-op limiter that still polls `control()` once inside `cancellable_sleep`)
and then `musicbrainz::wait_for_request_slot(cancelled)`. Keep the same sequence of `control()` polls:
replace the first call with `cancellable_sleep(Duration::ZERO, control)?` and read `cancellable_sleep`
to confirm it polls once for a zero duration; if it does not poll at all for zero, drop the call. Delete
`LAST_ACOUSTID`, `ACOUSTID_INTERVAL`, the `cfg(test)` `MUSICBRAINZ_INTERVAL`, `rate_limit`, the local
`request_delay` and the private `lock_unpoisoned`. `cancellable_sleep`, `WAIT_SLICE` and
`cancellation_interrupts_wait_and_backoff` stay. The two moved tests leave `network_tests.rs`.

**B9 — the gate.** In `scripts/check-architecture.sh`, inside the `== Engine HTTP boundaries ==` block
only:

- `http_boundary_budget=5` (1 in `net/client.rs` + 4 excluded). If your measured count differs, the
  budget equals your count and your final message says why.
- Rewrite the comment above it: the boundary now exists at `crates/reprise-core/src/net/client.rs`;
  the remaining four are the scrobbling family (own identity, own auth rhythm); the budget still
  ratchets in both directions. Change the failure hint from "(docs/plans/consolidation-plan.md,
  package 2.1)" to "route it through crates/reprise-core/src/net/client.rs". **Never cite a
  `docs/plans/…` path from this script**: the "Documentation references from code" gate fails when a
  cited plan is deleted on landing, and wave plans are deleted on landing.
- Add, directly after the budget block, an allowlist assertion modelled on `check_frontend_allowlist`
  (lines 327-347) but over `crates/reprise-core/src`: every file matching
  `ureq::Agent::(config_builder|new_with_defaults|new_with_config|with_parts)` must be one of
  `crates/reprise-core/src/net/client.rs`, `crates/reprise-core/src/scrobbling.rs`,
  `crates/reprise-core/src/scrobbling/lastfm.rs`, `crates/reprise-core/src/library/lastfm_stats.rs`,
  `crates/reprise-core/src/library/listenbrainz.rs`, `crates/reprise-core/src/podcasts/stream_proxy.rs`.
  Message: `"<file> constructs a ureq agent outside the net boundary"`. Keep `scripts/check-shell.sh`
  (shellcheck) green.
- `scripts/tests/qa-linters.sh:214-223` pins other patterns in this script; none of them is in this
  block. Run it anyway.

## Known traps

- **`significant_drop_in_scrutinee` is a workspace lint (`Cargo.toml:46`).** Never put
  `lock_unpoisoned(..)` or a `MutexGuard` in a `match`/`if let` scrutinee. Bind the guard with `let`,
  copy the `Option<Instant>` out, drop the guard (end of block) before any sleep.
- **`allow_attributes_without_reason` is on.** Any new suppression needs `reason = "…"`. Do not delete an
  existing suppression in a touched file as "unfulfilled" without checking it under the CI toolchain:
  CI runs a newer clippy (1.99) than local (1.97); what is unfulfilled locally may fire there.
- **Feature-gated builds.** `#[cfg(any(test, feature = "test-fixtures"))]` must move with the fixture
  code. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` as well as the plain
  gate: `reprise-mcp` enables `reprise-core/test-fixtures` in its dev-dependencies, and that is the
  configuration that compiles the seam.
- **lrclib/netease fixture seams are not gated.** They compile in release builds today. Leave them
  that way (gating them would remove a seam from release builds — a behaviour change). Note it in your
  final message as wave-4 material.
- **Limiter position relative to the fixture branch** differs per provider (table 2). Moving a
  `wait_for_slot` call across a fixture branch changes fixture-mode timings, which `scripts/check-lyrics-smoke.sh`
  and the mcp integration tests exercise.
- **Do not sleep in tests.** The old `fetch_respects_rate_limit` injected `sleep`; the new tests use the
  pure `reserve`. A real sleep in a unit test is flakiness under CI load.
- **The `cfg(test)` `slot(key)` accessor** must be `#[cfg(test)]`; otherwise clippy flags dead code in
  the non-test build.
- **`cargo doc -D warnings`.** Doc comments with intra-doc links (`[`AgentPolicy`]`) must resolve; the
  `net` module is `pub(crate)`, so link only to items in scope.
- **File sizes.** Every touched file ends below 800 lines. Current: `network.rs` 742, `cover_download.rs`
  720, `lrclib.rs` 550, `podcasts/http.rs` 541 — all shrink. Keep `net/rate.rs` and `net/client.rs`
  under 300 each by keeping tests in the sibling files.
- **Ownership.** No `Cargo.toml` changes (ureq is already a core dependency; `unversioned` resolver
  types are already used by `source_artwork.rs`). No change to `reprise-stems`' own `ureq::get`
  (`crates/reprise-stems/src/provision.rs:368`) — out of scope, note it.
- **English everywhere**, focused commits, no agent attribution lines.

## Verification

Run from the worktree root, in this order; every command must pass:

```
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo clippy --all-targets --workspace --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo test -p reprise-core
cargo test -p reprise-mcp -p reprise-cli            # the mcp integration tests set the fixture variables
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # must print nothing
scripts/check-architecture.sh                        # budget 5, allowlist green
scripts/check-shell.sh
scripts/tests/qa-linters.sh
scripts/check-lyrics-smoke.sh                        # exercises the lrclib fixture seam and the request log
rg --count-matches 'ureq::Agent::config_builder' crates/reprise-core/src   # must list exactly the 5 files
rg -n 'LAST_REQUEST|LAST_ACOUSTID|MIN_REQUEST_INTERVAL|REQUEST_INTERVAL|sources_http' crates/reprise-core/src   # must print nothing
```

Do not run the unfiltered `reprise-gnome` test suite. The orchestrator runs
`scripts/check-merge-readiness.sh` after the code phase. Report the final `config_builder` count,
every table row where the code disagreed with this plan, and the list of deleted statics.
