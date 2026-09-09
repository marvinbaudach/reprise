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

Text, figures and screenshot frames stay still and readable. Remove scroll
entrances, count-ups, pointer parallax and background drift. The phone keeps its
recorded visualization moving inside its display on desktop and mobile. Keep one
interactive spectral example in the design details, honouring reduced motion.

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

The phone visualization was restored after the mobile review, as explicitly
requested. It reuses the existing recorded track, renderer and playback policy.
Browser checks observe consecutive changing canvas frames on desktop and mobile,
verify that the canvas stays inside the phone, and verify that drawing stops
offscreen and under reduced motion. The screenshot frame and reading surface
retain the calmer layout. The regression first failed against the static phone.
Project/showroom source quality, all 101 showroom tests and the production
browser checks passed. Rust formatting, Clippy, workspace tests and dependency
audit passed as well; the workspace run completed on retry after an initial
SIGTERM without an assertion failure. The local commit is titled
`The phone keeps its visualization alive`.

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

## Desktop song scene — local preview

The supplied GNOME capture replaces the desktop hero image. A segmented spectrum
and a slowly rotating, cover-derived color disc bring the right sidebar to life.
The sharp cover stays stationary and readable. The phone moves to the lower left
so both visualizations remain visible; the desktop caption aligns to the right.
The enlarged screenshot carries the same scene and zoom geometry.

The source is the user-supplied 3456 × 2096 screenshot dated 2026-09-09 15:26:27.
The `gnome-visualizer` WebP ladder contains 800, 1200, 1600 and 2400 pixel widths.
`desktop-cover.webp` crops its existing artwork; `desktop-cover-glow.webp` is a
small, preblurred derivative, rotated by CSS every 25 seconds. No full-frame
video or external player is loaded for this effect. The bars reuse the existing
recorded visualization loop; they are illustrative and are not an analysis of
Elevator Operator. The screenshot's track metadata remains part of the capture.

Both canvas scenes and the disc stop outside the viewport, in a hidden document,
under reduced motion, and when the hero is covered by its modal. The static
capture remains usable without JavaScript. Canvas dimensions have a small lower
bound for legible segments and an upper bound for enlargement; simulation
catch-up paints at most once per browser frame.

The implementation plan is one bounded local stage: reproduce the missing
animation in the browser, add the capture and scene, verify desktop/mobile/modal
behavior and performance, run project gates, review the diff, and commit as
`The desktop showcase keeps its song scene alive`. Publication follows local
review. There is no separate repository coordination board for showroom work.

The first browser regression failed because no desktop canvas existed. The new
browser checks observe successive changing frames and disc transforms, check
phone/sidebar separation at desktop and mobile sizes, exercise the enlarged
scene, and verify offscreen, covered and reduced-motion pauses. The existing
film controls, touch seeking, responsive layout and keyboard checks remain.

Review found and corrected a covered GNOME caption and unnecessary rendering
work. Glow fills now touch only their radial bounds, constant band colors are
cached, and the hero requests a 120-pixel cover (about 4 KB); its larger derivative
is requested only in the lightbox. The animation still uses the same spectrum,
colors and simulation cadence. This avoids full-canvas fills for every bar.

Before the phone follow-up, three local Lighthouse 13.4.1 runs per mode measured mobile performance
95/95/95 and desktop 100/100/100. Median LCP was 2.929 seconds on mobile and
0.586 seconds on desktop; CLS was zero throughout. Median mobile TBT was
17.5 ms, with zero on desktop. Mobile LCP remains above the good threshold;
these navigation audits do not measure field INP or prove deployed Web Vitals.
Browser interaction checks and native gates ran separately from the timing
measurements. Physical-device and Firefox/Safari review remain manual checks.

### Phone atmosphere and hover follow-up

Local review requested the moving oil-like atmosphere on the phone as well and
reported a different hover response. A failing browser check reproduced the
phone's fixed border color overriding the shared hover token. The phone now
sets the resting token, so hover and keyboard focus use the shared edge color.
A second failing check reproduced the absent phone atmosphere. A slowly drifting
CSS color layer now accompanies the spectrum in the hero and enlarged phone.
It uses the same visibility, modal and reduced-motion policy as the other scene.

The Chromium harness now starts with an actual hover-capable pointer, then
switches to touch emulation for phones. Browser checks cover hover-color parity,
phone atmosphere movement, offscreen/covered pauses, enlargement and reduced
motion. The original atmosphere and sharper app controls remain in the capture;
this is an illustrative website loop rather than a live audio analysis.

### Completed local verification

After the phone follow-up, all 101 showroom tests, source quality, TypeScript
and the complete browser suite passed again. The final local Lighthouse runs
remain 95/95/95 on mobile and 100/100/100 on desktop. Median LCP is 2.928 seconds
on mobile and 0.666 seconds on desktop, CLS is zero, and median TBT is 4 ms on
mobile and zero on desktop. Field INP and mobile LCP remain the stated limits.

Every command from the repository merge-readiness gate passed locally, including
Rust formatting, Clippy, documentation, workspace tests, 590 isolated rule-owned
display tests and the dependency audit (only the accepted paste advisory).
The final phone changes touch only showroom code; the affected source-quality,
showroom, browser and performance checks were rerun. All changed code files are
under 800 lines. Review found no remaining defect in the implemented scope.

This is a committed local preview. The commands ran individually before commit;
a clean integration-wrapper run and hosted CI belong to subsequent integration.
Safari/Firefox and physical-device checks remain manual. The worktree's temporary
lock is released at completion; no unrelated checkout or user library was changed.
