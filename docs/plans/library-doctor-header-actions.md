---
slug: library-doctor-header-actions
worktree: /home/marvin/Projects/reprise/.worktrees/doctor-header-actions
branch: feat/doctor-header-actions
phase: refactored
codex_session:
created: 2026-08-11
---
# View actions belong to the view, not to the title bar

Opening the Library Doctor review makes two text buttons — **All** and
**None** — appear in the window's title bar, next to buttons that never
change meaning (the ⋮ menu, the updates trigger, the info-panel toggle).
Nothing marks them as belonging to the page below, they appear and vanish
with navigation, and the label `All` collides with the `All` segment of the
review's own category filter a few pixels lower.

They move into the view. The column header of the review table grows a
**master checkbox** with three states, sitting directly above the column of
per-row checkboxes it controls. The title bar keeps only what is always
true.

## Evidence

The mechanism exists exactly once in the codebase. `DoctorChrome` owns a
`review_actions` box that is packed into the shared library header
(`ui/window/library_chrome.rs:81-83`) and shown only while the review page
is the visible navigation page (`library_chrome.rs:133-140`). The review
page fills it through `set_review_actions` (`library_chrome.rs:112-118`),
called from `ui/library_doctor/mod.rs:655` with the page's own `presets`
box (`review_page.rs:475-477`, exposed as `chrome_actions()` at
`review_page.rs:526-528`). No other view uses it, so the whole path can go.

The replacement is almost free because the pieces are already there:

- The column header already reserves the checkbox column. Its first child
  is an **empty placeholder label** registered in the `selection` size group
  (`review_header.rs:72-94`), which is what keeps the header columns aligned
  with the rows. The master checkbox takes that slot.
- The tri-state calculation already exists for album group headers
  (`review_header.rs:149-158`) — selected count versus selectable count,
  `set_active` when full, `set_inconsistent` when partial.
- `session.all()` and `session.none()` already respect the active category
  filter and already skip rows that are not `Ready`
  (`reprise-core/src/library/library_doctor/review.rs:478-513`). The agreed
  semantics — *the checkbox covers exactly what the filter shows* — is what
  the core already does. Nothing in `reprise-core` needs to change.

## Two traps this plan has to clear

**The narrow layout hides the column header.** A breakpoint sets
`header.root` invisible below `WIDE_BREAKPOINT` (`review_page.rs:491`). If
the master checkbox simply lives in that box, then in a narrow window there
is no way to select or clear everything at all — today's title-bar buttons
are always present, so that would be a regression. The header therefore
splits: the checkbox stays visible in every layout, only the column *labels*
are hidden.

**A long-lived checkbox re-enters its own handler.** The album checkboxes
are rebuilt on every bind, so writing their state back is harmless. The
master checkbox lives for the lifetime of the page, so `set_active` during a
refresh would fire `toggled` and re-run `all()`/`none()`. Its handler id must
be stored and blocked while the state is written back.

## Files

Starting points, not a fence — follow the compiler and the call sites if
something else needs touching.

- `crates/reprise-gnome/src/ui/library_doctor/review_header.rs` — checkbox,
  state function, header split
- `crates/reprise-gnome/src/ui/library_doctor/review_page.rs` — wiring,
  refresh, breakpoint, removal of the preset buttons
- `crates/reprise-gnome/src/ui/window/library_chrome.rs` — removal of the
  `review_actions` path
- `crates/reprise-gnome/src/ui/library_doctor/mod.rs` — the call at :655
- `crates/reprise-gnome/src/ui/library_doctor/tests.rs`,
  `crates/reprise-gnome/src/ui/window/library_chrome_tests.rs` — structure
  assertions that name the old mechanism verbatim
- `crates/reprise-gnome/src/ui/strings_library_doctor.rs` — strings
- the Library Doctor stylesheet that defines `doctor-review-header-action`
  and `doctor-album-check`
- `docs/ux-rules.md` — DOC-3a amendment, new DOC-3c, new STYLE-12

---

## Task 1: the tri-state calculation, as a tested function

`album_change_count` (`review_header.rs:233-239`) with its unit test
(`review_header.rs:250-257`) is the model to copy: a pure function, tested
without a display.

- [ ] **Write the failing test** in the existing `mod tests` of
      `review_header.rs`:

```rust
#[test]
fn doc_3c_the_master_check_mirrors_the_visible_selection() {
    use super::MasterCheckState;
    assert_eq!(
        super::master_check_state(0, 0),
        MasterCheckState { active: false, inconsistent: false, sensitive: false }
    );
    assert_eq!(
        super::master_check_state(0, 4),
        MasterCheckState { active: false, inconsistent: false, sensitive: true }
    );
    assert_eq!(
        super::master_check_state(2, 4),
        MasterCheckState { active: false, inconsistent: true, sensitive: true }
    );
    assert_eq!(
        super::master_check_state(4, 4),
        MasterCheckState { active: true, inconsistent: false, sensitive: true }
    );
}
```

- [ ] **Run it and watch it fail** — `cargo test -p reprise-gnome
      doc_3c_the_master_check` reports that `master_check_state` does not
      exist.

- [ ] **Implement it** in `review_header.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MasterCheckState {
    pub(super) active: bool,
    pub(super) inconsistent: bool,
    pub(super) sensitive: bool,
}

pub(super) fn master_check_state(selected: usize, selectable: usize) -> MasterCheckState {
    MasterCheckState {
        active: selectable > 0 && selected == selectable,
        inconsistent: selected > 0 && selected < selectable,
        sensitive: selectable > 0,
    }
}
```

- [ ] **Use it for the album headers too.** Replace the hand-rolled pair in
      `bind_album_header` (`review_header.rs:157-158`) so both levels of the
      table cannot drift apart:

```rust
    let check_state = master_check_state(selected, total);
    checkbox.set_active(check_state.active);
    checkbox.set_inconsistent(check_state.inconsistent);
```

      `total` is already computed at `review_header.rs:153`. Album headers
      keep their current sensitivity — do **not** apply `check_state.sensitive`
      there; that field exists for the master checkbox only.

- [ ] **Run the tests** — `cargo test -p reprise-gnome review_header` passes,
      including `doc_9b_a_fully_deselected_album_says_none_selected`.

- [ ] **Commit** — `feat(doctor): shared tri-state calculation for review checkboxes`

---

## Task 2: the master checkbox in the column header

- [ ] **Add the two strings** to `strings_library_doctor.rs`, next to
      `DOCTOR_ALL` / `DOCTOR_NONE`:

```rust
pub const DOCTOR_SELECT_ALL: &str = N_!("Select all");
pub const DOCTOR_SELECT_ALL_VISIBLE: &str = N_!("Select all visible changes");
```

      Leave `DOCTOR_ALL` and `DOCTOR_NONE` in place for now; Task 5 removes
      them once nothing references them.

- [ ] **Split the header** in `review_header.rs`. The struct grows the
      widgets the page needs to reach:

```rust
pub(super) struct ReviewHeader {
    pub(super) root: gtk4::Box,
    pub(super) labels: gtk4::Box,
    pub(super) select_all: gtk4::CheckButton,
    pub(super) select_all_label: gtk4::Label,
    pub(super) groups: ReviewColumnGroups,
}
```

      In `new()`, the checkbox replaces the empty placeholder that used to
      occupy the `selection` size group, and the column labels move into
      their own box:

```rust
    pub(super) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.add_css_class("dim-label");
        let groups = ReviewColumnGroups::new();

        let select_all = gtk4::CheckButton::new();
        select_all.set_size_request(16, 16);
        select_all.add_css_class("doctor-album-check");
        select_all.add_css_class("doctor-review-select-all");
        select_all.set_tooltip_text(Some(&strings::text(strings::DOCTOR_SELECT_ALL_VISIBLE)));
        select_all.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::DOCTOR_SELECT_ALL_VISIBLE,
        ))]);
        // a11y-semantics: role=checkbox name=select-all-visible state=selection action=toggle
        select_all.set_focusable(true);
        groups.selection.add_widget(&select_all);
        root.append(&select_all);

        let select_all_label = gtk4::Label::builder()
            .label(strings::text(strings::DOCTOR_SELECT_ALL))
            .xalign(0.0)
            .visible(false)
            .css_classes(["caption"])
            .build();
        root.append(&select_all_label);

        let labels = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        labels.set_hexpand(true);
        for (group, text) in [
            (&groups.track, strings::DOCTOR_TRACK),
            (&groups.field, strings::DOCTOR_FIELD),
            (&groups.current, strings::DOCTOR_CURRENT),
            (&groups.arrow, ""),
            (&groups.proposed, strings::DOCTOR_PROPOSED),
            (&groups.source, strings::DOCTOR_SOURCE),
            (&groups.edit, ""),
        ] {
            let label = gtk4::Label::builder()
                .label(if text.is_empty() {
                    String::new()
                } else {
                    strings::text(text)
                })
                .xalign(0.0)
                .hexpand(!text.is_empty())
                .css_classes(["caption"])
                .build();
            group.add_widget(&label);
            labels.append(&label);
        }
        root.append(&labels);

        Self {
            root,
            labels,
            select_all,
            select_all_label,
            groups,
        }
    }
```

      The `selection` entry disappears from the loop because the checkbox
      now holds that size group. Everything else about the columns is
      unchanged — the alignment between header and rows must look exactly as
      it does today.

- [ ] **Retarget the breakpoint** in `review_page.rs:491`, so a narrow
      window loses the column titles but keeps the checkbox, now labelled:

```rust
        breakpoint.add_setter(&header.labels, "visible", Some(&false.to_value()));
        breakpoint.add_setter(&header.select_all_label, "visible", Some(&true.to_value()));
```

- [ ] **Build** — `cargo build -p reprise-gnome`. The checkbox exists but
      does nothing yet; that is Task 3.

- [ ] **Commit** — `feat(doctor): a master checkbox in the review column header`

---

## Task 3: wire it to the session

- [ ] **Give `ReviewState` the widget and its handler.** Add to the struct
      (`review_page.rs`, near `apply` and `change_summary`):

```rust
    select_all: gtk4::CheckButton,
    select_all_handler: RefCell<Option<glib::SignalHandlerId>>,
```

      and populate them where the state is built (`review_page.rs:415-432`):

```rust
            select_all: header.select_all.clone(),
            select_all_handler: RefCell::new(None),
```

- [ ] **Connect the toggle** where the `all` / `none` buttons are wired
      today (`review_page.rs:457-477`), replacing them entirely:

```rust
        {
            let state = state.clone();
            let handler = header.select_all.connect_toggled(move |button| {
                if button.is_active() {
                    state.session.borrow_mut().all();
                } else {
                    state.session.borrow_mut().none();
                }
                state.refresh();
            });
            *state.select_all_handler.borrow_mut() = Some(handler);
        }
```

      A click while the box is `inconsistent` activates it, which selects
      everything the filter shows — the agreed behaviour. Delete the `all`
      and `none` buttons, the `presets` box, and the `all` / `none` /
      `chrome_actions` fields of `LibraryDoctorReviewPage`
      (`review_page.rs:340-342`, `:510-517`) together with the
      `chrome_actions()` accessor (`:526-528`).

- [ ] **Write the state back on refresh.** Add the method next to
      `refresh_filter_summary` (`review_page.rs:91-94`), which already walks
      the same visible rows:

```rust
    fn refresh_master_check(&self) {
        let rows = self.visible_rows();
        let selected = rows
            .iter()
            .map(|row| row.selected_change_count)
            .sum::<usize>();
        let selectable = rows
            .iter()
            .map(|row| row.selectable_row_ids.len())
            .sum::<usize>();
        let check = master_check_state(selected, selectable);
        let handler = self.select_all_handler.borrow();
        if let Some(handler) = handler.as_ref() {
            self.select_all.block_signal(handler);
        }
        self.select_all.set_active(check.active);
        self.select_all.set_inconsistent(check.inconsistent);
        self.select_all.set_sensitive(check.sensitive);
        if let Some(handler) = handler.as_ref() {
            self.select_all.unblock_signal(handler);
        }
    }
```

      and call it from `refresh()` on the line after
      `self.refresh_filter_summary();` (`review_page.rs:82`). Import
      `master_check_state` from `review_header`.

- [ ] **Verify by hand in the running app.** Build and drive it headless —
      never put the window on the user's desktop. Check, in this order:
      the box starts checked (everything is preselected); unchecking a
      single row turns it mixed; clicking it once selects everything again;
      clicking it again clears everything and `Apply` goes insensitive;
      switching the filter to `Casing` and clearing there leaves `Year`
      rows selected — visible by switching back to `All`.

- [ ] **Commit** — `feat(doctor): the master checkbox drives the visible selection`

---

## Task 4: remove the title-bar mechanism

- [ ] **Delete the chrome path.** In `library_chrome.rs`: the
      `review_actions` box and its `pack_end` (`:81-83`), the field on
      `DoctorChrome` and its initialiser (`:93`), the `set_review_actions`
      method (`:112-118`), and the `review_visible` block that only served
      it (`:133-140`). Remove the `DoctorChrome` struct field where it is
      declared. In `library_doctor/mod.rs`, delete the call at `:655`.

- [ ] **Fix the structure tests** in
      `library_doctor/tests.rs:99-115`. Four assertions name the removed
      mechanism (`:107`, `:110`, `:111`, `:114`). Replace them so the test
      now guards the *absence* of the old path and the presence of the new
      one, keeping the surrounding assertions untouched:

```rust
    let header = include_str!("review_header.rs");

    assert!(!coordinator.contains("set_review_actions"));
    assert!(!review.contains("chrome_actions"));
    assert!(!chrome.contains("review_actions"));
    assert!(header.contains("groups.selection.add_widget(&select_all)"));
    assert!(review.contains("header.select_all.connect_toggled"));
```

- [ ] **Fix `library_chrome_tests.rs`.** Delete the `review_actions` box it
      builds for the fixture and the `set_review_actions` call at `:100`,
      plus the `all` / `none` buttons that fed it and any variable that goes
      unused as a result. The rest of the test — title widget, top bars,
      start/end title buttons — stays exactly as it is; it is the proof that
      removing this did not disturb the header.

- [ ] **Run** — `cargo test -p reprise-gnome` and
      `cargo clippy --workspace --all-targets -- -D warnings`. Clippy is the
      gate that catches leftovers here: unused imports, unused fields, a
      `presets` box nobody appends to.

- [ ] **Commit** — `refactor(doctor): the title bar no longer carries view actions`

---

## Task 5: strings, styles, rules

- [ ] **Remove the dead strings.** `DOCTOR_ALL` and `DOCTOR_NONE` in
      `strings_library_doctor.rs` now have no callers — confirm with
      `rg 'DOCTOR_ALL|DOCTOR_NONE' crates/` and delete them. Update the
      translation catalogues the same way the repo does for any removed
      `N_!` string, and add the two new ones from Task 2.

- [ ] **Remove the dead CSS.** `doctor-review-header-action` has no widget
      left. Delete its rules. Give `doctor-review-select-all` whatever
      `doctor-album-check` already provides for size and accent so the
      master checkbox reads as the same control family as the checkboxes
      below it — if `doctor-album-check` carries the whole treatment, adding
      the new class to the existing selector is enough.

- [ ] **Amend DOC-3a** (`docs/ux-rules.md:3930-3942`). It currently names
      the buttons: *„All" selects every ready row, „None" clears
      everything*. Rewrite that sentence for the master checkbox and add a
      dated amendment line in the style of the existing one:

```
  reviewable starts selected.** Every concrete track/field change has its own
  selection and arrives preselected. The master checkbox in the column header
  selects every ready row when it is on and clears every row when it is off;
  neither touches a stale or conflicting row. …
  *Amended 2026-08-11: the `All`/`None` title-bar buttons became one
  tri-state master checkbox in the review's own column header — see DOC-3c
  and STYLE-12.*
```

- [ ] **Add DOC-3c** to section Y, after DOC-3b, in the section's voice:

```
- **DOC-3c** [active] [gtk] — **The master checkbox says what it covers.**
  The review's column header carries one checkbox above the row checkboxes.
  It is checked when every selectable row the current category filter shows
  is selected, mixed when only some are, and unchecked when none are;
  it is insensitive when the filter shows nothing selectable. Toggling it
  affects exactly the rows that filter shows and never touches a stale or
  conflicting row. It stays reachable in the narrow layout, where the column
  titles are hidden and it is labelled instead.
```

- [ ] **Add STYLE-12** to section S — the general rule, so the next view
      does not repeat this:

```
- **STYLE-12** [active] [gtk] — **The title bar only carries what is always
  true.** The window header holds actions whose meaning does not change with
  the visible page: the primary menu, search, the panel toggles, global
  status. Anything that belongs to one page — selection presets, bulk
  actions, page-local filters — lives inside that page, near what it acts
  on. A control that appears and disappears with navigation is
  indistinguishable from a permanent one while it is there, and its label
  competes with the page's own vocabulary (the case that prompted this: the
  Library Doctor's `All` preset sat in the title bar directly above the
  review's own `All` filter segment). Views do not push widgets into the
  shared header; if a view seems to need it, the action is in the wrong
  place.
```

- [ ] **Run the rule gates** — `./scripts/check-ux-traceability.sh` and
      `./scripts/check-accessibility-semantics.sh`.

- [ ] **Commit** — `docs: view actions stay in the view (STYLE-12, DOC-3c)`

---

## Verification

- `cargo test -p reprise-gnome` and `cargo test -p reprise-core` — the core
  is untouched, so a change there means something went wrong.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `./scripts/check-ux-traceability.sh`,
  `./scripts/check-accessibility-semantics.sh`.
- Display-backed tests need `GDK_BACKEND=x11` under Xvfb, **one process at a
  time** — the display suite is flaky when run as a herd, and several gates
  are already red on `origin/dev`. Compare every failure against the merge
  base before blaming this change.
- **Visual proof, not just green tests.** Two screenshots of the review
  page, wide and below the breakpoint. Wide: the checkbox sits exactly above
  the row checkboxes, the title bar's right side shows only the permanent
  buttons. Narrow: the column titles are gone, the labelled checkbox is
  still there. Also capture the mixed state — a partially selected table.

## Out of scope

The other buttons on the right of the title bar (menu, updates trigger,
sync spinner, panel toggle) are global and stay. The category filter, the
footer, the apply flow, and anything in `reprise-core` stay as they are.
