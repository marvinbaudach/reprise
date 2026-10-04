---
slug: issue-sweep-2026-10-03-c
worktree: /home/marvin/Projects/reprise-issue-sweep-2026-10-03-c
branch: feature/issue-sweep-2026-10-03-c
phase: shipped
codex_session:
created: 2026-10-03
---
# Strand c — Radio says when it is turned off, and the desktop retries covers when the network returns (#1018, #1052)

Mother plan: `docs/plans/issue-sweep-2026-10-03.md`. Paths below are under
`crates/reprise-gnome/src/ui/` unless stated.

## #1018 — Radio module-off state

- SRC-10a (`docs/ux-rules.md`, `[active] [gtk]`) gives Podcasts, YouTube and Radio one empty-state
  geometry, and says that when a source's own module is off and nothing is subscribed, the same
  tile/title/body/one-button shape reads "{Source} is turned off" with an "Enable in Preferences"
  button. Podcasts implements it (`podcasts/podcasts_empty_state.rs` `ModuleOff`,
  `podcasts/podcasts_view.rs` `MODULE_OFF_PAGE`, `podcasts_view_copy.rs` `module_off_copy`,
  `strings_podcasts.rs` `PODCAST_SOURCE_OFF_TITLE` / `PODCAST_ENABLE_IN_PREFERENCES`).
- Radio has `RadioEmptyState { List, NoResults, Empty }` and no module check.
- The sidebar hides Radio when its module is off, but an open Radio view stays open when the
  module is switched off in Preferences, and `window/library_shell.rs` / `window/section_search.rs`
  route `ViewSource::Radio` too. That is where the state shows.

**Required behaviour.** With the radio module off, the Radio view shows the SRC-10a module-off
shape: "Radio is turned off", the same body pattern as Podcasts, and one "Enable in Preferences"
button that opens the same Preferences target Podcasts uses. Switching the module back on returns
to the normal state without reopening the view. Reuse the existing copy with the source name; add
strings only if the existing ones cannot carry "Radio", and then add them to the gettext catalogs.

Tests (rule-named, `src_10a_…`): the module-off state for Radio, its button target, and the
return to the list when the module comes back. Mirror `podcasts/podcasts_view_tests.rs` around
line 336.

## #1052 — desktop cover retry on network return (new rule NET-7c)

- `window/source_connectivity.rs` watches `gio::NetworkMonitor` and notifies concerts, releases,
  podcasts, YouTube, radio and preferences. The cover download is not a target.
- `cover/cover_download_batch.rs`: transient failures (transport, worker) stay open "so a later
  pass can retry them", but no pass starts on a network return. `start()` can be suppressed by
  `startup_tasks::begin_exact` on a settled library; `start_user_triggered()` ignores that
  freshness. Progress shows on the sidebar scan card (`cover/main_cover_download_progress.rs`).

**Required behaviour.**

- On an offline-to-online transition seen by the existing connectivity monitor, the cover batch
  starts a retry pass **only if the last pass left at least one transient failure open or ended
  in `failed()`**, Artwork is enabled, and no pass is running. Otherwise nothing happens — in
  particular no scan-card progress on an ordinary network handoff.
- Online-to-online changes are not a return. A return during a running pass does nothing.
- Use the existing start path that retries open entries; do not add a second download path.

**Rule.** Add `NET-7c [active] [gtk]` to `docs/ux-rules.md`, right after NET-6 (strand c is based
on dev, where NET-7a/7b do not exist yet; the rebase after the cover branch lands moves it after
NET-7b). Text, in the house style of the neighbouring NET rules: when the desktop's network
returns after being offline, covers whose download failed only because the network was missing
are tried again in one pass; a return after a pass that left nothing open does nothing visible.
Reference #1052.

Tests (rule-named, `net_7c_…`):

- a return after a pass that left a transient failure starts exactly one retry pass;
- a return after a clean pass starts nothing and shows no progress;
- an online-to-online change starts nothing;
- a return while a pass is running starts nothing.
Use `REPRISE_TEST_CONNECTIVITY_FILE` / the existing test seam in `source_connectivity.rs` if a
test needs the real monitor path.

## Gates

`cargo fmt --check`, `cargo clippy --all-targets --workspace -- -D warnings`,
`cargo test --workspace`, `scripts/check-ux-traceability.sh`, and the gettext checks if strings
were added. Headless runs, if any, use the full isolation recipe from `AGENTS.md`. Every touched
code file stays under 800 lines.

Commit bodies say `Closes #1018` and `Closes #1052` on the respective fix commits.
