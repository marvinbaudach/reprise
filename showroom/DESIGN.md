# A calmer portfolio and a familiar film player

Approved in the conversation on 2026-09-09. This supersedes the older showroom
entrance choreography and chapter order for this local preview.

## Design

Lead with the running product and Marvin Baudach's responsibility for its design,
architecture and quality. Keep the current dark palette, native screenshots and
source-backed evidence. Show three concise strengths and the film before the
technical case studies. Move the timeline, code census, detailed quality incident,
headless contract and source methodology into optional disclosures. Retain the
full measurements and their trade-off together. Use descriptive navigation.

Text, figures and screenshots stay still and readable. Remove scroll entrances,
count-ups, pointer parallax and background drift. Keep one interactive spectral
example in the design details, honouring reduced motion.

The existing 58-second film uses a central play button, duration badge, seek bar,
elapsed/total time, mute/volume, fullscreen and replay. Playback only follows an
explicit action, never loops, and rests on the end card. Controls reveal on
pointer activity, touch or keyboard focus; paused controls stay visible. Native
controls are the fallback before enhancement. Preserve the existing source ladder
and no-download-before-play behaviour. Explain loading and playback errors.

## Delivery scope

This change implements the page and player, verifies their public browser
behaviour and existing showroom contracts, and serves a local preview. The local
commit is titled `The showcase puts the product first and lets the film seek`.
No push or deployment is authorised by this request.

Test boundaries: the delivered page and actual video element, as approved by the
requested local result. Verify explicit playback, seeking while paused/playing,
volume, keyboard focus, fullscreen, end/replay, small screens, reduced motion and
content visibility. Browser tests use an isolated Chromium profile and local media.

## Progress

- Read the workflow, active ownership, existing design and current dev history.
- Base: c19ae351c6; isolated branch `feature/showcase-calm-player`.
- No repository lock or coordination board exists in this checkout. Scope is
  `showroom/` only; no other strand's files are edited.
- Existing handover notes describe old film withdrawals; current dev mounts the
  film and includes the 58-second encodes. Those current commits are authoritative.
- Implemented the product-first introduction, three sourced highlights, early
  film, descriptive navigation and optional technical details. Author and contact
  remain visible outside the disclosures.
- Removed entrance, counter, background and diagram animations. The optional
  spectral example remains interactive. The code ratio now renders directly from
  its current data without depending on the removed choreography.
- Implemented the enhanced player and its native-control fallback without adding
  dependencies or changing the film assets.
- Test-first failures covered the central play action, reading order, static
  metrics and diagrams, exhausted video sources and the final displayed time.
  Each regression now passes.
- All 101 showroom tests, TypeScript checking, source lint and the production
  build pass. The browser suite passes against the production preview in an
  isolated Chromium profile, including playback, paused seeking, volume,
  fullscreen, keyboard access, idle controls, end/replay, failed-download
  recovery, touch input and reduced motion. Checked widths: 320, 390, 768 and
  1440 pixels. Screenshots were inspected for the desktop and mobile page,
  player, architecture and performance sections.
- Adversarial review caught three regressions and fixed them: an unfilled code
  ratio after removing the counter animation, indefinitely pending playback when
  every source fails, and the held end card displaying 0:57 instead of 0:58.
- Repository checks passed: project/showroom/Android source quality, shell
  and script self-tests, worktree maintenance tests, gettext, architecture,
  GStreamer availability, accessibility/input/listen-report parity, frontend
  thinness, UX traceability, AppStream, release metadata, Flatpak, GNOME idioms,
  AI hygiene, motion tokens, Rust formatting/lint/documentation and dependency
  audit. The audit reports only the already accepted `paste` advisory. The
  Android gate required the installed SDK path and freshly generated UniFFI
  bindings in this isolated checkout; neither required a tracked Android change.
- `cargo test --workspace` passed: 5,707 tests passed, none failed, and 903 were
  ignored by that suite. All 590 rule-owned display tests then passed separately
  in isolated XDG/D-Bus/Xvfb environments, using four bounded workers. The gate
  excludes its three explicitly declared measurement tools.
- The local preview stage is complete. All edited code files are under 800 lines,
  the diff has no whitespace errors, and no repository lock needs releasing.

## Local review

### Mobile follow-up

The focused mobile review found two layout defects that the earlier overflow
checks did not detect: the performance-table caption retained table-caption
layout inside a block table and collapsed to a narrow column; the landscape
video exceeded the usable height below the fixed header. The caption now uses
the reading width with larger type. Short landscape viewports cap the inline
video height while fullscreen keeps the complete viewport.

Both defects were reproduced by failing browser assertions before the CSS fixes.
The permanent browser suite now also checks touch pause, finger-drag seeking
without resuming playback, and landscape fullscreen height. Additional review
sizes are 320×568, 360×800, 390×844, 430×932, 844×390 and 768×1024. Expanded
technical evidence is included in the mobile review.

The follow-up passed project/showroom source quality, all 101 showroom tests,
the browser suite against the production build, and the additional mobile
walkthrough with inspected screenshots. Rust formatting, workspace Clippy,
all 5,707 workspace tests and the dependency audit were rerun and passed. No
native, Android, packaging or workflow files changed in this follow-up. All
edited code remains under 800 lines. Its local commit is titled
`The showcase stays readable on phones in both orientations`.

### Preview and verification

Run `npm run dev -- --host 127.0.0.1 --port 4175 --strictPort` from `showroom/`,
then open `http://localhost:4175/reprise/`. The current session also serves the
production build at `http://127.0.0.1:4176/reprise/`.

Run `npm run test:browser` with Chromium installed at `/usr/bin/chromium`.
`CHROMIUM` overrides the executable and `SHOWROOM_URL` overrides the preview URL.
The harness creates an isolated browser profile and removes it when finished.

The existing GitHub contact route and supplied film/poster are retained. No
unverified claims, contact details or new media have been introduced. Firefox,
Safari and physical phones remain manual cross-browser checks; the automated
touch and narrow-screen checks use Chromium emulation.

The repository gates were invoked individually on this local branch. A clean
integration-wrapper run against the latest `dev` is deferred until integration;
`dev` advanced independently while this preview was being verified. This record
does not claim a hosted CI result or permission to push, merge or deploy.
