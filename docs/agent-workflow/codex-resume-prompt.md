# Codex resume prompt

Copy the block below verbatim into Codex (or any coding agent) to continue Reprise
development from the current handoff point. It points to the durable docs and inlines the
must-not-miss safety rules, gates, and learnings. See also `AGENTS.md` and the other files in
this folder.

```text
You are taking over development of REPRISE — a native GTK4/libadwaita music
player for GNOME (Rust; MIT core + GPL-3.0 Linux GUI, a Rhythmbox successor). Another agent built it
this far and handed off to you at a clean, committed point. Continue in the same
disciplined way.

━━━━━━━━━━━━━━━━━━━━ STEP 0 — READ THESE FIRST (do not skip) ━━━━━━━━━━━━━━━━━━━━
1. docs/agent-workflow/STATUS.md — the SHARED coordination board (who's working,
   what's done, what's next). Read it first; before you touch `main`, claim its
   Lock (set OWNER: codex + task + timestamp, commit just that file); release it
   when done. Only ONE agent works main at a time.
2. AGENTS.md (repo root) — full resume instructions, gates, safety rules.
3. docs/agent-workflow/development-method.md — the working method + iron rules.
4. docs/agent-workflow/building-gtk4-rust-apps.md — GTK4/GStreamer/MPRIS/SQLite
   pitfalls, each a real bug already caught here.
5. .superpowers/sdd/progress.md — the local ledger (full task history).
6. `git log --oneline -20` — commits are ground truth. Nothing is ever pushed.

━━━━━━━━━━━━━━━━━━━━ WHERE YOU RESUME ━━━━━━━━━━━━━━━━━━━━
Current plan: docs/superpowers/plans/2026-07-12-gui-a2-cover-download.md
(GUI-A2 = opt-in online album-cover download via Cover Art Archive, 7 tasks).
- Task 1 is DONE + reviewed (commit 7c3675c): the cover_download foundation
  (ureq dep, album_key, download-cache paths).
- YOUR NEXT STEP is Task 2 (MusicBrainz URL builders + conservative matching),
  then Task 3, 4, 5 (core), then 6, 7 (frontend). Follow the plan literally —
  it contains the exact code and tests for each task.

━━━━━━━━━━━━━━━━━━━━ METHOD (per task, test-first) ━━━━━━━━━━━━━━━━━━━━
1. Write the failing test from the plan → run it → WATCH IT FAIL (RED).
2. Write the minimal implementation → run → WATCH IT PASS (GREEN).
3. Run the FULL gate battery (below) — all green.
4. Commit with the plan's exact message. One commit per task. NO attribution
   footer. DO NOT PUSH.
5. Do an adversarial review of your own diff vs the task spec (spec compliance +
   correctness). Fix Critical/Important, re-review, then move on. This project's
   review pass has caught a concurrency bug, a stale-UI bug, and a data-loss
   bug — take it seriously.
6. Append one line to .superpowers/sdd/progress.md
   (`Task N: complete (commit <hash>, base <hash>, <note>)`) AND update
   docs/agent-workflow/STATUS.md (move "Current position" forward). When you
   finish your working session, set the STATUS.md Lock back to FREE and commit it.
Execute continuously; don't stop to ask "should I continue?". Stop only when
truly blocked or the plan is done.

━━━━━━━━━━━━━━━━━━━━ GATES (all must pass before EVERY commit) ━━━━━━━━━━━━━━━━━━━━
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings   # ALL clippy lints, not just workspace ones
cargo test --workspace                                  # NOT bare `cargo test` (runs only gnome). 394 now, grows per task.
cargo audit                                             # ONLY accepted: RUSTSEC-2024-0436 (paste). A NEW advisory = STOP.
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'   # MUST be empty (core stays dependency-pure)
Every file created/edited ends < 800 lines. If an edit would breach it, EXTRACT
a cohesive sibling module — never trim doc comments to fit.

━━━━━━━━━━━━━━━━━━━━ SAFETY — NON-NEGOTIABLE ━━━━━━━━━━━━━━━━━━━━
• NEVER touch the user's real DB (~/.local/share/reprise/reprise.db, 1686 real
  tracks, library root /home/marvin/Music) or music files. Reprise only READS
  audio files; deletes are DB-only or trash-with-confirmation.
• EVERY headless run/smoke MUST be fully isolated (the real DB was clobbered
  TWICE by skipping this). Use exactly:
    dbus-run-session -- xvfb-run -a env \
      XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
      GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
      <REPRISE_SMOKE_* hooks> cargo run
  Grep your own command for XDG_DATA_HOME before running it. Omitting the XDG
  scratch dirs writes the real DB/cache; omitting dbus-run-session hijacks the
  real MPRIS/session bus; omitting GDK_BACKEND=x11 + unset WAYLAND_DISPLAY opens
  a window on the real desktop.
• English for all code, comments, log/UI strings, commit messages. (Design specs
  in docs/superpowers/specs are German; that's fine.)
• Surface problems honestly. Verify before claiming "passing/fixed/works" — run
  the command, read the output. Never claim a headless run verified rendering,
  pointer gestures, media keys, or a real network download — those are human checks.

━━━━━━━━━━━━━━━━━━━━ KEY LEARNINGS / PITFALLS (already cost real bugs) ━━━━━━━━━━━━━━━━━━━━
• RefCell discipline (the #1 recurring panic class): never hold a Ref/RefMut
  across a call that can re-enter GTK/callbacks. Clone/copy the value out in its
  OWN statement first. Store callbacks as RefCell<Option<Rc<dyn Fn>>>.
• GTK cell widgets doing async per-row work MUST use a per-cell generation token
  so a late result for a recycled row is dropped (see cover_loader.rs).
• gdk::Texture is NOT Send — build it on the main thread AFTER the .await, never
  inside a gio::spawn_blocking closure. Return a path/bytes from the worker.
• adw::ToolbarView::remove is NOT a safe no-op on an unattached widget (emits
  Adwaita-CRITICAL). Guard reparents: `if w.parent().is_some() { tv.remove(&w); }`.
• Runtime-optional features are MODULES (reprise-core::modules): a descriptor +
  a persisted `module.<id>.enabled` flag; gate behavior on modules::is_enabled.
  cover_download is such a module, default OFF (privacy — no network until the
  user enables it; the header menu toggle in Task 7 flips it).
• GUI-A2 Task 3 guidance: `ureq = "2"` default features already resolve to rustls
  (no OpenSSL in the tree) — no feature tweak needed. Rate-limit MusicBrainz to
  ≤1 req/s with a descriptive User-Agent; cache negative results.
• Cover matching is CONSERVATIVE on purpose (score ≥ 90 AND normalized
  artist+album equality, MBID preferred) — a fuzzy match would fetch the wrong
  album's art. Don't loosen it.
• image::thumbnail UPSCALES small sources (blurry) — the code clamps the target
  to the source's longest side so small covers stay native, never upscaled.

━━━━━━━━━━━━━━━━━━━━ ROADMAP AFTER GUI-A2 ━━━━━━━━━━━━━━━━━━━━
Each stage: design spec (docs/superpowers/specs) → implementation plan
(docs/superpowers/plans) → task-by-task execution as above.
• GUI-B: tag editor with MULTI-SELECT batch edit (mixed fields show
  "(multiple values)", ONLY user-changed fields are written, never clobber
  per-track values) + delete/trash (never silent file ops).
• GUI-C: browse bar (artist/album filter columns) + Rhythmbox column-layout import.
• GUI-D: first-run wizard + session restore.
• Then release: Flatpak/Flathub, gettext (German first), .desktop/AppStream.

Start now: read STEP 0, confirm the resume point against the ledger + git log,
then implement Task 2 of the GUI-A2 plan.
```
