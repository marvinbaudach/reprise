---
slug: device-page-unreachable-behind-a-pushed-page
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-08
---
# The device page arrives, and nobody sees it

Reported 2026-08-08 against the installed build: clicking the phone in the
sidebar does not open the device page. The window title changes to the device
name; the content area keeps showing the Library Doctor.

## Diagnosis — done, do not re-derive it

The content pane is two layers:

- `content_nav`, an `adw::NavigationView`, whose root page is tagged
  `now_playing_wiring::LIBRARY_CONTENT_TAG`;
- `content_stack`, a `gtk4::Stack` inside that root page, holding `library`,
  `device-sync` and the other named pages.

`device_sync_page::open` (`device_sync_page.rs:502-524`) adds the device page to
`content_stack` under `"device-sync"` and sets `window_title`. It never touches
`content_nav` — it does not receive it.

The Library Doctor is not a stack child. `LibraryDoctorCoordinator::open_root_page`
(`library_doctor/mod.rs:225-231`) **pushes** a page onto `content_nav`. While it
is pushed it covers the stack entirely.

So opening a device swaps a child of a covered stack and renames the window.
The pushed page stays on screen. Selecting an ordinary source does not have this
problem because `window_navigation::show_content_callback` pops back to the root
page first (`window_navigation.rs:88-95`); `open_device` (`window.rs:457-471`)
has no such step.

Two consequences worth stating:

- Neither the running sync nor the running track check is involved. They were
  present when it was reported and are a coincidence; the reproduction needs
  only a pushed page.
- The Library Doctor is not special. Any pushed page — the metadata navigator,
  the folder browser — hides the device page the same way.

Ruled out by evidence, do not revisit: the device page failing to install in the
stack (`show_page`'s "content stack target is not installed" warning does not
appear in the journal, and `open` returns `true`).

## Task 1 — A seam that can be tested, then the failing test

The sequence lives in a closure in `window.rs:457-471`, which nothing can call.
Extract it — behaviour unchanged for now — into a named function that takes the
navigation view as well, e.g. `window_navigation::open_device_place(content_nav,
content_stack, window_title, device_id, runtime, split_view) -> bool`.

**Write the test before adding the pop, and watch it go red.** A display test
that builds the real two-layer arrangement — a `NavigationView` whose root page
is tagged `LIBRARY_CONTENT_TAG` and contains a `Stack` — pushes a second page on
top, then opens a device, and asserts the **user-visible symptom**:

- `content_nav.visible_page()` is the root page, not the pushed one;
- `content_stack.visible_child_name() == Some("device-sync")`.

Assert the visible page, never "pop was called" — the second passes with the bug
still on screen.

## Task 2 — The fix

Pop `content_nav` back to `LIBRARY_CONTENT_TAG` before showing the device page,
the same way `show_content_callback` already does. Reuse that logic rather than
writing a second copy — a predicate duplicated in this codebase has produced a
user-visible bug more than once, and "which of the two ways of showing content
pops?" is exactly that shape.

Order matters: pop first, then switch the stack, so nothing renders the old
stack child on top of a transition.

Do not make `device_sync_page::open` take the navigation view. It is the page
builder; knowing about the shell's navigation is the caller's job.

## Task 3 — The same hole, elsewhere

`open_device` was not the only caller that switches `content_stack` directly.
Find every caller of `content_stack::show_page` and of
`content_stack.set_visible_child*`, and check each one against the same
question: *if a page is pushed on `content_nav`, does this switch reach the
user?*

Report what you find. Fix the ones that are genuinely reachable from the UI;
for any that are not, say why rather than changing them. If the answer is "most
of them", the right fix is one funnel that both pops and switches, and everything
routes through it — propose that rather than sprinkling pops.

## Out of scope

- The Library Doctor's own behaviour and its three cards.
- The device page's contents.
- Sidebar collapse behaviour beyond what the extraction already carries.

## Verification

- The new display test is red before Task 2 and green after. Paste both.
- `cargo test -p reprise-gnome -p reprise-core -p reprise-platform-linux`.
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`.
- Display-backed tests need `GDK_BACKEND=x11` under Xvfb with `WAYLAND_DISPLAY`
  unset, one process at a time.
- `check-architecture.sh` is already red on `origin/dev`
  (`crates/reprise-core/src/library/tag_edit_write.rs`, 824 lines).
- End to end, by hand or headless: open the Library Doctor, then click the phone
  in the sidebar, and see the device page. That is the report; the unit test is
  the guard, not the proof.
