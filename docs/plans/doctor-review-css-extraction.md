---
slug: doctor-review-search-prereq
worktree: /home/marvin/Projects/reprise-doctor-review-search-prereq
branch: feature/doctor-review-search-prereq
phase: shipped
codex_session:
created: 2026-08-15
---

# Extract the Doctor review CSS out of `library_doctor/mod.rs`

The prerequisite PR of [`doctor-review-search.md`](doctor-review-search.md) §3.
It lands **on its own, before** the search feature branch, and the search branch
is cut from the commit that merges it.

## Why it is its own PR

`crates/reprise-gnome/src/ui/library_doctor/mod.rs` was **781** lines.
`scripts/check-architecture.sh:20` is `if (( lines >= 800 ))` — a file fails
**at** 800, so the budget is ≤ 799 and `mod.rs` had **18** lines of headroom.
Tasks 8 and 10 of the search plan both need room there.

Separating it buys three things: the diff is tiny and mechanical, so it reviews
in one pass; `check-architecture.sh` is proven green before a line of feature
code exists; and a later gate failure on the feature PR is then unambiguously
the feature's. Precedent: #506 (`9fecc6d8f5`) ran exactly this kind of
extraction as its own change.

## The change

- New `library_doctor/review_css.rs` with `pub(super) fn css() -> String`,
  mirroring `start_page_css.rs`: an array of `&'static str` rules, `.join(" ")`.
- Moved out of `mod.rs`'s `css()` (`:99-141`) **verbatim and in order**:
  `:109-130` (the `.doctor-album-*`, `.doctor-review-*`, `.doctor-current-empty`,
  `.doctor-card-*` rules, including the comments at `:125-126` and `:128-129`)
  and `:132-138` (`.doctor-review-meta*`, `.doctor-review-stale`,
  `.doctor-review-footer*`, `.doctor-review-apply`). 29 rules.
- **Left behind:** `:101-108` (`.doctor-conflicts-*`, `.doctor-conflict-*`) and
  `:131` (`&start_page_css::css(),`).
- `mod review_css;` beside `mod start_page_css;` (`:19`), and
  `&review_css::css(),` spliced where `:109` was.

The joined string changes **order** — `start_page_css`'s rules now follow the
review rules instead of sitting between them — but not content. Safe here and
only here: every moved selector (`.doctor-album-*`, `.doctor-review-*`,
`.doctor-card-*`, `.doctor-current-empty`) is disjoint from every
`start_page_css` selector (`.doctor-start-*`), so no equal-specificity rule can
win differently.

## Its own net

Nothing in the tree asserted these rules existed. `review_css.rs` brings a test
asserting `crate::ui::library_doctor::css()` still contains
`.doctor-album-check`, `.doctor-review-row`, `.doctor-card-accent`,
`.doctor-review-footer`, `.doctor-review-apply` (moved), plus `.doctor-start-run`
and `.doctor-conflicts-dashed` (must **not** have moved).

## Proof

- `scripts/check-architecture.sh` green with `mod.rs` at **754** (781 − 29 + 2).
- `cargo test --locked --workspace --exclude reprise-platform-linux` unchanged
  from the same command on `origin/dev`.

## Landing

Lands first. The search branch `feature/doctor-review-search` is cut from this
branch's tip and is rebased onto `dev` once this merges.
