# Agent workflow — shared skills for Codex & Claude

Project-local guidance so **any** coding agent can work on Reprise the same way, without
depending on a specific tool's plugin system. Read these together with the root **`AGENTS.md`**
(which holds the resume instructions, gates, and safety rules).

> **Coordinating two agents (Claude ⇄ Codex): read `STATUS.md` FIRST.** It is the shared,
> git-tracked board for who's working, what's done, and what's next — claim its Lock before you
> touch `main`, release it when done. Only one agent works `main` at a time.

- **`development-method.md`** — how the project is built: brainstorm → design spec →
  implementation plan → task-by-task TDD → per-task review → stage close-out. The iron rules
  (TDD, verification-before-done, hard gates, isolation, honesty).
- **`building-gtk4-rust-apps.md`** — hard-won GTK4/gtk4-rs 0.11 / GStreamer / MPRIS / SQLite
  pitfalls, each a real bug caught in this codebase. Read before touching frontend/platform code.
- **`codex-resume-prompt.md`** — a ready-to-paste prompt to hand a fresh agent (Codex or other)
  so it resumes from the current handoff point with the safety rules, gates, and learnings inline.

## Where the live state and the plans are

- `.superpowers/sdd/progress.md` — the ledger (what's done, commit hashes, deferred minors,
  incidents). Authoritative for "where are we".
- `docs/superpowers/specs/` — design specs (German).
- `docs/superpowers/plans/` — implementation plans (task-by-task, with code + tests).

Start every session by reading the ledger and `git log`, then open the newest plan and resume
at its first un-`complete` task.

## Origin note

`development-method.md` distills the "superpowers" agent workflow (brainstorming, writing-plans,
subagent-driven-development, test-driven-development, verification-before-completion, code
review) into plain, tool-agnostic process. `building-gtk4-rust-apps.md` is the project's own
accumulated GTK4 pitfall list. Both are self-contained — no plugin required to follow them.
