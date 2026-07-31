# STYLE-1 — Effect explicit, not inherited (draft, 2026-07-18)

Finished wording for `docs/ux-rules.md`. **Not yet entered:** the file
currently belongs to the running Codex task (sections Q and J). After its
merge, append it as **section S** (P is the last one on main, Q and R arrive
with `feat/search-and-new-releases`).

## Occasion

Four cases in one day, all with a green test, all noticed only in the
screenshot:

| Case | Set | Actually rendered | Cause |
|---|---|---|---|
| Headerbar surface | `@headerbar_bg_color` | `#16181b` (window color) | `ToolbarStyle::Flat` swallows bar backgrounds |
| Search strip | second top bar | appears to float | the same Flat trap |
| Headerbar title | `set_title_widget(NONE)` | "Reprise" centered | Adwaita falls back to the window title |
| Sidebar width | `max-sidebar-width = 240` | 295 px | label without `ellipsize` forces a minimum width |

The common denominator is **not** `Flat`, but: a property that has been set
stays without effect because the default state does something other than
expected — and the test checks the property instead of the result.

---

## Section S. Surfaces & Geometry

Anything meant to be visible must be set explicitly. Inherited or
framework defaults do not count as set: they are the most common reason a
property is set and yet nothing happens.

- **STYLE-1** [planned] [gtk] — **Effect explicit, not inherited.** Every
  surface meant to set itself apart from content (headerbar, revealed
  bars, sidebar edges, panels) carries background **and** separator line
  explicitly; every binding geometry (fixed widths, minimum heights) is
  checked against its actual allocation.
  `flat` stays exactly where **no** separation is deliberately wanted.
  Known traps this rule addresses: `AdwToolbarView` with
  `ToolbarStyle::Flat` suppresses bar backgrounds (including
  `@headerbar_bg_color`); an `AdwHeaderBar` without a title widget renders
  the window title as a fallback (`show-title` must additionally be off); a
  `GtkLabel` without `ellipsize` reports its full text as **minimum** width
  and thereby defeats any `max-width` on the container;
  `AdwOverlaySplitView` computes in `sp` without `sidebar-width-unit = Px`.
  **Test rule:** intent may be checked, but for surfaces and geometry the
  **result** must be proven — not "property X is set", but
  "the surface has a visible background" or "the column stays at its width
  in a narrow window". What the framework guarantees is tested for
  existence; what can fail to appear is tested for effect (the same figure
  of thought as TIP-1a/2a and SEARCH-2).

---

## Addition for `RELEASING.md`

Under the manual acceptance points:

> - **STYLE-1 "floating" test** [manual] — Open every revealable bar
>   (search bar, banner, progress card) once: if it folds flat over the
>   content without a surface and edge of its own, the background is
>   missing — `ToolbarStyle::Flat` has swallowed it. Cross-check in all
>   three dark themes, because the window color is off in a different way
>   in each theme.

---

## Implementation note

When entering it: check the section letter against the `main` state current
at that point (today Q and R would be taken by the search/NR branch, S would
be free) — exactly the way the comment at the head of section O does it.
