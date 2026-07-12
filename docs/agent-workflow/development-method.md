# Development method — how Reprise is built

This is the working method both agents (Codex, Claude) follow. It is a project-local
distillation of the "superpowers" workflow (brainstorming → writing-plans →
subagent-driven-development → TDD → verification-before-completion → code review). You do
NOT need any plugin to follow it — everything below is plain process against repo files.

## The loop, per feature stage

```
brainstorm  →  design spec  →  implementation plan  →  execute task-by-task  →  stage review
(decide what) (docs/…/specs) (docs/…/plans)          (TDD + gates + commit)   (whole-branch)
```

### 1. Brainstorm (before any code)

Do NOT jump to code. Settle the genuinely-open decisions with the user first — one focused
question at a time, multiple-choice when possible, always with a recommendation. Decompose
anything too big for one plan into sub-stages. YAGNI ruthlessly. Get explicit approval of the
design before writing the spec.

### 2. Design spec → `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`

Written in German (internal design docs are German; code/UI/commits are English). Covers:
goal, scope (explicit in/out), architecture with clear module boundaries, data flow, error
handling, testing strategy, an **"Explicitly NOT doing"** (YAGNI) section, and the project's
non-negotiables (core purity, the "never touch the user's files" promise, privacy). Self-review
for placeholders/contradictions/ambiguity, then get the user to approve the written spec.

### 3. Implementation plan → `docs/superpowers/plans/YYYY-MM-DD-<feature>.md`

Assume the implementer knows Rust but nothing about this codebase. Every task has: exact file
paths, an **Interfaces** block (what it consumes/produces, with real signatures), and
**bite-sized TDD steps with the actual test code and implementation code** — no placeholders,
no "add error handling", no "similar to Task N". A `## Global Constraints` block at the top
carries the non-negotiables verbatim; every task inherits them. Number tasks; state each task's
expected new test count. Draw task boundaries so a reviewer could reject one task while
approving its neighbor.

### 4. Execute — task by task (this is where the code happens)

For each task, in order:

1. **Write the failing test** (from the plan) → run it → **watch it fail** (RED). If it doesn't
   fail, the test is wrong.
2. **Write the minimal code** to pass → run it → **watch it pass** (GREEN).
3. **Run the full gate battery** (see AGENTS.md). All must be green.
4. **Commit** with the plan's exact message. One commit per task. No attribution footer. Never push.
5. **Adversarially review the diff** against the task's spec — spec compliance (nothing missing,
   nothing extra) AND code quality (correctness, the RefCell/generation/purity rules). If you can,
   have a *separate* review pass do this (a fresh reviewer catches what the implementer's context
   hides — in this project that has caught a concurrency bug, a stale-surface bug, and more). Fix
   Critical/Important findings, re-review, then move on. Record Minor findings for the stage review.
6. **Append one line to the ledger** `.superpowers/sdd/progress.md`:
   `Task N: complete (commit <hash>, base <hash>, <note>)`.

Execute continuously — don't stop to ask "should I continue?" between tasks. Stop only when
blocked, genuinely ambiguous, or the plan is done.

### 5. Stage close-out (after the last task)

Re-run the full gate battery + the core-purity proof + the isolated headless E2E battery. Do a
**whole-branch review** (the broad, cross-task pass a per-task review can't see — e.g. does any
state path leave two surfaces inconsistent?). Triage the deferred minors. List the manual
(human) checks headless can't cover. Record the stage complete in the ledger.

## Iron rules (do not negotiate these away under pressure)

- **TDD:** no implementation before a failing test you watched fail. Wrote code first? Delete it,
  write the test, start over. "Too simple to test" is how simple code breaks.
- **Verification before claiming done:** never say "passing"/"fixed"/"works" without having run
  the command and seen the output. Evidence before assertions.
- **Gates are hard gates:** fmt, clippy `-D warnings`, `cargo test --workspace`, `cargo audit`,
  core-purity, `< 800` lines per file. A red gate blocks the commit — fix it properly, don't
  weaken a test or trim docs to pass.
- **Isolation is safety:** every headless run fully isolated (own display + own data/cache + own
  bus). See AGENTS.md — the exact command is there. This project's real DB was damaged twice by
  skipping it.
- **Surface problems honestly:** if a test fails, say so with the output; if you skipped a step,
  say that; if you touched something you shouldn't have, report it and fix it. Don't bury it.

## When picking up mid-stream

Read `.superpowers/sdd/progress.md` + `git log`. The last un-`complete` task in the newest plan
is where you resume. Trust the ledger + git over any recollection.
