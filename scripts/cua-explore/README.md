# Exploratory UX agents

This opt-in harness lets a code-blind agent explore Reprise like a curious or
impatient user. It records every interaction as before/action/after CUA evidence
and applies deterministic UX oracles to the resulting accessibility snapshots,
geometry, timings, pointer results, and process logs.

It is intentionally not part of ordinary CI. A maintainer runs it on a clean
candidate snapshot before promoting `dev` to `main`, reviews the evidence, and
decides whether an observation is a defect. Timing and layout heuristics remain
advisory until a finding reproduces in two fresh profiles.

## What it probes

The mission deck gives agents different goals and user mindsets:

| Mission | Persona and pressure |
| --- | --- |
| `first-time-exploration` | A first-time listener with no Reprise vocabulary discovers local playback and reports unexplained states. |
| `section-search-isolation` | A curious source-switcher checks that search follows the visible section instead of silently affecting Music. |
| `pointer-layout-reachability` | An impatient pointer user changes window sizes and probes handlers, hit targets, overlays, misrouted clicks, scrolling, and late layout movement. |
| `offline-recovery` | A commuter loses and regains connectivity while local music and network-backed sources remain open. |
| `large-library-stress` | A power user works in 100,000 generated rows, selects 512 independent writable fixtures, changes metadata, cycles sorting, combines filters, and scrolls aggressively. |

The oracles look for runtime criticals and panics, inaccessible or invisible
actions, semantic actions with no visible effect, semantic/pointer disagreement,
pointer occlusion, misrouted clicks, reversed or jumping scroll, lost selection,
main-loop stalls, slow feedback, waiting without visible status, uninvited layout
shifts, and offline transitions that break local-music behavior.

These are anomaly detectors, not substitutes for product judgement. Expected
animation, resize adaptation, and progress movement are excluded by the oracle
contracts. Real Wayland rendering, physical input devices, audible playback,
portals, media keys, and actual network behavior still require the manual GNOME
pass in `RELEASING.md`.

## Run it

List or validate missions without building or launching the app:

```sh
scripts/cua-explore/run.sh --list-missions
scripts/cua-explore/run.sh --validate-only \
  scripts/cua-explore/missions/offline-recovery.json
```

Run the built-in seeded, code-blind explorer from a clean worktree:

```sh
evidence_root="$PWD/target/cua-explore-evidence"
scripts/cua-explore/run.sh \
  scripts/cua-explore/missions/first-time-exploration.json \
  "$evidence_root/first-time-seed-11" --seed 11
```

Every output directory must be new. Repeat a suspicious observation with a
second seed and fresh generated profile before treating it as confirmed:

```sh
scripts/cua-explore/run.sh \
  scripts/cua-explore/missions/first-time-exploration.json \
  "$evidence_root/first-time-seed-29" --seed 29
```

The built-in explorer is useful for reproducible first-time and pointer coverage
and regression baselines. It follows an activation of a visibly asynchronous
control with an explicit waiting-status observation, and uses real pixel
dispatch for the pointer mission. The large-library, section-search, and
offline-recovery missions refuse to start without a reasoning agent. Their
structured batch/sort/filter work, source routes, offline/restart/reconnect
sequence, and distinct seeded Music, Podcast, and Radio rows are audited from
retained actions and states rather than accepted as agent claims. Attach one
with an explicit JSON argument vector; the harness never invokes a shell:

```sh
agent_argv='["/absolute/path/to/reprise-ux-agent","--jsonl"]'
scripts/cua-explore/run.sh \
  scripts/cua-explore/missions/large-library-stress.json \
  "$evidence_root/large-agent-1" \
  --profile release --agent-command-json "$agent_argv"
```

The external process receives one JSON object per line containing the mission,
persona, latest normalized observation, and at most 20 recent actions. It must
return exactly one typed action per line. The accepted action vocabulary is
`activate`, `type` using named fixture tokens, `press`, bounded `hotkey`,
`scroll`, `resize`, `restart`, `set-connectivity`, explicit `wait`,
`complete-workload`, and `finish`. A checkpoint is accepted only when the
retained trajectory satisfies that workload's required action pattern. Offline
checks additionally require each cached source row exactly once during the
offline visit and after reconnect. The batch checkpoint is audited
independently: exactly 512 private database rows must carry the pinned genre and
year, exactly 512 disposable FLAC copies must have changed, and no other row may
carry those values. Reprise does not show a selection count outside the tag dialog; the batch audit therefore accepts the dialog title as evidence—the missing display is an open UX finding, not a harness property. Exhausting the action budget without an explicit `finish`
is a failed run. Arbitrary text,
shell commands, destructive targets, stale element indices, unknown actions,
URLs, and exhausted budgets fail closed. The agent process receives only a
small allowlisted environment and a disposable `HOME`; it is nevertheless an
explicitly trusted executable, not a filesystem sandbox.

## Evidence

Each retained run contains:

- `run-manifest.txt`: commit, mission, profile, seed, isolation facts;
- `summary.json`: counts and the explicit advisory status;
- `report.md`: human-readable findings with confidence and evidence;
- `trajectory.jsonl`: replayable typed actions and state identifiers;
- `states/`: normalized CUA snapshots and screenshots around interactions;
- `app-*.log`: one application log per launch or restart.

Start review with `report.md`, then inspect the referenced before/action/after
states. A report with no findings means only that no anomaly was observed within
that persona, seed, action budget, and headless environment. It is not proof that
the feature is correct.

`report.py` also exposes pure confirmation and delta-minimization helpers for an
agent adapter that replays a candidate sequence. Confirmation retains only the
same evidence-bearing finding reproduced in at least two independent runs;
minimization removes actions only while the remaining sequence stays valid and
still reproduces the finding.

## Isolation and test data

The launcher refuses a dirty worktree and an existing evidence directory. It
builds the `test-fixtures` feature, creates a private Xvfb display, Openbox,
D-Bus session, AT-SPI registry, CUA daemon, XDG data/cache/config roots, and a
fake audio sink. It never launches on the live Wayland session.

All catalog rows are generated. Writable tag targets are independent copies of
the committed sine-wave FLAC fixture inside a disposable, disk-backed profile
under `~/.cache/reprise-scratch` or the ignored worktree scratch root. Fixture
creation rejects existing paths and every path outside those two approved
scratch roots, including the real Reprise data and Music directories. The 100,000-row
profile materializes only 512 audio copies; the other rows exist solely in the
disposable database to exercise query, sorting, filtering, and rendering limits.

Reprise itself runs in an unprivileged private network namespace, so it cannot
reach host or internet services; an attached reasoning-agent process stays
outside that namespace so its model transport can work. Offline transitions do
not alter host networking. With `test-fixtures` enabled,
the window connectivity boundary watches one file inside the disposable profile
and projects its explicit `online` or `offline` value to all source views and
device sync. This exercises Reprise's offline presentation and recovery without
accounts, credentials, or external requests from Reprise. The mission's online
phase exercises presentation and state transitions, not a successful real
service response.

## Before promoting `dev` to `main`

This is a deliberate maintainer check, not a merge gate. On the exact clean
candidate commit, run at least the five supplied missions in release mode and
attach a reasoning agent for `large-library-stress`,
`section-search-isolation`, and `offline-recovery`. Use two fresh seeds for a new or changed area,
retain the evidence paths with the release notes, and
review every error plus warning. A repeated error blocks promotion until fixed
or explicitly accepted with evidence; a one-off heuristic finding is recorded
for follow-up rather than silently discarded.

Do not add this harness to `.github/workflows`. Large exploratory runs are
expensive, nondeterministic by design when a reasoning agent is attached, and
depend on human UX judgement.
