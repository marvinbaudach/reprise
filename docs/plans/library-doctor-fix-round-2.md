---
slug: library-doctor-fix-round-2
worktree: /home/marvin/Projects/reprise-library-doctor-fix-round-2
branch: feature/library-doctor-fix-round-2
phase: planned
codex_session:
created: 2026-08-07
---
# Library Doctor — fix round 2

Base: `origin/dev` = `53a9011f76`. Worktree `feature/library-doctor-fix-round-2`.

Round 1 landed as `d35c2396ff` (PR #302) and is on `origin/dev`. **The structure is
there; the policy and the visual hierarchy are not.** This plan fixes nine reported
defects on the summary page, the sidebar job card and the sidebar `ISSUES` block.

Binding repository contracts that outrank this plan: `AGENTS.md`, `docs/ux-rules.md`,
`TESTING.md`. The round-1 plan `docs/plans/library-doctor-redesign.md` is history —
where this plan contradicts it, **this plan wins**, and §H says why.

**Nothing in the scan engine, the MusicBrainz/AcoustID clients or the tag-write
machinery changes** — with exactly one deliberate exception, §E-2, which is a
*reporting* fix inside `scan.rs`, not a scanning-logic change.

---

## A. Design reference and its frame numbers

The commissioning brief names frames "3 (Ergebnis)", "3b", "5 (scan running)", "7".
**Those numbers do not match the mockup file.** The mockup (`Library Doctor.dc.html`,
nine figures on one canvas) numbers them:

| Mockup frame | What it shows | Brief calls it |
| --- | --- | --- |
| 3 | **Scan läuft** (scan running) | "frame 5" |
| 3b | Scan finished while the user is elsewhere | "frame 3b" ✓ |
| **4** | **Ergebnis** — the three result cards | "frame 3" |
| 7 | Nothing to fix (empty state) | "frame 7" ✓ |

Go by the *name*, never by the brief's number. `docs/plans/library-doctor-design-frames.md`
in this repo uses the mockup's own numbering and is correct; it is the only
transcription available inside the repo — read §"Frame 3", §"Frame 3b", §"Frame 4",
§"Frame 7" there. The measured values below are transcribed from the mockup source
and are authoritative for this plan.

### Mockup frame 4 (Ergebnis) — measured

```
main            padding 44px 64px
column          max-width 700px, flex column, gap 24px, flush left
title           h3  "1,648 tracks checked"
subline         13.5px, text@50%   "Whole Library · MusicBrainz on · 7 skipped"

applied card    flex row, gap 16, padding 18px 20px, radius --radius-md,
                background --color-surface, box-shadow --shadow-sm
  icon          20×20, flex none, margin-top 2, colour accent-300, glyph = check
  heading       16px, weight 500, margin-bottom 8   "1,017 fixes already applied"
  lines         13.5px, text@62%, gap 4
                  "208 stray spaces and casing corrections"
                  "809 MusicBrainz IDs filled in — no visible change to your tags"
  action        btn-secondary, align-self flex-start, undo glyph + "Undo"

review card     same padding/radius/background,
                box-shadow 0 0 0 1px accent@45%          ← accent border
  icon          20×20, colour accent (full), glyph = stethoscope
  heading       16px/500   "88 changes need your eye"
  lines         13.5px, text@62%: one per category, then "across 31 albums"
  action        btn-primary, align-self flex-start   "Review 88 changes"

conflicts card  padding 16px 20px, radius --radius-md,
                border 1px dashed text@18%, NO background, NO shadow
  icon          19×19, colour text@45%, glyph = warning
  heading       14.5px (smaller than the other two), margin-bottom 3
                  "3 spelling conflicts, no clear winner"
  body          13px, text@52%   DOCTOR_CONFLICTS_BODY verbatim
  action        none

footer          row, gap 18, margin-top 6
                btn-ghost "Scan again"  +  12.5px text@38% "Results are kept until the next scan."
```

### Mockup frame 3 (scan running) — measured

Sidebar: `LIBRARY / SMART / ISSUES` sections all present, `Missing files` visible, and
the job card pinned at `margin-top:auto` **below** them — one card, spinner ring,
`Checking tracks…` (ellipsized), `45%`, `Cancel` as 12px accent text, a 4px bar, then
`742/1,648 tracks`.

The mockup's running *page* still shows two dimmed blocks with disabled buttons. **The
brief overrides that** (§C-2): the running page is progress only.

### Mockup frame 3b — measured

No job card (scan finished). Sidebar `ISSUES` holds `Missing files` **and** a
Library-Doctor row: accent-tinted background, 1px accent inset border, stethoscope
glyph, label `Library Doctor`, trailing count `88` in accent. A pill toast bottom-centre:
`1,017 tags fixed` + `Undo` + `✕`.

### Mockup frame 7 — measured

`Nothing to fix` (h4) · body 13.5px text@55%, max-width 380px:
`1,648 tracks checked, 7 skipped. Your tags are already consistent with each other.`
· row: btn-secondary `Scan again` + 12.5px text@40% `Whole Library · MusicBrainz on`.

---

## B. Decisions. Nothing here is open.

**U-1 … U-3 were decided by the product owner. Do not re-litigate them.**

- **U-1 — FB-8 is amended: the `ISSUES` block and the progress cards are visible at
  the same time.** Today `docs/ux-rules.md` FB-8 rules that a visible progress card
  *replaces* the whole Issues block; that is why `ISSUES` and `Missing files` are gone
  in the reported screenshot, which was taken mid-scan. The mockup's own frame 3 shows
  both. FB-8 gets rewritten (§G-1), it keeps its id and stays `[active]`.
- **U-2 — the subline under the page title is the mockup's scan-facts line**, not
  `doctor_checked_counts()`: `{scope} · MusicBrainz on|off` and ` · {n} skipped` only
  when `skipped > 0`. Reason: the page title already carries "1,648 tracks checked", so
  `doctor_checked_counts()` would repeat it, and at `skipped == 0` it would print a
  literal `0`, which acceptance criterion 2 forbids anywhere on the page.
- **U-3 — the live counters on the running page are forecasts, and say so.**
  `{n} will be fixed quietly` / `{n} waiting for you`. Reason: per §C-1 the quiet write
  only starts when the scan completes, so mid-scan nothing has been written yet;
  "fixed quietly so far" would claim a past that has not happened. The finished page
  says "already applied" because by then it has.

Engineering decisions, with their reasons:

- **E-1 — partial results of a cancelled scan are discarded** (that is already what
  `mod.rs` does: `DoctorScanOutcome::Cancelled` falls back to the last complete scan or
  to the start page). Therefore no state renders `DOCTOR_RESULTS_SO_FAR`, and that
  string is **deleted** together with its catalogue entries. If a gate insists a string
  stay listed, keep the constant and mark it unused with a comment naming this decision
  — do not invent a screen for it.
- **E-2 — the live preview summary stops counting unresolved groups.** Root cause of
  "14 spelling conflicts" against "27 checked" (§E below): `scan.rs` computes groups for
  **one track at a time** and `DoctorScanSummary::merge` *adds* those per-track counts
  up. A spelling conflict is by definition cross-track, so a per-track count is
  meaningless and summing it inflates without bound. The completed scan recomputes over
  the whole set and is correct. Fix: publish `0` groups in the live preview.
- **E-3 — the summary renders only after the quiet write has finished.** The running
  page covers the scan *and* the quiet write that follows it. That is what removes the
  reported "2 fixes to apply + disabled Undo" state entirely, rather than relabelling
  it. `doctor_fixes_to_apply()` and `doctor_mbid_line_pending()` become unreachable and
  are deleted, along with `AppliedBlock::pending`.
- **E-4 — if the quiet write fails or never runs, the applied card is not rendered at
  all.** Nothing was applied, so nothing may claim it was. The existing failure toast
  (`TAG_WRITE_BUSY` / `DOCTOR_JOB_FAILED`) stays and the sidebar refresh in
  `abandon_auto_apply` stays, so the findings are still reachable.
- **E-5 — the scan facts describe the scan, not the current controls.** Take the scope
  from `scan.scope_kind` and the remote flag from `scan.options.remote_enabled`, never
  from `self.page.remote_active()` or the scope dropdown. (`scan_summary`'s existing
  `remote_visible` parameter keeps its current meaning — do not touch it.)
- **E-6 — `summary_page.rs` is split.** It is 630 lines today and grows here;
  `scripts/check-architecture.sh` caps Rust files below 800 and this directory should
  stay in the 200–400 band. Target files in §C.
- **E-7 — the review card's category lines get a noun so ngettext has something to
  agree with:** `{class} · {doctor_change_count(n)}` → `Casing / Whitespace · 64 changes`.

---

## C. The summary surface

Files today, all under `crates/reprise-gnome/src/ui/library_doctor/`:
`summary_page.rs` (630 lines; holds `SummaryBlocks`, `DoctorSummaryPanel` **and**
`LibraryDoctorPage`), `result_pages.rs` (post-apply + empty), `progress_card.rs`,
`mod.rs` (coordinator), `start_page.rs`.

Target layout (E-6):

| File | Holds |
| --- | --- |
| `summary_model.rs` | `SummaryBlocks`, `AppliedBlock`, `ReviewBlock`, `ReviewLine`, the `DoctorPageState` enum, their unit tests |
| `summary_cards.rs` | the three card builders and the shared card primitives |
| `summary_page.rs` | `DoctorSummaryPanel` + `LibraryDoctorPage` (the state machine) |
| `running_page.rs` | **new** — `DoctorRunningPanel` |

### C-1. The safe fixes are applied, not pending (brief §1)

`mod.rs:348-351` already enqueues the quiet job the moment the scan completes
(`set_scan_pending_auto` → `start_auto_apply`), so the *policy* is right. What is wrong
is that the summary is rendered **during** that write, in a `pending` shape with a
disabled `Undo`. Fix by E-3:

1. Introduce `DoctorPageState` in `summary_model.rs`:
   ```rust
   enum DoctorPageState {
       Start,
       Running { kind: DoctorJobKind, completed: usize, total: usize, live: DoctorScanSummary },
       Summary { scan: DoctorScan, quiet: QuietOutcome },
       Empty { checked: usize, skipped: usize, facts: ScanFacts },
       PostApply,
   }
   enum QuietOutcome { Applied(Option<DoctorWriteReport>), Failed }
   ```
   `LibraryDoctorPage` holds one `RefCell<DoctorPageState>`; `refresh()` becomes a
   single match over it. Delete `partial_summary`, `auto_complete` and `auto_running` —
   they encoded the same thing in three loose flags.
2. `begin_partial_scan` / `set_partial_summary` become `set_running(kind, completed,
   total, live)`; `set_scan_pending_auto` keeps the page in `Running { kind: Apply }`
   instead of rendering the summary; `complete_auto_apply(report)` moves it to
   `Summary { quiet: Applied(report) }`; `fail_auto_apply()` moves it to
   `Summary { quiet: Failed }`.
3. The applied card heading becomes past tense: `doctor_already_applied(n)`.
   `Undo` is **sensitive** whenever the state is `Summary { quiet: Applied(_) }` with
   at least one applied change and no job is running. It is the only control on that
   card. It stays wired to `start_revert()` — verify by hand that reverting really
   restores the tags (acceptance 1).
4. Delete `doctor_fixes_to_apply()`, `doctor_mbid_line_pending()` and
   `AppliedBlock::pending`.

### C-2. Running and finished are two different pages (brief §3)

New `running_page.rs`, `DoctorRunningPanel`, flush left in the same 700px column:

- heading `strings::DOCTOR_SCANNING` for `DoctorJobKind::Scan`,
  `DOCTOR_UPDATING_TAGS` for `Apply`, `DOCTOR_REVERTING_TAGS` for `Revert`
  (css `title-2`, `xalign 0.0`)
- `doctor_track_progress(completed, total)` as a dim line
- a `GtkProgressBar`, fraction from completed/total, a11y label `DOCTOR_PROGRESS`
- the two forecast counters (U-3), **each only when its count > 0**:
  `doctor_will_fix_quietly(n)`, `doctor_waiting_for_you(n)`. Shown for
  `kind == Scan` only — during the quiet write there is nothing left to forecast.
- exactly one button: `strings::CANCEL`, wired through a new
  `LibraryDoctorPage::connect_cancel` to `LibraryDoctorCoordinator::request_cancel`.

Not on this page: `Scan again`, `Review`, `Undo`, `doctor_checked_counts()`,
`DOCTOR_RESULTS_KEPT`, `DOCTOR_CONTROLS_LOCKED`, and any card chrome.

`DOCTOR_CONTROLS_LOCKED` stops being a paragraph anywhere. Per the brief it becomes a
tooltip on the disabled control: set it as the tooltip of `Scan again` and of the
review/undo buttons while a job runs, and clear the tooltip when they are sensitive
again. (The finished page can only be reached with no job running, so in practice this
is the belt to the braces — implement it, keep it cheap.)

`DOCTOR_RESULTS_KEPT` **stays** on the finished page — mockup frame 4 has it next to
`Scan again`.

### C-3. Zero lines do not render (brief §2)

Today `summary_page.rs:191-202` appends the spacing/casing line and the MBID line
unconditionally, which is where `0 stray spaces and casing corrections` comes from.

- every detail line is emitted only when its own count is `> 0`
- a block whose lines are all zero is not emitted
- the conflicts block is not emitted at `0` (already true — keep it)
- all three blocks empty → the existing empty state (`result_pages::show_nothing`,
  mockup frame 7) instead of a page of zeros. `SummaryBlocks::is_empty()` already
  drives this; keep it and cover it with a test.
- the empty state's own body must not print `0` either: split
  `doctor_nothing_to_fix_body` into a variant with the skipped clause and one without,
  and pick by `skipped > 0`.

### C-4. Three cards, not a text column (brief §5)

Delete `block_card()` (a `gtk4::Box` with `boxed-list` — the wrong class, and it puts
the action *under* the text). Build each card in `summary_cards.rs` as:

```
adw::Bin  +  css "card"                    ← the app's own card idiom,
  └ gtk4::Box horizontal, spacing 16,        see device_sync_page_layout.rs:303-304
      margins 18 top/bottom, 20 start/end
      ├ gtk4::Image  pixel_size 20, valign Start, margin_top 2
      ├ gtk4::Box vertical, spacing 4, hexpand true
      │   ├ heading label  css "heading", xalign 0, wrap
      │   └ detail labels  css "dim-label", xalign 0, wrap
      └ action button  valign Start, hexpand false
```

Per-card differences:

| Card | Surface | Icon | Action |
| --- | --- | --- | --- |
| applied | plain `.card` | check, accent-tinted | `Undo`, plain button |
| review | `.card` + `.doctor-card-accent` (1px accent inset border) | stethoscope, accent | `Review {n} changes`, `.suggested-action` |
| conflicts | **no** `.card` — `.doctor-conflicts-dashed` only, no fill | warning, muted | **none** |

- `.doctor-card-accent` is a new rule in `library_doctor::css()`:
  `box-shadow: inset 0 0 0 1px alpha(@accent_color, 0.45);`
- `.doctor-conflicts-dashed` already exists in `library_doctor::css()`
  (`1px dashed @borders`, radius 12, padding 12) — raise its padding to `16px 20px` to
  match the mockup and keep it fill-free.
- accent icon colour: `image.add_css_class("accent")`. Muted icon: `"dim-label"`.
- the conflicts heading is one step smaller than the other two headings (mockup:
  14.5px vs 16px) — use `caption-heading`/`heading` accordingly, whichever the app's
  `text_levels.rs` already offers; do not invent a font size.
- **icons**: use the glyph the app already ships. The sidebar's `NavIcon::LibraryDoctor`
  is `system-search-symbolic` (`sidebar_presentation.rs:49`); use the *same* icon for
  the review card so the page and the sidebar entry agree, unless a stethoscope
  symbolic actually ships with the app — check `crates/reprise-gnome/**/icons` and the
  brand assets first and say in the result summary which you used. Applied card:
  `emblem-ok-symbolic`. Conflicts card: `dialog-warning-symbolic`.

### C-5. Left-aligned, top-weighted (brief §6)

`summary_page.rs:272-280` wraps the stack in `adw::Clamp::maximum_size(760)`, and
`AdwClamp` *centres* its child — that is the reported centring, and
`result_pages.rs:163-190` additionally centres its labels and its `status_box`.

- clamp: `maximum_size(700)`, `tightening_threshold(700)`,
  `halign: gtk4::Align::Start`, `hexpand: false`, `valign: gtk4::Align::Start`
- margins: `top 44`, `start 64`, `end 24`, `bottom 36` (mockup padding 44/64)
- `heading_label()` / `body_label()` in `result_pages.rs`: drop
  `justify(Center)`, set `xalign(0.0)`; `status_box` drops `valign(Center)` and its
  centred action row — the empty state is left-aligned too (mockup frame 7), only the
  post-apply page keeps its centred hero, which is a different frame (6) and is not in
  scope. **Leave the post-apply page alone.**
- nothing on the summary or empty page is centred.

Prove it, do not eyeball it: a display test that measures the first card's allocation
(`x < 120` from the content edge, `width <= 700`), plus the screenshot pass in §I.

---

## D. Grammar (brief §9)

Everything counted on these screens goes through `plural()` (which is
`i18n::ngettext`), never through `formatted()` with an `s` glued on. `plural()` is
`plural(singular, plural, count, values)` and lives in `strings.rs:23`.

Rename and convert in `crates/reprise-gnome/src/ui/strings_library_doctor.rs`:

| Now | Becomes | Forms |
| --- | --- | --- |
| `doctor_fixes_applied` | **`doctor_already_applied`** | `{count} fix already applied` / `{count} fixes already applied` |
| `doctor_changes_need_your_eye` | **`doctor_needs_review`** | `{count} change needs your eye` / `{count} changes need your eye` |
| `doctor_conflicts_headline` | **`doctor_unresolved_spellings`** | `{count} spelling conflict, no clear winner` / `{count} spelling conflicts, no clear winner` |
| `doctor_spacing_casing_line` | same name | `{count} stray space and casing correction` / `{count} stray spaces and casing corrections` |
| `doctor_mbid_line` | same name | `{count} MusicBrainz ID filled in — no visible change to your tags` / `{count} MusicBrainz IDs filled in — no visible change to your tags` |
| `doctor_across_albums` | same name | `across {count} album` / `across {count} albums` |
| `doctor_tracks_checked_heading` | same name | `{count} track checked` / `{count} tracks checked` |
| `doctor_change_count` | same name | already plural — wrap both literals in `N_!` |
| `doctor_review_changes` | same name | already plural — wrap both literals in `N_!` |
| `doctor_apply_changes` | same name | already plural (leave) |

New strings, all wrapped in `N_!`, placeholder style `{name}` like the rest of the file:

```rust
pub const DOCTOR_REMOTE_ON:  &str = N_!("MusicBrainz on");
pub const DOCTOR_REMOTE_OFF: &str = N_!("MusicBrainz off");

pub fn doctor_will_fix_quietly(count: usize) -> String   // "{count} will be fixed quietly"
pub fn doctor_waiting_for_you(count: usize) -> String    // "{count} waiting for you"
pub fn doctor_skipped_facts(count: usize) -> String      // "{count} skipped" / plural
pub fn doctor_scan_facts(scope: &str, remote: &str, skipped: Option<usize>) -> String
    // joins with " · " — "Whole Library · MusicBrainz on · 7 skipped"
pub fn doctor_nothing_to_fix_body_skipped(checked: usize, skipped: usize) -> String
pub fn doctor_nothing_to_fix_body(checked: usize) -> String   // no skipped clause
```

Delete: `DOCTOR_RESULTS_SO_FAR` (E-1), `doctor_fixes_to_apply`,
`doctor_mbid_line_pending`. Update every catalogue/round-trip test that lists them, and
any German `.po`/`.pot` entries — `git grep` each removed identifier and each removed
message string before you finish.

`doctor_checked_counts()` loses its caller (U-2). Check whether anything else uses it
(`review_page.rs`, the MCP DTOs, tests); if nothing does, delete it too and say so.

Scope label mapping for `doctor_scan_facts` — from `scan.scope_kind` (E-5), the same
strings the start page uses: `whole_library` → `DOCTOR_SCOPE_WHOLE_LIBRARY`,
`current_view` → `DOCTOR_SCOPE_CURRENT_VIEW`, `selection` → `DOCTOR_SCOPE_SELECTION`.
Read the actual `kind()` strings from `library_doctor/scope.rs`; do not guess them.

---

## E. Counts come from one scope (brief §4)

Root cause, found by reading `crates/reprise-core/src/library/library_doctor/scan.rs:150-165`:

```rust
let (mut track_proposals, mut track_groups) =
    local_rules::proposals_for(std::slice::from_ref(&read_track));   // ONE track
...
preview_summary.merge(super::presentation::partial_scan_summary(
    &track_proposals, track_groups.len(), 1, 0));                    // and merge() ADDS
```

`DoctorScanSummary::merge` (`presentation.rs:27-40`) sums `unresolved_groups`. A
spelling conflict needs at least two differing spellings across tracks, so a per-track
group count is not a smaller version of the real number — it is a different number, and
summing it over 27 tracks is how "14 spelling conflicts · 27 checked" happens. The
completed scan recomputes over `&read_tracks` as a whole (`scan.rs:187`) and is right.

Fix:

1. **E-2:** publish `0` for groups in the live preview —
   `partial_scan_summary(&track_proposals, 0, 1, 0)`. Add a core test asserting the
   live summary's `unresolved_groups` never exceeds the completed scan's.
2. Every number the finished page shows comes from `scan_summary(&scan, …)` over the
   persisted scan, which only contains in-scope tracks — so scope consistency follows.
   Do not read counts from `count_pending_doctor_findings` or any library-wide query
   for this page.
3. Scope and remote facts from the scan itself (E-5).
4. The running page shows only the two forecast counters and the progress fraction —
   no conflicts number, no `checked · skipped`.

---

## F. One job card in the sidebar (brief §7)

Reported: two cards, `Che… 0% Cancel Scan` and `Checking tra… 1% Cancel`, both
truncated past legibility.

**Root-cause it before you change it.** Static reading says two *doctor* cards are
impossible: `append_doctor_card` has exactly one call site (`library_doctor/mod.rs:139`),
`LibraryDoctorCoordinator::new` has exactly one call site
(`window/window_runtime_wiring.rs:143`), and `SidebarActivitySlot::set_doctor_card` →
`replace_child` removes any previous card from the box. The second card is therefore
most likely a *different* job's card in the same slot — the library-scan card
(`scan/scan_progress.rs`, whose cancel button reads `CANCEL_SCAN` = "Cancel Scan", which
matches the first card) or the relink card (`issues/missing_progress.rs`). Both live in
`progress_root` and, under U-1, are allowed to be there.

So:

1. **Prove which widgets are in `progress_root`.** Add a display test that drives a
   doctor scan (or `DoctorProgressCard::show` plus the sidebar wiring) and asserts the
   set of visible children of `SidebarActivitySlot::progress_widget()` — exactly one
   doctor card, and it survives repeated scan/finish cycles without accumulating.
   Report in the result summary what the second card actually was.
2. If a genuine duplicate exists, remove the duplicate subscriber. **If the second card
   belongs to another job, do not remove it** — a running library scan and a running
   doctor scan are two jobs and may show two cards. Say so in the summary.
3. **Fix the truncation either way** (`library_doctor/progress_card.rs:64-92`). Today
   the title is `hexpand(true) + ellipsize(End)` and loses its allocation to `percent`
   and a full-width `Cancel` button, down to three characters. The brief: *give the
   label room; if it does not fit, ellipsize the detail line instead and keep the label
   whole.*
   - raise the title's minimum: `title.set_width_chars(16)` — enough for
     "Checking tracks…" in a 240px sidebar (NPP-1)
   - make the trailing controls cheap: `percent` and `cancel` get the app's small text
     level (`caption`), `cancel` keeps `.flat` (mockup: 12px accent text, not a chunky
     button)
   - the detail line keeps `ellipsize(End)` and absorbs any remaining shortfall
   - **measure it**: a display test that renders the card at the sidebar's real width
     and asserts `title.is_ellipsized() == false` for `DOCTOR_SCANNING`. Do the same for
     the library-scan card's title if you touch it (`scan/scan_progress.rs`). Per
     `docs/` house rule, measure with `pick()`/allocation, never by assuming.

The card's contract stays: label, percentage, `Cancel`, progress bar, one detail line,
plus the existing click/Enter/Space activation and its a11y properties (ACC-8).

---

## G. `ISSUES` is missing from the sidebar (brief §8)

Root cause: `sidebar/sidebar_issues_section.rs` builds a **`GtkStack`** with an
`issues` page and an `activity` page, and `sidebar_activity_slot.rs:145-166` switches to
`activity` whenever any progress card is visible. Mid-scan the whole `ISSUES` block —
heading, `Missing files`, import errors — is therefore not hidden by accident but by
FB-8's letter.

### G-1. Amend FB-8 (U-1)

`docs/ux-rules.md` around line 1196, FB-8 `[active]`. Rewrite the replacement clause:
the `ISSUES` block stays visible while progress cards are shown; the cards sit **below**
it, pinned to the bottom of the sidebar (mockup frame 3). Keep the id, keep every other
part of the card contract (spinner, title, percent, cancel, 3px bar, detail, click
target), and keep FB-2a's "[replaced by FB-8]" pointer intact. Note the reversal and its
date in the rule text so the next reader does not think it drifted.

### G-2. Implementation

- `build_issues_section` returns a `gtk4::Box` (vertical): the issues block
  (`problem_header()` + `issues_listbox`, keeping the existing
  `bind_property("visible")` link so the heading follows the listbox) and then
  `activity_slot.progress_widget()`.
- Delete `IssuesSurface`, `issues_surface_for_progress`, `show_issues_surface`,
  `attach_issues_stack`, and the stack-page switching in
  `sidebar_activity_slot::show_surface_for_progress`.
- **Keep** `sync_revealer_visibility` and the `Revealer` visibility tracking: an
  unrevealed revealer must still drop out of layout, or the sidebar reserves height for
  invisible cards.
- Keep the bottom pinning: `bottom_region_placement()`'s `valign: End` /
  `vexpand: false` behaviour has to survive the loss of the stack —
  `progress_root`'s own `progress_spacer` already pushes the cards down; verify the
  cards still sit at the very bottom with the ISSUES block above them, and that an
  empty ISSUES block leaves no gap.
- Update `sidebar/sidebar_layout_tests.rs`, `sidebar/sidebar_tests.rs`, the
  `sidebar_activity_slot` test, and any `sidebar_issues_section` assertion that encodes
  the swap. Re-run `scripts/check-ux-traceability.sh` — FB-8's traceability marker must
  still resolve.

### G-3. The Doctor's `ISSUES` entry

Already implemented: `queries::count_pending_doctor_findings` →
`pending_doctor_count` → `doctor_issue_visible()` → `NavIcon::LibraryDoctor`
(`sidebar_rebuild.rs:40,74-78,346`, `sidebar_presentation.rs:29,49,96`). Verify
end-to-end and fix whatever does not hold:

- appears only when a **completed** scan has unreviewed findings
- carries the count (mockup 3b: trailing count, accent) and gets the
  `attention` treatment `issue_row_presentation` already grants it
- clicking it opens the Doctor **review** page (acceptance 6) — check where the row's
  activation lands today; if it opens the summary instead, route it to the review
- disappears once the findings are applied or dismissed
  (`set_reviewed_scan` → `sidebar.refresh(...)`)
- sits under `ISSUES`, next to `Missing files`, below `SMART`

---

## H. Where this plan overrides round 1

`docs/plans/library-doctor-redesign.md` §4 said the running Doctor page shows
`DOCTOR_RESULTS_SO_FAR` with the same two summary blocks, actions disabled, and
`DOCTOR_CONTROLS_LOCKED` as the reason. **That is exactly the screen being rejected.**
§C-2 replaces it. Do not "restore" the round-1 behaviour, and update
`docs/ux-rules.md` § Y (Library Doctor / Tag Cleanup) so the DOC-* rules describe the
split running/finished pages, the three cards, the zero rule and the past-tense applied
block. Keep every DOC-* id alive; retarget rather than delete.

---

## I. Verification. Green tests are not the deliverable.

Run, from the worktree:

1. `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`
2. `cargo test -p reprise-core` and `cargo test -p reprise-gnome`
3. display tests: single-threaded under Xvfb, `--test-threads=1`, `GDK_BACKEND=x11`
   with `WAYLAND_DISPLAY` unset — see `TESTING.md` / `scripts/check-display-tests.sh`.
   Several display tests are **already red on `origin/dev`**; before reporting a
   failure, check the same test on the base commit and say which failures pre-date this
   branch.
4. `scripts/check-ux-traceability.sh`, `scripts/check-frontend-thinness.sh`,
   `scripts/check-accessibility-semantics.sh`, `scripts/check-architecture.sh`,
   `scripts/check-motion-tokens.sh`, `scripts/check-input-parity.sh`
5. New tests this plan owes:
   - the zero rule: no detail line, block or card is emitted at `0` (model-level)
   - all-zero → empty state
   - running state renders no `Scan again`/`Review`/`Undo`/`checked · skipped`
   - `Undo` sensitive exactly in `Summary { quiet: Applied }` with changes > 0
   - failed quiet write → no applied card (E-4)
   - live summary's `unresolved_groups` ≤ the completed scan's (core, E-2)
   - singular forms: `1 change needs your eye`, `1 fix already applied`,
     `1 spelling conflict, no clear winner` (acceptance 7)
   - left alignment and ≤700px column, measured (C-5)
   - the doctor card's title is not ellipsized at sidebar width (F-3)
   - exactly one doctor card in `progress_root` across repeated scans (F-1)
   - `ISSUES` block and a visible progress card coexist (G-2)
6. Do not claim a gate passed without pasting its output.

Acceptance criteria, from the brief — every one has to be demonstrated, not asserted:

1. safe fixes are on disk before the summary appears; `Undo` works
2. no line, block or card anywhere reads `0`
3. mid-scan: progress heading, progress bar, `Cancel`, nothing else
4. after completion: three left-aligned cards, actions inline on the right, conflicts
   card visibly quietest
5. one job card in the sidebar, label readable
6. `ISSUES` holds a `Library Doctor` entry with the count; clicking it opens the review
7. a one-change scan reads `1 change needs your eye`
8. a clean library shows the empty state, not three empty blocks

## J. Scope fence

Do not touch: the scan engine's scanning logic, `local_rules.rs`, the
MusicBrainz/AcoustID clients, the remote cache, `db_tag_write_jobs.rs`, the review page
and its rows, the start page, the post-apply page, `reprise-mcp`. The one core edit
allowed is E-2 in `scan.rs` plus its test. Commit in focused steps; do not touch files
outside this worktree.
