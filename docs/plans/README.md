# Plans

`AGENTS.md` sets the rule this directory follows: throwaway per-stage
implementation plans do not live in the repository. `.superpowers/sdd/progress.md`
plus `git log` are the authoritative record of what is done and in flight. What
stays here is the exception — a plan that outlives its own execution because
later work has to follow it.

## What is kept

- Plans still being worked: `phase: planned`, `todo`, `coded`, `reviewed`.
- Finished plans that code or a script cites by path. `scripts/check-architecture.sh`
  fails the build when a `docs/…` path named from `crates/` or `scripts/` does not
  resolve, so these are load-bearing and are deleted only together with the citation.
- Contracts that were never plans — `docs/ux-rules.md` and the ADRs — which live
  outside this directory and outrank the code.

## What is dropped once the work lands

- `*.HANDOFF*.md` / `*-handover.md` — session-to-session relay notes. Their content
  belongs to the branch that consumed them.
- Plans reaching `phase: shipped`, `complete`, `dropped` or `reverted`, unless cited
  as above. The commit history holds the plan and its outcome together.
- Evidence directories under `docs/evidence/` whose plan has shipped.

Markdown-to-markdown links to deleted plans are expected and are deliberately not
gated — `scripts/check-architecture.sh` explains why. A dangling plan reference in
another plan is a historical record, not a defect.
