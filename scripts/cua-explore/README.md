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
| `hover-affordance-sweep` | A reviewer points at every named button-like control before clicking anything. |
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

After every launch and every restart the runner polls `get_window_state` until
the snapshot is undegraded and holds more than the bare window element. The X11
window exists before the app registers with the AT-SPI registry, and a run that
starts in that gap stays blind for its whole duration. The wait is condition
based, capped at 60 seconds, and reports the driver's `degraded_reason` when the
cap is reached.

Element positions do not come from cua-driver. Under X11/Xvfb the AT-SPI SCREEN
coordinate space reports (0, 0) for every node - measured on Reprise (170 of
170) and on gnome-calculator (107 of 107) - so the driver adds the window origin
and lands every element on the same pixel. The harness therefore walks the
accessibility tree itself in WINDOW coordinates, starting at the frame child of
the application - the application node carries no component interface, and
starting there would shift both walks by one level - normalises against the
frame node, and adds the window origin from `list_windows`. The frame node's
own WINDOW rectangle is retained per snapshot in `summary.json` under
`geometry_measurements[].calibration`:
normalising against it is right exactly when it is the same rectangle as the
`list_windows` entry, and the sizes are the test for that. Geometry is resolved per element, not per tree: cua-driver returns one entry
per indexed row - measured 180 against 485 nodes in the full walk, and not a
cap, since its `max_elements` defaults to 5 000 - so the two trees are never
the same shape. Elements are grouped by their own key of role, label, width and height. A group
resolves only when the driver and the walk hold the same number of nodes for
that key, and they are then paired in walk order - sound because both enumerate
the same tree in pre-order and the driver's elements are a subset of the walk,
so equal counts on a subset mean the sets are identical. Anything else leaves
that group alone without geometry while the rest keep theirs. Pairings inside a
group are counted separately as `resolved_ordered`, because they rest on that
subset argument rather than on the key alone. `subset_violations` counts the
elements in groups where the driver reports *more* nodes than the walk can see:
those never pair, and a non-zero count is evidence against the subset argument
everywhere else too. The
position oracles skip untrusted elements. `summary.json` records one entry per
snapshot under `geometry_measurements`, named by executor generation and state.
Each successful entry carries its own resolution quota (resolved, unmatched,
ambiguous, degenerate, outside the window) and calibration; each failed entry
names the reason. The report renders generations separately, so a clean restart
cannot overwrite an earlier untrusted measurement.

The two driver tools take pixels in different spaces, measured from their own
schemas: `move_cursor` with `scope=desktop` takes desktop coordinates, while
`click` takes x/y together with `window_id` in full-window space. Every pixel
click therefore goes through `window_pointer_point`, which subtracts the window
origin exactly once and then asserts the point lands inside the target's own
rectangle. A point outside its target is a coordinate-space error in the
harness, not a measurement, so it fails loudly rather than returning a number
that looks like a finding.

Positions are only ever compared where both sides were measured. An element
whose geometry could not be resolved carries the driver's placeholder frame at
the window origin, and comparing that against a real measurement reported a
toast's buttons as moving 1051 px while they had not moved at all - six
reproduced `uninvited-layout-shift` findings that were entirely that artefact.
The same guard applies to the scroll-direction median, where a single
placeholder row is enough to invert the verdict.

Tests for the explorer start from `scripts/tests/fixtures/hover-sweep-observe.json`,
a verbatim recording of cua-driver output from a live run, and drive the public
`propose` entry point with the action gateway in the loop. Hand-written
fixtures disagreed with the driver three times running - role spellings, the
element container, the missing `actions` list - and each time the suite stayed
green while the real run did nothing. Role spellings live in exactly one place,
`ui_vocabulary.ROLE_ALIASES`; an unknown one still falls through unchanged and
fails where it is used.

The sidebar sections are not exposed to accessibility at all, so the sweep
measures the view it starts in first and records any section it cannot reach as
`reachable: false` instead of aborting the run.

The hover sweep points at every visible, enabled, actionable element whose role
has a hover contract - buttons and links strictly, rows, cells, tabs, chips and
tiles softly - and only when its position was actually measured, since a
placeholder frame would hover some other part of the window. Role spelling
comes from the hover rulebook rather than the mission's list, because the driver
answers with its own ("push button" for "button"). The sweep is bounded by the
mission's action budget, spread across the sections that actually have an
accessible handle - a section that can never be visited gets no share. The
reserve is the free exploration before the workload starts, one activation per
reachable section, the checkpoint and the finish, plus a small margin. On the
recorded snapshot that is 31 distinct targets in 44 actions against a budget of
220, so the mission budget already covers the surface; `summary.json` records
`hover_coverage` per section - candidates, reached, and how many were left to
the budget - so a partial sweep is a stated result and never a silent one.

The cursor exclusion box is switched by measurement, not by assumption. Once
per launch the runner parks the pointer, moves it away and parks it again,
comparing the parked region across the three captures: a drawn pointer
disappears and comes back, a moving interface does not return to the same
pixels. Only when the pointer really lands in the capture does the hover oracle
exclude it - a blanket 48 px box blinds every icon button smaller than itself,
which is exactly what the hover rule is about. The measurement is retained as
`cursor-visibility.json` and in `summary.json` under `cursor_visibility`.

The same label appears several times in the tree - a cell, a button and a
toggle button can all read "Add filter" - and only one of them carries the
AT-SPI action. cua-driver reports no actions at all, so picking a target by
role landed on a shell that never had one and the app was blamed for it. The
accessibility walk reads the Action interface per node and the matcher hangs it
on the driver element that owns it, over the same bridge as the geometry.
Exactly one candidate with an action is the target; several refuse the choice;
none is its own finding, `no-accessible-action`, because a control that offers
assistive technology nothing to invoke is a real fault and a different one from
an action that fires and does nothing. `suspected-no-handler` now means only
the latter.

`suspected-no-handler` cannot tell two different faults apart, because the
explorer activates over AT-SPI: either the control does nothing at all, or only
its accessibility action is unwired while a real click works. The click probe
drives one control both ways and contrasts what each changed - state signature,
changed pixels inside the element, and the rating stars, which are individual
buttons labelled with a glyph:

```sh
scripts/cua-explore/run.sh --click-probe "☆" \
  scripts/cua-explore/missions/first-time-exploration.json \
  "$evidence_root/click-probe-1"
```

Neither row changing means the control does nothing. Only the pixel row changing
means assistive technology is offered an action that goes nowhere. The pixel
route is refused outright when the element's position was not measured, and a
target below 8 px carries a warning rather than a silent coin toss. Retained as
`click-probe.json` with two screenshots per route.

Every element-addressed action carries the `element_token` from the exact
`get_window_state` response that exposed its target. If a driver snapshot lacks
that preferred handle, the harness sends `element_index` together with the same
snapshot's `snapshot_id`; it never sends a bare index. This contract also covers
targeted `type_text`, `press_key`, and `scroll` calls. A token whose embedded
snapshot disagrees with the response's `snapshot_id` fails before dispatch, so
a stale handle cannot be rewritten to look current - and so does a token from a
response that carries no `snapshot_id` at all, because there is nothing to
check it against. The click probe marks `dispatched` only when the returned
action payload actually confirms dispatch; a row that was never dispatched says
so in its note and carries no verdict about the product.

Before trusting any hover verdict, settle whether a pointer move reaches the
app at all. The probe places the pointer on one named control twice - once
through cua-driver's `move_cursor`, once through a real X11 warp - and prints
the changed-pixel ratio inside the element's rectangle plus where X actually
reports the pointer:

```sh
scripts/cua-explore/run.sh --hover-probe "Add filter" \
  scripts/cua-explore/missions/hover-affordance-sweep.json \
  "$evidence_root/hover-probe-1"
```

The table puts `driver_frame` next to `measured_frame`: if two differently
sized controls share a `driver_frame`, that column is a placeholder rather than
a position. `x11_cursor` is the ground truth. If `move_cursor` leaves it at the park point,
the driver never moved the real pointer. If both routes land on the target but
only the X11 row changes pixels, `move_cursor` draws an overlay and the hover
path needs `xdotool`. The table is printed and retained as `hover-probe.json`
next to the two screenshots per route.

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

## The bundled reasoning agent

`agents/reprise_ux_agent.py` is the credential-free agent for
`section-search-isolation`, `offline-recovery`, and `large-library-stress`. It is a
seeded deterministic state machine: declarative workload phases are resolved against each
fresh observation, every action passes a local copy of the protocol gate, and missing
affordances get two observations plus a bounded alternate before a named note is retained.
It does not use a model, network transport, API key, or credential environment variable.

Pass the seed in the explicit agent argv; `run.sh --seed` controls only the built-in
explorer:

```sh
agent_argv="[\"/usr/bin/python3\",\"$PWD/scripts/cua-explore/agents/reprise_ux_agent.py\",\"--seed\",\"11\"]"
scripts/cua-explore/run.sh \
  scripts/cua-explore/missions/section-search-isolation.json \
  "$evidence_root/section-search-agent-11" \
  --agent-command-json "$agent_argv"
```

Agent observations and findings are condensed into the finish reason and written in full
to the disposable `HOME` as `agent-notes.jsonl`; the runner retains that file under
`evidence/agent/`. The vocabulary-only `agents/probe_agent.py` records up to 15 normalized
observations for the maintainer calibration run without attempting workload coverage.

## Evidence

Each retained run contains:

- `run-manifest.txt`: commit, mission, profile, seed, isolation facts;
- `summary.json`: counts and the explicit advisory status;
- `report.md`: human-readable findings with confidence and evidence;
- `trajectory.jsonl`: replayable typed actions and state identifiers;
- `states/`: normalized CUA snapshots and screenshots around interactions;
- `app-*.log`: one application log per launch or restart.
- `agent/*.jsonl`: retained bundled-agent notes or vocabulary observations, when present.

## Hover acceptance

The `hover` action moves the real desktop pointer after a driver preflight, captures a
baseline and hovered PNG, compares only the target rectangle, and returns the pointer to a
safe park point. Buttons and links without a visible change are errors; rows and cells are
warnings, while unsupported images or geometry produce informational evidence. Run the
single-dispatch safety check with `run.sh --hover-smoke MISSION OUTPUT`. Authoritative
sweeps use `--gtk-animations off`; compare an optional `on` run with `hover_compare.py` to
find affordances that disappear when GTK animations are disabled. Icon buttons without an
accessible name remain outside the sweep and are reported by the accessibility oracle.

The agent-free `hover-affordance-sweep` mission visits Music, Queue, Playlists, Podcasts,
YouTube, Radio, and My Stats in that order. In each section it hovers at most 28 named,
actionable, enabled, visible button-like elements, sorted by geometry and label. Its
workload is complete when every section was reached, the configured minimum target count
was hovered, and at least one hover per section produced measurable screenshots. Hover
findings are the mission result and do not themselves make the workload incomplete.

Start review with `report.md`, then inspect the referenced before/action/after
states. A report with no findings means only that no anomaly was observed within
that persona, seed, action budget, and headless environment. It is not proof that
the feature is correct.

`report.py` also exposes pure confirmation and delta-minimization helpers for an
agent adapter that replays a candidate sequence. Confirmation retains only the
same evidence-bearing finding reproduced in at least two independent runs;
minimization removes actions only while the remaining sequence stays valid and
still reproduces the finding.

## What counts as an affordance

GTK4 hangs `listitem.scroll-to` on *every* row and *every* cell of a
`GtkColumnView`, `list.select-all` on every list, and the `win.*` / `window.*` /
`default.*` GActions on the window itself. None of those is a thing a user can
do; they are the instructions assistive technology uses to move around. Counting
them as affordances made all 52 table cells "actionable", and the first two
cells sharing a label then ended the run.

The rule lives in exactly one place, `ui_vocabulary.py`:

- **structural** — matches `STRUCTURAL_ACTION_PREFIXES`. Never an affordance.
- **invocable** — everything else. `MEASURED_INVOCABLE_ACTIONS` documents what
  has actually been observed (`click`, and nothing else across 1020 recorded
  snapshots of the 2026-08-10 night run).

An **unknown** name counts as invocable — the same choice `ROLE_ALIASES` makes
for unknown role spellings: visibly wrong beats silently blind. Every unknown
name is counted in `summary.json` under `unknown_action_names` and raises one
`unknown-action-name` finding per run, so the next measurement can classify it.
`app.*` is deliberately *not* in the prefix list: it never appeared in any
recording, and this list documents what the app emits, not what it might emit.

Note that a role can still make an element actionable on its own
(`ACTIONABLE_ROLES`). The table's column header keeps counting as a target
because its role is `row` — what changed is the accusation: with no invocable
action it now yields `no-accessible-action` ("there was nothing to invoke")
instead of `suspected-no-handler` ("the app did not react").

## Ambiguous accessible names

Two nodes can legitimately share a name — a rating column has 27 buttons called
`★`. Refusing to choose between them used to end the run, which cost five of
twelve runs and roughly 4500 unspent actions in the night of 2026-08-10.

For a *measurement* refusing is right: a position you cannot prove must not be
invented, which is why `atspi_geometry` stays strict. For a *navigation* there
is no wrong answer, only an unrecorded one. So `_target` picks the first match
in reading order (`y`, then `x`, then `element_index`), records which one it
took and which alternatives existed, and raises one
`ambiguous-accessible-name` finding per name and run — a finding about the app,
which is what a screen reader user faces too.

One divergence to know about: `stable_key`'s `occurrence` counter in
`oracles.normalize_snapshot` numbers identical labels in *column* order
(`x`, then `y`), not reading order. Unifying the two would move every element
identity in the harness and belongs with the optional `key` target, not here.

## The mission declares its window

`missions/*.json` may carry `"window": {"width": …, "height": …}`. The runner
applies it once after every launch and restart, before the window origin is
resolved, and measures back what the window manager granted
(`summary.json → window_setup`). More than two pixels of drift raises
`window-size-not-honoured` — a warning, not an abort. If the measurement itself
fails (wmctrl gone, hung, or the window not found) the record degrades to
`achieved: null` plus an `error`, and the same warning is raised: measuring is a
read and gets a bounded retry, resizing is a write and never gets a second one.

This exists because the window size used to be inherited from whatever the app
defaulted to, and that default was below the width at which Reprise closes both
side panels. Every mission that navigates the sidebar was testing a window that
had no sidebar.

`pointer-layout-reachability` declares `1200x800` on purpose: its persona is
"impatient pointer user on a small display". It starts wide and is resized down,
so the collapse and its undo toast are part of what that mission tests. The
other five declare `1600x1000`.

## Semantic dispatch fallback

The reasoning agent switches its effective activation policy from `ax` to `px`
only after three semantic actions were accepted by the driver, produced no
observable effect, and the same targets then responded to pointer dispatch.
That is the `semantic-route-unavailable` environmental finding.

A driver refusal is categorically different. It is a harness contract failure,
raises `DriverError`, aborts the action path, and never reaches the agent as an
ineffective activation. It therefore cannot schedule a pointer retry, increment
the three-attempt fallback counter, emit `semantic-route-unavailable`, or
supersede the accessibility oracle.

## When a run ends

| Class | Example | Behaviour | Exit |
| --- | --- | --- | --- |
| Observation | ambiguous label, missing section, incomplete checkpoint, driver frame that survived a retry | finding, run continues | — |
| Incomplete | budget spent, `mission_complete: false` | full report, valid evidence | 0 |
| Aborted | app died, driver unusable, isolation broken | report as far as it got, `abort_reason` set | 1 |

The exit code answers the only question a shell can answer: did the tool work?
Whether the *mission* reached its goal is in `summary.json → outcome` and in the
aggregate report. An incomplete mission is a result, not a failure — treating it
as one buried every legitimate one in the sweep's failure list.

## Driver faults

`CliTransport` retries a **read-only** call twice (250 ms, 500 ms) on invalid
JSON or a timeout. It never retries an action: a second `click` would be a
second user input and would falsify the run.

A response counts as a success only when it carries the payload
`SUCCESS_CONTRACT` names for that tool - `effect` for an input action,
`elements` for `get_window_state`, `x`/`y` for `get_cursor_position`, and so
on. Listing the known failures was tried first and does not hold: cua-driver
0.19.3 answers exit 0 with at least four shells that are not proven successes:
`{"status":"refused","refusal":{...}}`, a bare
`{"code":"background_unavailable",...}` object with neither of those keys, a
plain human-readable line, and a normal-looking outcome carrying
`escalation.reason == "delivery_failed"`. The first three end the call. The
fourth answers the contract and reports in the same breath that the input never
arrived: it is retained as a fault and marked undelivered, and an undelivered
action never produces a product finding. A tool the harness calls without an
entry in `SUCCESS_CONTRACT` fails too, rather than passing unchecked.

Every failed attempt is retained in `evidence/driver-faults.jsonl` with the
first 2000 characters of stdout and stderr. A parsed response with a refusal or
non-success status is retained there in full even when the process exits 0.
Each failure is counted in `summary.json → transport_faults` and reported once
per run as `driver-transport-fault`. The point is the payload: one malformed
frame killed a 20-minute run on 2026-08-10 and left nothing behind to diagnose.

## Oracles that never fire

`summary.json → oracle_activity` counts, per declared oracle, how often it was
*evaluated* and how often it *fired*. An oracle that never evaluates looks
exactly like a clean product, and nothing used to say otherwise; it now raises
`oracle-never-evaluated`. An oracle that is legitimately superseded (the
`ax`-only branches, once a run has switched to pointer dispatch) carries
`superseded_by` and stays silent without complaint.

## Where the fixtures come from

Never hand-write a fixture. Every one of them is copied verbatim out of a real
run, and two of them exist because hand-written ones disagreed with the driver
three times running.

There are two kinds, and mixing them up is the trap
`cua-explore-fixture-integrity.py` guards:

- **measured** (`night-2026-08-10-*`, `postfix-2026-08-10-sidebar-open`) —
  recorded *after* `CuaExecutor.with_measured_geometry`. Exactly what `_target`,
  `normalize_snapshot`, the explorer and the agent see. Carries `actions` and
  real `frame` values.
- **raw** (`hover-sweep-observe`, `postfix-2026-08-10-search-open`) — straight
  cua-driver output. No `actions` key, every `frame.y` is 0. Good for roles and
  labels, useless for actions.

`hover-sweep-observe.json` predates action injection entirely (2026-08-07). That
is why the ambiguity trap could pass the suite while it was killing real runs.

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
expensive, seed-varying by design when an exploratory agent is attached, and
depend on human UX judgement.
