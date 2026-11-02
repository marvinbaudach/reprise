# Showing Reprise to Rhythmbox users — design

Status: approved in brainstorming, 2026-10-31. Branch: `chore/community-launch-spec`.

## Problem

Reprise is public and installable, but almost nobody knows it exists. On 2026-10-31 the
repository had 3 stars and 73 unique visitors over the previous 14 days. The Flatpak bundle and
the Android APK attached to release 0.1.321 had been downloaded once each. A showreel
(`showroom/public/media/showreel/`) and the Showroom at <https://marvinbaudach.github.io/reprise/>
exist, but they have never been shown to anyone outside.

The obvious audience is Rhythmbox users. Reprise already imports a Rhythmbox library with its
play counts, ratings and playlists, offered on first run, in the primary menu and in
Preferences. Neither the README nor the Showroom mentions that.

Four things stand in the way of simply posting a link:

1. **Strangers would install a build that is still allowed to drop their data.** `AGENTS.md`
   still carries "Not released yet — no backwards compatibility", which lets a change discard an
   old data shape without migrating it. For a Rhythmbox user, the imported play counts and
   ratings are the whole reason to switch.
2. **There is no update path and no way to report a crash.** The release `.flatpak` bundle does
   not update itself (`RELEASING.md` names a self-hosted Flatpak repository as the deferred
   follow-up), and the application log lives only in memory, so a crash leaves a tester nothing
   to send.
3. **The audience is wary of AI-built software.** Flathub has rejected AI-generated and
   AI-assisted apps since 2026-05-29, with an undefined exception for mature projects. GNOME
   Circle adopted the AI policy of GNOME Shell extensions and has paused new submissions.
   Similar sentiment is common in GNOME and Linux forums. Reprise is built with AI assistance and
   says so in its README.
4. **The Flatpak, the only desktop channel, most likely cannot run the import at all.** See
   the facts below.

## Facts the design rests on

- **Rhythmbox is actively maintained.** 3.5.0 shipped on 2026-08-02 and 3.5.1 on 2026-08-24; the
  last commit is from 2026-10-26. 3.5 added lyrics from lrclib.net and `.lrc` sidecars, a podcast
  redesign and a new playback backend. It still builds against GTK 3 and libpeas 1, and the GTK 4
  port issue ([#1890](https://gitlab.gnome.org/GNOME/rhythmbox/-/work_items/1890)) has been open
  since 2021-04-25. Sources: [NEWS](https://gitlab.gnome.org/GNOME/rhythmbox/-/raw/master/NEWS),
  [meson.build](https://gitlab.gnome.org/GNOME/rhythmbox/-/raw/master/meson.build).
- **Wave 0 of `docs/plans/consolidation-plan.md`**, audited against `origin/dev` at
  `c47ec70bad`:
  - 0.0, 0.8 and 0.10 are done.
  - 0.6 (MSRV) and 0.9 (MCP default) are partial but irrelevant to Flatpak testers.
  - 0.1 to 0.3 (log file, log mirror, crash marker) are open or partial and block a test round.
  - 0.4 (Copy Diagnostics in the primary menu) and 0.5 (startup-failure copy) are partial.
  - 0.7 is done except item 4, the compatibility section.
- **The Flatpak** is built by the `flatpak` job of `.github/workflows/release.yml` with
  `flatpak-builder --repo=repo` and bundled without `--repo-url`. The x86_64 bundle is 8.9 MB.
  No `.flatpakref`, `.flatpakrepo` or hosted OSTree repository exists.
- **The Rhythmbox import cannot work in today's Flatpak.** This comes from reading the code; it
  has not been run. The manifest's `finish-args` grant no filesystem access beyond gvfsd.
  `default_rhythmdb_path()` in `crates/reprise-gnome/src/ui/preferences/preference_rhythmbox.rs`
  resolves `glib::user_data_dir()`, which inside the sandbox is
  `~/.var/app/io.github.marvinbaudach.Reprise/data`, so the import is never offered. The importer
  matches tracks by the file path in each `location` URI of `rhythmdb.xml`, so it only finds
  tracks whose library paths are the host paths.
- **GitHub Pages** publishes only `showroom/dist`, from `.github/workflows/pages.yml`. That
  collision is why `RELEASING.md` deferred the Flatpak repository.

## Decisions

| Question | Decision |
| --- | --- |
| Audience | Rhythmbox users first, then Linux users who manage a real library. An engineering write-up comes later as its own track. |
| Shape | Three stages: readiness, a small test round, a broad launch. Nothing is posted widely before the test round has met its exit criteria. |
| Relation to Rhythmbox | Reprise is "an alternative for Rhythmbox users", never a "successor" or a "replacement". Posts make no claim about Rhythmbox's state or its GTK 4 port. |
| AI disclosure | Every post carries one sentence that matches the README, plus a link to a short FAQ. |
| Update channel | A GPG-signed Flatpak repository, hosted in a separate repository `marvinbaudach/reprise-flatpak` with its own Pages site. |
| Flatpak permissions | The Flatpak gains `--filesystem=xdg-music` and read-only access to both places where Rhythmbox keeps its database, so the import works inside the sandbox. |
| AUR | Added before the broad launch, not before the test round. |
| Promotion | Once the Flatpak repository is public, agents no longer promote `dev` to `main` on their own. The maintainer decides each promotion, as `docs/agents/branching.md` already prescribes for public distribution. |

## 1. Message

**Core sentence**, used wherever space allows:

> Reprise is a GTK4/libadwaita music player for people who manage a real library. If you use
> Rhythmbox, it imports your library with play counts, ratings and playlists.

**What sets Reprise apart.** Each claim is checked against Rhythmbox 3.5.1 before it appears in
a post:

1. a libadwaita interface that fits GNOME;
2. a fast column library, measured with 100,000 tracks;
3. artist pages with photo, tour dates and new releases;
4. Library Doctor for missing or wrong tags;
5. an Android app with device sync.

Lyrics, podcasts and MPRIS are not differentiators, because Rhythmbox has them too.

**AI sentence**, identical in every post:

> Built with AI assistance: I direct the product, review changes and remain responsible for the
> result. The gates and tests are public.

The README gains a short FAQ entry that answers the obvious follow-up questions: what the AI
does, what the maintainer does, and what the gates check. In threads, AI criticism gets one
friendly answer with the FAQ link and no argument.

**Honesty.** The release is called an "early test release", and the README carries a list of
known limitations. Promise little and keep it.

## 2. Stage 1 — Readiness

The goal of this stage: a stranger can install Reprise, receives updates, loses no data and can
report a crash.

### 2.1 Data safety

Item 4 of task 0.7 lands. The "Not released yet — no backwards compatibility" section of
`AGENTS.md` is replaced with the text the consolidation plan prescribes, with the version and
schema number corrected to those of the first build published to the Flatpak repository. From then on,
migrations are forward-only and lossless, and settings keys are migrated instead of discarded.
This change lands with the first build published to `reprise-flatpak`, not later. Under
`docs/agents/branching.md`, public distribution begins when a build is available there, not when
the test round is announced.

The same change records in `AGENTS.md` and `docs/agents/branching.md` that public distribution
has begun, which ends the standing permission for autonomous promotion. From then on every
release reaches every tester through `flatpak update`, and the maintainer decides when that
happens.

### 2.2 Crash reports

Wave 0 tasks 0.1 to 0.4 land as the consolidation plan describes them. They are already
specified there; this design only pulls them ahead of the test round:

- a rotated log file;
- the log mirrored into it;
- a panic hook that writes a crash marker with a backtrace and offers "Copy Diagnostics" on the
  next start;
- the same "Copy Diagnostics" entry in the primary menu.

Task 0.5 waits until after the test round.

The consolidation plan predates several rule renumberings: START-3, START-4 and FB-9 now name
other rules. Rule IDs and file paths for these tasks are re-derived from the current
`docs/ux-rules.md` and code, not copied from the plan.

### 2.3 Update channel

- `release.yml` already produces an OSTree repository. It is signed with a dedicated GPG key and
  published to `marvinbaudach/reprise-flatpak`, served by that repository's own Pages site, so it
  never collides with the Showroom deployment.
- The repository keeps only the last three builds (`flatpak build-update-repo --prune
  --prune-depth=3`). Publishing must not grow that repository's git history without bound, so the
  site stays well under GitHub's 1 GB Pages limit.
- It offers a `.flatpakrepo` and a `.flatpakref`. The `.flatpakref` names Flathub as its
  `RuntimeRepo` for the GNOME runtime, so one click in GNOME Software installs Reprise.
- Bundles stay attached to releases and are built with `--repo-url`, so a bundle install also
  receives updates.
- The repository serves x86_64 only, as today.
- The maintainer creates the signing key. Its private half lives only in an Actions secret.

### 2.4 The import inside the Flatpak

- The manifest gains `--filesystem=xdg-music`, read-write, because Reprise writes tags, `.lrc`
  files and covers beside music files. The XDG music directory then appears inside the sandbox
  under its host path, so library paths match the paths in `rhythmdb.xml`.
- It gains read-only access to `~/.local/share/rhythmbox` (Rhythmbox from the distribution) and
  `~/.var/app/org.gnome.Rhythmbox3/data/rhythmbox` (Rhythmbox from Flathub).
- Inside the sandbox, the importer looks for `rhythmdb.xml` at those two host locations instead
  of `glib::user_data_dir()`.
- A library outside the XDG music directory is a listed known limitation. The README explains
  how to widen access with `flatpak override` or Flatseal.

### 2.5 Making the import visible

- README and Showroom gain a "Coming from Rhythmbox?" section with a clip of about 20 seconds:
  the import offer, its progress, and the library with play-count and rating columns.
- The clip is recorded in an isolated profile with a generated library and a generated
  `rhythmdb.xml`, never with the maintainer's library or database.
- The README gains the AI FAQ and the known-limitations list, and its Downloads section points at
  the `.flatpakref`.

### 2.6 Dress rehearsal

Before a stranger imports a library, a *copy* of the maintainer's own `rhythmdb.xml` is imported
by the installed Flatpak build, not by `cargo run`, because only the installed build shows
whether the sandbox lets the import through. The importer is pointed at the copy through its
path override.

- The run uses a fresh Flatpak data directory, so the maintainer's existing data is untouched.
- The network is unshared (`flatpak run --unshare=network`). Without network nothing is
  downloaded, so nothing is written beside the real music files the copy points at.

This touches real data and therefore needs the maintainer's explicit approval at the time it
runs. That approval names both conditions.

### Stage 1 is done when

- a fresh machine installs Reprise from the `.flatpakref`;
- that installation receives a later update through the repository;
- the installed Flatpak offers the Rhythmbox import and matches tracks in the XDG music directory;
- the dress rehearsal imports cleanly;
- a deliberately triggered crash produces a copyable report.

## 3. Stage 2 — Test round

**Scope.** 10 to 30 Rhythmbox users over two to four weeks. The request is small and concrete:
import your Rhythmbox library and use Reprise for a week.

**Where testers are found.** Small, friendly places rather than the large subreddits:

- Mastodon and the wider Fediverse with #GNOME and #Linux, in German as well as English;
- r/gnome, as a "looking for Rhythmbox users to test …" post;
- Lemmy, c/gnome.

Rhythmbox's own issue tracker and chat rooms are off limits. Advertising there would hijack its
community.

**The call.** Short and in English: the core sentence, the request, the `.flatpakref` link, the
AI sentence and how to give feedback. The drafts are prepared in advance as a doc outside the
repository, because they include German variants and are not project documents. Posting
happens from the maintainer's own accounts.

**Feedback.**

- A new issue template, "Rhythmbox import report", asks for the library size, for what did and
  did not come over (play counts, ratings, playlists, automatic playlists, podcasts, radio
  stations), and for the diagnostics report.
- One pinned thread collects everything that is not a bug.
- The maintainer answers within a day where possible. Testers who are heard stay.

**During the round**, import and data-loss bugs come before everything else, and fixes ship as
updates through the repository.

**Exit criteria.** Stage 3 starts when all of these hold:

- at least five libraries other than the maintainer's have been imported (target: ten or more);
- no data-loss bug is open;
- at least one update has reached testers without problems;
- no crash without a known cause is open.

If they do not hold after four weeks, the maintainer decides deliberately how to continue. The
launch does not happen by default.

## 4. Stage 3 — Broad launch

**Before the launch.**

- The AUR package `reprise-git`, building `main`. `main` moves only by promotion, so the package
  always matches the latest release. Stem separation stays available through the optional
  `onnxruntime` dependency, as the August Flathub decision E5 set out; without it the feature
  degrades at runtime.
- Two or three tester quotes, used with permission, and real numbers ("N libraries of up to M
  tracks imported").
- The video cut for each platform (the showreel plus the import clip), uploaded natively
  everywhere, with one copy on YouTube or PeerTube that news sites can embed.

**Launch day.** Tuesday to Thursday, from about 15:00 German time, when North America is
awake. Posts go out over a few hours, each with a text written for that community:

1. r/linux and r/gnome;
2. Lemmy (c/linux, c/gnome) and Mastodon;
3. Show HN: "Reprise – a GTK4 music player that imports your Rhythmbox library".

**Days two and three.** Tips go to OMG! Ubuntu, It's FOSS and Linuxiac, and in German to
linuxnews.de and gnulinux.ch. This Week in GNOME is approached only after asking in its Matrix
room whether an AI-assisted third-party app is welcome.

**Rules for the day.**

- The maintainer stays in the threads for the first three to four hours.
- AI criticism gets one friendly answer and the FAQ link.
- Nobody is asked to upvote. Reddit bans accounts for vote solicitation.

**Measuring.** Stars, issues from new people, AUR votes and GitHub traffic. Downloads from the
Flatpak repository cannot be counted, because GitHub Pages offers no access logs. That is the
price of free hosting.

## 5. Later — the engineering write-up

The architecture, the 100,000-track measurements and the openly documented AI pipeline make a
separate story for a developer audience: a blog post, possibly with its own Show HN. It starts
no earlier than two weeks after the launch and gets its own design.

## Out of scope

- Flathub and GNOME Circle submissions, which their AI policies rule out for now.
- An aarch64 Flatpak.
- Stem separation in the Flatpak.
- Wave 0 task 0.5 and everything beyond Wave 0.
- Paid hosting or download analytics.
