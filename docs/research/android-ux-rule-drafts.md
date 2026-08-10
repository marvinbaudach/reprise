# Android-UX — Regelentwürfe (Vorschlag, nicht angewandt)

Erzeugt 2026-08-01 aus vier parallelen Recherchen. **Nicht in**
`docs/ux-rules.md` **eingetragen** — Status, Testebene und Geltungsbereich
entscheidet der Mensch, und zwei blockierende Pläne arbeiten in der Datei.

Jeder Entwurf trägt `[planned]` und **keine** Testebene. Vor dem Eintragen
kommt der Geltungsbereich aus `p5-surface-scopes.md` dazu; die Reihenfolge
ist bindend: `[status] [ebene] [surface:…]`.

## Plattformpflichten der Wiedergabe — `AUD`, 23 Entwürfe

Audio-Focus, Becoming-Noisy, Foreground Service, Benachrichtigung, Doze, Medientasten. Quellen: developer.android.com, Media3-Doku.

Belege je Regel-ID in `ux-android-audio.md` (Scratchpad).

- **AUD-1** [planned] — Playback never begins without confirmed audio focus:
  the app requests audio focus (or delegates it to ExoPlayer via
  `setAudioAttributes(..., handleAudioFocus = true)`) immediately before
  starting a player and checks that focus was actually granted before
  making sound. A track that becomes audible before or without a granted
  focus request is exactly the double-audio collision the focus API exists
  to prevent.
- **AUD-2** [planned] — Permanent focus loss ends playback, not just
  interrupts it: on `AUDIOFOCUS_LOSS` the app pauses immediately and does
  not auto-resume, because no `AUDIOFOCUS_GAIN` callback will ever arrive
  for a permanent loss — the user must take an explicit action to restart.
  Treating a permanent loss like a transient one leaves playback silently
  dead behind a UI that still claims "playing."
- **AUD-3** [planned] — Transient focus loss self-heals on regain: on
  `AUDIOFOCUS_LOSS_TRANSIENT` (or `_CAN_DUCK`) the app pauses or ducks but
  preserves position and state, then on the matching `AUDIOFOCUS_GAIN`
  resumes playback and restores volume automatically, without the user
  pressing play again. A phone call or navigation prompt is an
  interruption, not a stop.
- **AUD-4** [planned] — Spoken-word content pauses on transient loss
  instead of ducking: whenever the playing item is not music (podcast
  episode, audiobook), the app reacts to a "may duck" grant by pausing
  anyway, per the documented guidance that automatic ducking "isn't useful
  when playing spoken content... the app should pause instead." Ducked
  speech turns unintelligible in a way ducked music does not.
- **AUD-5** [planned] — The Android 12 forced fade-out is a backstop, not
  the mechanism: on API 31+, a player that keeps outputting audio after
  losing focus gets faded out and muted by the system itself once another
  app is granted `AUDIOFOCUS_GAIN`. Relying on this instead of reacting to
  the focus-change callback makes playback appear to go silent for no
  reason, and it stays muted until the app explicitly re-requests focus.
- **AUD-6** [planned] — Audio focus can only be requested from a state the
  platform already considers active: on Android 15+ (API 35), a focus
  request fails outright (`AUDIOFOCUS_REQUEST_FAILED`) unless the app is
  the top app or already running a qualifying foreground service. A
  background-triggered playback attempt (queued action, stale timer,
  delayed intent) that hasn't first established the foreground service
  acquires no focus and produces no sound, with nothing that looks like a
  crash to explain why.
- **AUD-7** [planned] — Unplugging silences, it never surprises: the app
  registers a receiver for `ACTION_AUDIO_BECOMING_NOISY` only while
  something is playing (registered on play, unregistered on stop/pause)
  and pauses — not ducks — immediately on receipt. The intent exists
  specifically for the moment output is about to jump from
  headphones/Bluetooth to the speaker at full volume.
- **AUD-8** [planned] — `mediaPlayback` is a declared, permissioned
  foreground-service type, not an implicit one: targeting Android 14+
  (API 34) requires both the `FOREGROUND_SERVICE_MEDIA_PLAYBACK`
  permission and `android:foregroundServiceType="mediaPlayback"` on the
  service element. Omitting either throws
  `MissingForegroundServiceTypeException` or a `SecurityException` the
  moment `startForeground()` is called — the service crashes outright
  instead of degrading playback.
- **AUD-9** [planned] — Promotion to foreground has a hard deadline: after
  `Context.startForegroundService()`, the service has "a few seconds" to
  call `Service.startForeground()`; missing the window is a fatal
  `ForegroundServiceDidNotStartInTimeException`. The path from "user
  pressed play" to "notification posted" must never block on I/O (library
  scan, disk read, network) before promoting.
- **AUD-10** [planned] — The service starts from something the user just
  touched, never from thin air: on Android 12+ (API 31), starting the
  foreground service while the app has no visible activity and isn't
  already in a qualifying exempt state throws
  `ForegroundServiceStartNotAllowedException`. Playback must always launch
  from a user-visible trigger — an activity, a notification or
  media-button action, an exact alarm the user scheduled — never from a
  bare background callback or a missed reconnection attempt.
- **AUD-11** [planned] — The foreground exemption lasts exactly as long as
  it's earned: `mediaPlayback` services are currently excluded from
  Android 15's 6-hour background runtime cap that applies to
  `dataSync`/`mediaProcessing`, but only for as long as the service is
  doing what its type promises. A paused player idling past the session
  library's own auto-demotion window (10 minutes with no user interaction)
  drops out of the foreground state and can be killed like any background
  process — "paused" is not a substitute for "stopped" when the app
  expects to keep running.
- **AUD-12** [planned] — The notification cannot outlive the thing it
  announces, and vice versa: the media notification stays pinned and
  undismissable for as long as the foreground service is up, and the only
  way to remove it is for the app itself to release the player or clear
  the playlist. A notification that lingers after playback has genuinely
  ended, or one that vanishes while a track is still audible, are both
  bugs the platform will not fix on the app's behalf.
- **AUD-13** [planned] — Denying notifications never denies playback: on
  Android 13+ (API 33), if the user declines `POST_NOTIFICATIONS`, the
  foreground-service notification disappears from the drawer, but the
  service keeps running and still appears in the system's Task Manager
  surface. The app must not gate starting or continuing playback on that
  permission being granted — refusing to play because it can't show a
  notification confuses a display permission for a playback permission.
- **AUD-14** [planned] — Lock-screen and quick-settings controls read only
  from the session's own metadata: the seek bar, title, artist, and
  artwork shown outside the app come exclusively from the session's
  `MediaMetadata` (title/display-title, artist, duration, artwork URI) and
  `PlaybackState`. Omitting duration disables the seek bar's progress
  display, and omitting the seek action from the declared actions disables
  scrubbing entirely — independent of whether the player itself supports
  seeking.
- **AUD-15** [planned] — Media buttons only reach an active session, or one
  with a receiver to wake it: a headset or Bluetooth play/pause/next press
  routes to the last session that played audio locally, and only restarts
  an inactive session if that session registered a media-button receiver.
  A session that goes inactive without one is simply unreachable by
  hardware buttons until the user reopens the app.
- **AUD-16** [planned] — Bluetooth and car head units show whatever the
  session last published, not what's actually playing: track-change
  metadata must be pushed to the session on every transition, because
  connected devices render exactly what the session last set. A missed
  metadata update after a skip leaves external displays showing the
  previous track while audio has already moved on.
- **AUD-17** [planned] — Swiping the app from Recents does not stop music
  that's actually playing: the expected behavior is that a service with
  ongoing playback survives task removal, and a paused or stopped one does
  not. Overriding task-removal handling to unconditionally kill the
  service on swipe breaks the platform-standard expectation that dismissing
  an app from the multitasking view is not the same gesture as pressing
  stop.
- **AUD-18** [planned] — Doze's network freeze only lifts while a
  foreground service is actually up: network-dependent work (streaming a
  track, fetching remote podcast artwork, refreshing a feed) is exempt
  from Doze's suspended network access only for the duration an eligible
  foreground service is running. The same work triggered by a scheduled
  job or alarm outside of active playback is deferred to the next
  maintenance window like any other background app — sometimes for hours.
- **AUD-19** [planned] — The battery-optimization exemption prompt is
  reserved for the one thing that justifies it: Play policy permits
  requesting the ignore-battery-optimizations exemption only when the
  app's core function is adversely affected without it, and background
  music playback is the named acceptable case — but the `mediaPlayback`
  foreground service already carries that exemption while a track is
  playing. The app must not prompt for the blanket exemption to cover
  unrelated background work (library scans, sync) that isn't itself
  playback.
- **AUD-20** [planned] — Reading another app's audio files requires asking
  for them by name: on Android 13+ (API 33), files Reprise did not itself
  write into the media store are invisible to a library scan without the
  runtime-granted `READ_MEDIA_AUDIO` permission. Scoped storage does not
  merely restrict write access to such files — it hides their existence
  entirely until the permission is granted.
- **AUD-21** [planned] — Editing or deleting an imported file the app
  doesn't own requires the user's consent, per file or per batch:
  modifying or deleting a media-store entry that another app contributed
  throws a `RecoverableSecurityException` unless the app resolves it
  through the returned consent intent, or — for batch operations on
  Android 11+ — through a `createWriteRequest()` grant. There is no
  silent, permission-only path to rewriting or removing a file the app did
  not create.
- **AUD-22** [planned] — Publishing declares the foreground service before
  the store does: Play Console requires a per-type foreground-service
  declaration (a description of the feature plus a demo video) before an
  app using `FOREGROUND_SERVICE_MEDIA_PLAYBACK` can be published or
  updated. An undeclared or misdescribed use is a listing-blocking policy
  violation, independent of whether the manifest and code are otherwise
  correct.
- **AUD-23** [planned] — The target SDK floor moves every year whether or
  not the app does: Play requires apps to target a rising minimum API
  level on an annual cycle, and an app not rebuilt against the current
  target SDK by the deadline loses the ability to publish updates —
  entirely apart from whether its actual behavior still works.

## Material-3-Muster und Interaktion — `MOB`, 24 Entwürfe

Navigation, Predictive Back, Listen, Now Playing, Bottom Sheets, Gesten, Trefferflächen. Quellen: m3.material.io, Compose-Doku.

Belege je Regel-ID in `ux-android-m3.md` (Scratchpad).

- **MOB-1** [planned] — The bottom navigation bar carries three to five
  top-level destinations, never fewer and never more. Material 3 bars
  both ends explicitly — under three degrades to redundant tabs, over
  five collides once labels localize into longer strings — so the
  destination set for Reprise Android (Library/Browse, Search, Queue,
  Playlists, …) must be chosen to fit inside this band, not designed
  first and squeezed in after.
- **MOB-2** [planned] — Re-tapping the currently active bottom-nav
  destination resets that destination's own scroll position to the top
  and does not push a new entry onto its history stack (MOB-5). Material
  3 treats reselection as a same-place reset, not a navigation, so it
  must stay outside whatever back-stack model MOB-5 defines — otherwise
  every idle tap on the current tab silently grows the back history.
- **MOB-3** [planned] — On medium-and-wider window size classes
  (unfolded foldables, tablets in landscape, ChromeOS/DeX) the bottom
  navigation bar is replaced by a navigation rail carrying the same
  destinations in the same order, with the same reselect-to-top rule
  from MOB-2. Material 3 reserves the rail for "medium window size
  classes and larger" and explicitly tells desktop-shaped layouts not to
  use a bottom bar at all — this is a breakpoint switch, testable at the
  WindowSizeClass boundary, not a cosmetic option.
- **MOB-4** [planned] — Predictive back is mandatory on every
  back-navigable screen, wired through `OnBackPressedCallback` /
  `PredictiveBackHandler`, never through raw `KeyEvent.KEYCODE_BACK`
  interception. Android no longer supports intercepting the back key
  event at all, and un-migrated interception is flagged as broken by the
  platform itself — a screen that swallows the key event doesn't just
  lose its own animation, it breaks the swipe-preview for every screen
  behind it in the stack.
- **MOB-5** [planned] — Reprise's desktop browser-style history (one
  global stack, content clicks push, a place/sidebar switch restarts the
  stack — NAV-2/NAV-2a) carries over to Android unchanged in shape: an
  app-owned back stack fed into `PredictiveBackHandler`, not into
  Navigation Compose's own stack. This is not a deviation Android merely
  tolerates — its own reference example for custom back handling is a
  WebView that navigates "browsing history instead of the previous
  screens in your app," and Navigation 3 states outright that the app
  "own[s] its back stack" with no required shape. The desktop history
  model is the Android-endorsed pattern here, not a workaround.
- **MOB-6** [planned] — When the app-owned history stack is empty (the
  Android analogue of NAV-2a: "Back with no stack entries is disabled,
  never a no-op"), predictive back falls through to the system default —
  minimize the app / go to the previous task — instead of staying on
  screen as a dead swipe or force-closing the activity. A swipe that
  visibly starts a preview and then resolves to nothing violates
  Android's own contract for the gesture, which exists specifically "to
  inform users where their actions will take them."
- **MOB-7** [planned] — Every bottom sheet, dialog, and expanded search
  overlay owns its own back press: swiping or pressing back while one is
  open closes only that layer, leaving the underlying screen and its
  history stack (MOB-5) untouched. Overlays are never entries in the
  app-owned history stack — they are dismissed, not navigated away
  from — which is the mobile equivalent of Esc on a desktop popover.
- **MOB-8** [planned] — Full-screen Now Playing sits layered above
  whatever place is currently showing; it is not a new entry in the
  app-owned history stack (MOB-5). Back or predictive back from Now
  Playing always collapses it to the mini-player and returns focus to
  the place underneath in a single step, regardless of how deep that
  place's own history is — Now Playing must never take more than one
  back press to leave, and predictive back must never surface it as the
  side effect of popping unrelated history.
- **MOB-9** [planned] — The persistent mini-player bar shows, at
  minimum, artwork thumbnail, track title, artist, and a play/pause
  control, and the whole bar is a single tap target that opens full Now
  Playing. Android's own Compose media tooling ships this exact shape as
  a first-class pattern (a `MiniController` composable distinct from the
  full `Player`), which is the platform's baseline for what a
  glanceable, one-tap-away player bar needs to contain.
- **MOB-10** [planned] — If the mini-player-to-full-Now-Playing
  transition is built as a shared-element expansion (artwork growing
  into the full cover), it can only receive the predictive-back preview
  animation from a self-managed back stack driven by
  `PredictiveBackHandler` — Android states plainly that this shared
  element transition "does not work with `FragmentManager`, the
  Navigation Component, or Navigation Compose." Given MOB-5 already
  commits Reprise Android to an app-owned back stack for the browser
  history, this costs nothing extra: the same mechanism pays for both
  features.
- **MOB-11** [planned] — Track rows default to the two-line list-item
  height band (title + artist/album line) as the library's baseline
  density; a denser variant is a separate, explicit toggle, never a
  silent default. Material 3 fixes list-item height to "the tallest
  element within the list item" across three bands (56/72/88dp), so
  which band Reprise's row occupies is a deliberate content decision,
  not an incidental one.
- **MOB-12** [planned] — Libraries above a defined track-count threshold
  get a fast-scroll affordance on the main track list. Material 3 has no
  built-in fast-scroll list component, and a bare `LazyColumn` degrades
  to a long, slow drag once a library reaches the thousands of tracks
  that Reprise's desktop libraries already routinely hold. The threshold
  and the concrete affordance (draggable scrollbar handle vs. an
  alphabet/letter index) are an open decision — see below.
- **MOB-13** [planned] — Long-press on a track/album/playlist row enters
  selection mode, swapping the top app bar for a contextual one
  (selection count + actions) rather than opening a bottom sheet or menu
  for that same row. Android's dynamic-top-app-bar pattern implements
  exactly this: a long click "toggles its selection state" while a plain
  click keeps performing the row's normal action — one gesture, one
  outcome, and it must not also trigger MOB-15's context-menu sheet.
- **MOB-14** [planned] — Track and queue lists key every `LazyColumn`
  item on a stable, content-derived id (track id, queue-slot id), never
  on list position. Compose's own lazy-list guidance is explicit that
  position-keyed items "lose any remembered state" — including scroll
  position — the moment the dataset reorders, which is exactly what
  happens on every queue reorder (MOB-18) or removal (MOB-17). This is
  the direct Compose analogue of the desktop rule that the scroll anchor
  is "track/album ID plus offset, never a raw pixel value" (NAV-5).
- **MOB-15** [planned] — The "…" action affordance on a track, album, or
  playlist row opens a modal bottom sheet, not a dropdown menu — this is
  Reprise Android's direct equivalent of the desktop right-click context
  menu. Material 3 recommends a modal bottom sheet as the mobile
  alternative "to inline menus or simple dialogs… especially when
  offering a long list of action items, or when items require longer
  descriptions and icons," which is exactly the shape of a track's
  action list (Play next, Add to queue, Add to playlist, Go to
  album/artist, Remove, …).
- **MOB-16** [planned] — Short, low-cardinality, icon-free choices (sort
  order, a handful of filter toggles) use a dropdown/overflow menu
  instead of a bottom sheet. Material 3 draws this line explicitly: menus
  are for exactly this kind of compact choice and are more space
  efficient, while sheets earn their weight only once items "require
  longer descriptions and icons" (MOB-15) — a menu used for a long,
  icon-heavy action list or a sheet used for a three-item sort toggle are
  both the wrong component for their content.
- **MOB-17** [planned] — Swipe-to-remove is allowed on Queue rows only
  paired with a mandatory Undo snackbar carrying the single action
  Material 3 permits per snackbar. `SwipeToDismissBox` is Compose's own
  sanctioned pattern for exactly this "dismiss on swipe" interaction, but
  a swipe is easy to trigger by accident while scrolling a long queue —
  the undo affordance is not optional polish, it is the gesture's error
  recovery.
- **MOB-18** [planned] — Drag-to-reorder in the Queue requires an
  explicit pickup affordance (a drag handle, or a long-press before the
  drag starts) plus haptic feedback at pickup and drop; it never starts
  from a plain touch-and-move on the row. An implicit drag on a
  scrollable list is indistinguishable from a scroll gesture at the
  moment of touch-down, so Compose's drag/reorder tooling only becomes
  safe to use once the pickup itself is unambiguous — the concrete
  affordance (handle icon vs. long-press) is an open decision, see
  below.
- **MOB-19** [planned] — No swipe gesture, drag handle, or custom touch
  target sits inside the system gesture-navigation inset at the screen
  edges. Android's predictive-back guidance is explicit: "avoid adding
  touch gestures or drag targets under these gesture areas," because
  anything living there gets stolen by the system back/forward swipe
  before the app ever sees it — this applies directly to Queue
  swipe-to-remove (MOB-17) and any edge-anchored control.
- **MOB-20** [planned] — Every swipe-to-remove (MOB-17) and drag-to-reorder
  (MOB-18) exposes an equivalent TalkBack custom accessibility action, so
  the same outcome is reachable without performing the gesture. Compose's
  accessibility guidance names this pattern directly: "custom actions can
  be used for more complex touchscreen gestures, like swipe to dismiss or
  drag and drop, as these can be challenging for users with motor
  impairments" — a gesture without a custom-action fallback is a feature
  TalkBack users cannot use at all, not a degraded experience.
- **MOB-21** [planned] — Every interactive element — transport controls,
  per-row icon buttons, chips, the mini-player's tap target — is at
  least 48×48dp, with at least 8dp of clear space to the next touch
  target. This is Android's own accessibility minimum, stated as
  "touch targets at least 48x48dp, separated by 8dp of space or more,"
  and it is a hard floor, not a target to approach: anything smaller
  fails automated accessibility scanners as well as real thumbs.
- **MOB-22** [planned] — When a list row carries more than one inline
  action button (e.g. favorite + overflow on the same track row), the
  row groups them into custom accessibility actions on the row's
  semantics node rather than leaving them as separate focusable children.
  Compose's accessibility guidance calls out this exact case — a
  screen-reader user "navigating such a list can become tedious as the
  same action would be focused repeatedly" — so an ungrouped row
  multiplies TalkBack's per-item traversal cost by however many buttons
  it has.
- **MOB-23** [planned] — Explicitly enqueueing one or more tracks never
  starts playback, including when the queue was empty or exhausted; only a
  separate play action may make an enqueued track audible.
  <!-- REVIEW: Regelvorschlag -->
- **MOB-24** [planned] — While no queued track is loaded, the Queue view
  includes the current queue entry so an explicit enqueue remains visible and
  every row action addresses the same item the user sees.
  <!-- REVIEW: Regelvorschlag -->
- **MOB-25** [planned] — Deleting the track that is currently playing advances
  playback to the next surviving track in play order; it stops only when no
  queued track survives.
  <!-- REVIEW: Regelvorschlag -->

## Speicherzugriff und Berechtigungen — `STOR`, 18 Entwürfe

SAF, Berechtigungsverlust, Schreibzugriff, Play-Policy. Quellen: developer.android.com.

Belege je Regel-ID in `ux-android-storage.md` (Scratchpad).

- **STOR-1** [planned] — Folder selection uses ACTION_OPEN_DOCUMENT_TREE,
  never a manual path prompt: the app never asks the user to type or confirm
  a filesystem path for the library root. It launches
  `ACTION_OPEN_DOCUMENT_TREE` (or `StorageVolume.createOpenDocumentTreeIntent()`
  for a specific volume) and receives an opaque tree `Uri`, because Android
  grants no other durable, Play-compliant way to hand a whole folder to an
  app.
- **STOR-2** [planned] — Picker starting point, never a forced target: the
  app may pass `EXTRA_INITIAL_URI` to open the picker inside a plausible
  starting folder (the last-picked parent, or the primary volume's `Music/`
  tree) but never assumes the user accepts that folder — the user can
  navigate anywhere reachable from there, and the app must accept whatever
  tree they ultimately confirm, not the one it suggested.
- **STOR-3** [planned] — Restricted roots surface as guidance, not a silent
  failure: the internal storage root, the root of any SD card volume, and
  the `Download` directory cannot be granted via `ACTION_OPEN_DOCUMENT_TREE`
  at all on Android 11+. If the user tries to pick one of these, the OS
  itself blocks the selection — the app's copy tells them to pick the actual
  music subfolder instead of leaving them at an unexplained dead end in the
  picker.
- **STOR-4** [planned] — Every returned tree Uri is persisted before first
  use: the callback for `ACTION_OPEN_DOCUMENT_TREE` calls
  `contentResolver.takePersistableUriPermission()` with read+write flags in
  the same turn it receives the `Uri`, before any other use of it. A grant
  that is only used transiently is gone the next time the process restarts —
  that would silently turn a working library into an inaccessible one on the
  very next cold start, for no reason visible to the user.
- **STOR-5** [planned] — Persisted grants are inventory, not accumulation:
  every persisted tree grant the app is not actively using as a library root
  gets released via `releasePersistableUriPermission()` the moment the
  corresponding root is removed from the library, rather than left to
  linger. Android caps an app at a fixed number of persisted grants (512 on
  API 30+, 128 below) — a "remove folder" action that only deletes the
  database row while keeping the OS-level grant silently eats into that
  budget until multi-folder users hit the cap for no visible reason.
- **STOR-6** [planned] — Access is verified, never assumed: Android sends
  the app no event when a persisted grant is revoked — no broadcast, no
  callback comparable to a mount/unmount signal. The only way to learn a
  tree `Uri` stopped working is to try it and fail, so the app re-checks
  reachability (a cheap query against the tree, or the next scan) instead of
  trusting a grant it already holds to still be good indefinitely.
- **STOR-7** [planned] — Two loss shapes, not one: an inaccessible tree
  `Uri` is classified before the app reacts to it. A volume that is
  physically absent (SD card pulled, not yet remounted) is provisionally
  lost — same tree, same grant, Android reports it back once the volume
  returns. A grant the user revoked in Settings, an app reinstall or "clear
  storage", and the picked folder itself being moved, renamed, or deleted
  are all reported identically by the OS (`SecurityException` /
  `FileNotFoundException` on the next access) but none of them self-heal —
  only a fresh `ACTION_OPEN_DOCUMENT_TREE` pick restores them. Collapsing
  both into one grey "storage problem" throws away exactly the distinction
  the desktop build already keeps (`MissingReason::Unmounted` vs `Deleted`
  vs `Unknown`).
- **STOR-8** [planned] — Volume return heals silently: when a previously
  granted, currently unreachable tree's volume reports mounted again
  (`StorageManager.StorageVolumeCallback`, or simply the next successful
  access succeeding), the library resumes exactly where the desktop's
  mount-event healing (`P-6`, `PLAY-5b`) already lands — grayed tracks turn
  normal again without a rescan the user has to trigger by hand, and without
  a dialog congratulating them for reinserting a card.
- **STOR-9** [planned] — Permanent loss gets one explicit action, not
  silence: when a tree `Uri`'s loss cannot self-heal (STOR-7's second
  bucket), the app surfaces exactly one place to fix it — a persistent entry
  in Issues (the same place Import errors/Missing files already live per
  `NAV-1`/the `ISSUES` block) offering "Reconnect", which reopens
  `ACTION_OPEN_DOCUMENT_TREE` pre-aimed at that folder's last known location
  via `EXTRA_INITIAL_URI`. The user is never asked to remember or retype
  which folder it was.
- **STOR-10** [planned] — Loss is never read as deletion: exactly like
  `PLAY-4a/4b`, `PLAY-5a/5b`, and `P-6` on the desktop, a track whose tree
  `Uri` is unreachable — whichever of STOR-7's two buckets it falls in — is
  never removed from the database and never silently drops out of the
  visible library. It grays out, is skipped on playback and queue advance,
  and is excluded from enqueue, but the row and its metadata stay exactly
  where the user left them until evidence (a successful re-access) says
  otherwise.
- **STOR-11** [planned] — Each granted tree is its own root, not a merged
  filesystem: a device with an internal music folder and an SD card music
  folder needs two independent `ACTION_OPEN_DOCUMENT_TREE` grants (SD-card
  access, where offered at all, goes through
  `StorageVolume.createOpenDocumentTreeIntent()` on that specific
  `StorageVolume`). The app tracks each grant as its own persisted `Uri`
  with its own loss/heal state per STOR-7/STOR-8 — never a single combined
  "library folder" the way the desktop's one library root implicitly
  suggests.
- **STOR-12** [planned] — SD card write access is not MediaStore's write
  access: MediaStore can only modify the primary shared-storage volume — the
  SD card is read-only through it. That limitation does not carry over to a
  folder granted through SAF: an SD-card tree `Uri` carries the same
  read/write grant as an internal one. Tag/cover/lyrics writeback must key
  its "can I write here" check off the SAF document flags (STOR-13), never
  off "is this the primary volume" — or it will wrongly disable writeback on
  every SD card even though SAF itself allows it.
- **STOR-13** [planned] — Write capability is checked per document, not
  assumed from the grant: before offering an edit action (tag save, cover
  writeback, `.lrc` sidecar write) on a specific file, the app reads that
  document's `Document.COLUMN_FLAGS` and requires `FLAG_SUPPORTS_WRITE`
  explicitly. `DocumentFile.canWrite()` also returns true for a
  `FLAG_SUPPORTS_DELETE`-only document, so relying on it would offer an edit
  action that then fails at the moment of writing.
- **STOR-14** [planned] — Writes go through a file descriptor, never a
  path: an accepted write (tag save, cover embed, sidecar creation) opens
  the target document via `ContentResolver.openFileDescriptor(uri, "w")`.
  There is no filesystem path to open with `std::fs` on a SAF-backed source
  — the writeback layer's present assumption of a real path
  (`crates/reprise-core/src/library/tag_edit_write.rs`,
  `cover_writeback.rs`, `lyrics/sidecar_write.rs`) cannot survive unmodified
  onto this source type; it needs a writer abstraction under it.
- **STOR-15** [planned] — Directory listing is batched, never node-by-node:
  a scan walks a SAF tree using
  `DocumentsContract.buildChildDocumentsUriUsingTree()` with one query per
  directory that returns a full projection (document id, display name, mime
  type, size, last-modified) for all of that directory's children at once —
  never `DocumentFile.listFiles()`/`findFile()` per entry. Each `DocumentFile`
  call is its own `ContentResolver` round trip across two IPC hops; doing
  that once per file is what turns a sub-second desktop scan into tens of
  seconds at the same track count.
- **STOR-16** [planned] — A scan diffs against what it already knows,
  never re-walks blind: SAF exposes no lightweight "what changed" signal
  comparable to inotify (`crates/reprise-core/src/library/watcher.rs`
  today), so a rescan compares the current listing's document ids and
  last-modified timestamps against what the previous scan recorded, and only
  opens/reads tags for entries that are new or changed. Re-reading tags for
  an unchanged tree on every scan is the single most expensive default this
  design could ship with, given STOR-15's per-node IPC cost.
- **STOR-17** [planned] — The playback notification never depends on
  POST_NOTIFICATIONS: notifications tied to a `MediaSession` are exempt from
  the Android 13+ `POST_NOTIFICATIONS` runtime permission, so a denial never
  prevents the foreground playback service from starting or its transport
  controls from working. The app requests `POST_NOTIFICATIONS` only for
  notifications that are *not* exempt (e.g. "scan finished", "import
  failed") and asks for it in the context of the first such notification,
  never as a blanket startup prompt that appears to gate playback.
- **STOR-18** [planned] — MANAGE_EXTERNAL_STORAGE never appears in the
  manifest: a local music library is exactly the "media files access" /
  "user manually selects files" case Play policy names as not qualifying
  for All files access, and SAF already delivers what this app needs
  (folder grant, sibling files, write access) without it. Declaring it would
  fail Play's review and cost a Permissions Declaration Form the app has no
  case for, for zero functional gain over STOR-1.

## Praxis: was Nutzer vertreibt — `MOBQ`, 14 Entwürfe

Destilliert aus Issue-Trackern, Bewertungen und Foren zu Auxio, Gramophone, Musicolet, Symfonium und anderen.

Belege je Regel-ID in `ux-android-praxis.md` (Scratchpad).

- **MOBQ-1** [planned] — Group tracks into albums tolerantly, not by literal tag
  equality: fall back to album title + track-count or album title + folder-path
  proximity when the Album Artist tag is missing or inconsistent across tracks, so
  a single compilation or various-artists album doesn't fracture into one entry per
  track. Users hit this constantly with soundtracks and compilations, because
  Android's own media indexing only reliably carries the plain Artist tag, and it's
  documented as a years-old, unresolved-by-default failure mode across multiple
  players and forums, not a one-off.
- **MOBQ-2** [planned] — Normalize artist identity before grouping: collapse case
  variants, "feat./ft./&" suffix noise, and multi-value tag separators (";", "/",
  ",") into one canonical artist entry instead of one row per raw string. Left
  unhandled, this is one of the most-cited causes of a library that looks "broken"
  on first scan even though every individual file is correctly tagged.
- **MOBQ-3** [planned] — Ship a raw folder/filesystem browse view as a permanent
  fallback alongside the tag-based library, not a buried option. When tag-based
  grouping gets an album or artist wrong, folder view is the only way a user can
  still find and play a track — it's the one feature reviewers of folder-capable
  local players (Vanilla Music, Gramophone, BlackPlayer) call out unprompted, while
  VLC is specifically penalized for not treating folder browsing as a first-class,
  bulk-operable list.
- **MOBQ-4** [planned] — Make the first scan progressive and playable, not a
  blocking gate: surface and let the user play tracks as they're indexed, rather
  than showing an empty or spinner-locked library until the whole scan finishes.
  Poweramp is criticized by name for scanning only while it's in the foreground and
  active, which stretches first-run time on large libraries into something users
  notice and complain about.
- **MOBQ-5** [planned] — Target correctness at 100k+ tracks, not "works on my test
  library." Independent sources put the point where naive scanners start
  destabilizing around 50k tracks, and Symfonium's own scanner needed a dedicated
  fix release because it broke past roughly 95k songs — scale problems here aren't
  hypothetical, they show up at real collection sizes people actually have.
- **MOBQ-6** [planned] — Isolate scan failures per file: an oversized embedded
  cover (e.g. a 3000×3000 image) or a single corrupt tag must never abort or
  corrupt the scan of the rest of the library. Auxio's own troubleshooting docs
  name this exact failure as a "cascading failure" — one bad file silently hides
  good ones behind it.
- **MOBQ-7** [planned] — Identify tracks in playlists, queue, and resume position
  by a stable reference (file path and/or content hash), never by mutable tag
  fields. Auxio documents playlists and playback state silently breaking after a
  library rescan specifically because it identifies songs by metadata — editing a
  tag with any tag editor, including the app's own, orphans the entry.
- **MOBQ-8** [planned] — Persist playback position and queue eagerly, on every
  change, not only on clean app exit. Users report specific local players (Pulsar,
  Music Folder Player) failing to restore position at all after the process is
  killed in the background, and for a local player there's no server-side state to
  fall back on — if it isn't written to disk immediately, it's just gone.
- **MOBQ-9** [planned] — Give lists with thousands of entries an alphabetic
  fast-scroll index (jump-to-letter), not just an inertial scrollbar. Forum threads
  exist purely to ask which player still has this, and it's specifically marketed
  as a differentiator by players built for big libraries — a plain scrollbar over
  20,000 tracks is not browsing, it's guessing.
- **MOBQ-10** [planned] — Include a fast, minimal in-app tag and artwork editor.
  Real local libraries always have some fraction of wrong or missing tags, and the
  ability to fix a title or attach artwork without leaving the app or reaching for
  a separate PC tool is one of the specific, repeatedly-praised features behind
  both Musicolet's and Retro Music's followings.
- **MOBQ-11** [planned] — Make the zero-configuration default a fully usable app;
  treat deep customization as progressive disclosure the user opts into, never a
  precondition. Symfonium reviewers name "days of trial-and-error setup" as its
  main cost, in direct contrast to the praise Musicolet gets for working
  immediately with no setup screen at all.
- **MOBQ-12** [planned] — Open to a usable library instantly from cache; never
  block the opening screen on a fresh rescan. Musicolet's speed advantage is
  repeatedly attributed less to raw performance than to having nothing to wait on
  at launch — no network ping, no forced re-index — and reviewers cite exactly that
  as why the app "just opens."
- **MOBQ-13** [planned] — When a file is excluded from the library — unsupported
  format, storage-permission gap, corrupt tag — tell the user what was skipped and
  why, instead of presenting a library that's silently shorter than their file
  count. Auxio's troubleshooting page exists largely because "missing music" is a
  top support request, and as shipped, users have no way to distinguish "not
  scanned yet" from "silently dropped."
- **MOBQ-14** [planned] — During first-run onboarding, proactively explain and
  request exemption from battery optimization, framed around the concrete symptom
  ("music may stop when your screen is off") rather than left for the user to
  discover after playback dies mid-song. This is one of the most consistently
  documented reasons background audio breaks on Android across apps in general, it
  is nearly always the same OS behavior working as designed, and an onboarding step
  is cheaper than the trust lost when someone concludes the app is just broken.

## Zusammenfassung

**80 Entwürfe.** Zum Vergleich: aus dem bestehenden Regelwerk erbt
die Android-App 61 Regeln (`p5-surface-scopes.md`). Ihr Verhalten bezieht
die mobile Oberfläche also überwiegend aus diesen Entwürfen, nicht aus
dem Bestand.
