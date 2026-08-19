# The development pipeline

Reprise is built by several agents that hand work to one another. This document
is the repository's record of who does which step, and it is the source the
public site's chapter two derives its pipeline figure from — the table below is
parsed at build time, so a change here changes the page or turns a test red.

The executing form lives in `~/.claude/skills/pipeline/`, outside this
repository. That skill is what actually runs; this file is what can be cited.

## The steps

| Step | Phase      | Actor    | Writes | Judges |
|------|------------|----------|--------|--------|
| 01   | Plan       | Opus     | no     | no     |
| 02   | Checkpoint | Human    | no     | yes    |
| 03   | Implement  | Codex    | yes    | no     |
| 04   | Review     | Reviewer | no     | yes    |
| 05   | Refute     | Skeptic  | no     | yes    |
| 06   | Refactor   | Codex    | yes    | no     |
| 07   | Gate       | Gates    | no     | yes    |

## What the columns mean

**Writes** means the actor changes files in the working tree. **Judges** means
the actor decides whether work is acceptable. The two are never true for the
same actor, and that is the whole invariant: *nobody judges their own writing.*

It is worth being precise about where the line falls, because the obvious
guess is wrong. The separation is **not** between the author of a change and
whoever applies the review findings — those are the same actor. Codex runs both
step 03 and step 06, in the same worktree and the same session; handing findings
back to the actor that wrote the code is deliberate, and the pipeline skill says
so in its refactor phase:

> Take the accepted findings … and hand them back to **Codex** in the same
> worktree … Codex implements; Opus plans, reviews and verifies.

Nor do planning and implementation share an actor: Opus drafts the plan in step
01, Codex implements it in step 03.

The line that does hold is between **writing and judging**. Codex writes twice
and judges never. Opus plans, reviews and verifies, and never writes product
code. The reviewer reads a diff it did not produce. The skeptic tries to refute
findings it did not raise. The gates do not care who ran them.

## The human step

Step 02 is the only one a person performs, and it sits before any code exists.
The plan is challenged there — its premises, its cut, its evidence — and that
challenge is the one checkpoint the pipeline will not proceed past on its own.
Everything after it is machine work reviewed by other machines, which is exactly
why the gates in step 07 are the ones that decide, not any actor's opinion of
its own output.

## Gates

Step 07 is `scripts/check-merge-readiness.sh`. Each of its checks is one `gate`
call in that script; nothing else counts as a check, and the preparation steps
before them — refreshing the base ref, requiring a clean worktree, the stale
branch test — are preconditions rather than checks.
