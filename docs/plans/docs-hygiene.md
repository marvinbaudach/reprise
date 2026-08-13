---
slug: docs-hygiene
worktree: /home/marvin/Projects/reprise-docs-hygiene
branch: feature/docs-hygiene
phase: planned
created: 2026-08-13
---

# The planning docs should not ship, and should not carry a home directory

This repository is public and about to be promoted through the AUR and Flathub.
Measured on 2026-08-13 against `origin/dev`:

- 168 tracked files under `docs/` (5.0 MB)
- 115 of them are planning documents (`docs/plans/`, `docs/superpowers/plans/`)
- **78 tracked files contain the literal string `/home/marvin`** — 47 in
  `docs/plans`, 26 in `docs/superpowers/plans`, 3 in `docs/automation`, 1 in
  `docs/research`, 1 in `docs/agents`
- 16 plans carry `phase: shipped`

Design specs are genuine project documentation and stay, in the repository and
in the release tarball. The planning documents are work orders for one person's
automation: they name worktrees, branches and Codex sessions. They keep their
value inside the working repository — the plan index (`phase:` frontmatter) is
actively consulted before new planning — but they have no business in a source
archive a distribution unpacks.

The file lists below are a **starting point, not a fence**. Adjacent files may
be changed minimally and named in the commit message. Stop only if the
*contract* below turns out to be wrong.

## Task 1 — keep the plans out of release tarballs

Files (starting point): `.gitattributes`

`.gitattributes` already exists and does not yet use `export-ignore`. Add
`export-ignore` entries so `git archive` — which is what GitHub release
tarballs, and therefore AUR and Flathub, are built from — omits:

- `docs/plans/`
- `docs/superpowers/plans/`

**Do not export-ignore `docs/superpowers/specs/`.** Design specs are the good
kind of documentation and belong in the shipped source.

Verify with an actual archive rather than by reading the file:

```sh
git archive --format=tar HEAD | tar -t | grep -c '^docs/plans/'            # must be 0
git archive --format=tar HEAD | tar -t | grep -c '^docs/superpowers/plans/' # must be 0
git archive --format=tar HEAD | tar -t | grep -c '^docs/superpowers/specs/' # must be > 0
```

## Task 2 — retire the shipped plans

Delete every plan under `docs/plans/` and `docs/superpowers/plans/` whose
frontmatter carries `phase: shipped`. There were 16 on 2026-08-13; re-derive the
list rather than trusting that number:

```sh
git grep -l '^phase: shipped' -- docs/plans docs/superpowers/plans
```

Use `git rm`. The history keeps them, so nothing is lost — `git log --diff-filter=D`
still finds them.

**Delete only `phase: shipped`.** Every other phase (`planned`, `coded`,
`reviewed`, `refactored`, or an empty value) means the work is not finished and
the document is still in use. Leave those alone.

Before deleting, check whether any tracked file references a plan being removed
(`git grep -l '<basename>' -- . ':!docs/plans' ':!docs/superpowers/plans'`). A
referenced plan is a contract violation — stop and report it instead of leaving
a dangling link.

## Task 3 — take the home directory out of the docs

Replace the author's home prefix with the standard shorthand throughout the
tracked documentation that survives Task 2:

- `/home/marvin/Projects` → `~/Projects`
- any remaining `/home/marvin` → `~`

`~` keeps every path correct for the author while removing the username from a
public repository. This is a textual substitution — do not rewrite the
surrounding prose, do not "improve" the documents, and do not touch code,
scripts or tests.

Afterwards `git grep -c '/home/marvin' -- docs` must return nothing.

**Check the rest of the tree too, but do not change it blindly.** If
`git grep -l '/home/marvin' -- . ':!docs'` finds tracked files outside `docs/`,
report them in the commit message and in `.pipeline-codex.md`; scripts may need
the absolute path to work, and silently rewriting one would break it.

## Out of scope

- Deleting plans that are not `phase: shipped`.
- Rewriting git history to purge the paths from past commits.
- Any change to `docs/superpowers/specs/`, `docs/assets/`, or `docs/ux-rules.md`
  beyond the path substitution.
- Any change to code, tests, or the build.

## Done when

- The three `git archive` counts above hold.
- `git grep -c '/home/marvin' -- docs` returns nothing.
- `git grep -l '^phase: shipped' -- docs/plans docs/superpowers/plans` returns
  nothing.
- `scripts/check-ux-traceability.sh` still passes — it reads the docs tree and
  is the one gate that could notice a missing file.

Run anything heavy through `heavy-run medium --` and redirect output to a log
file rather than printing it. Do not launch the application.
