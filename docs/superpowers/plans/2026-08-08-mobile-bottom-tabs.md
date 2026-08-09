# The bar at the bottom starts working, and the screen swipes

Measured on the owner's phone this evening, from the running app.

## What is there today

**The four tabs are chips at the top** of the library — Titles, Albums, Artists,
Favourites, the selected one carrying a check mark. They work, through
`surfaceState.selectedTab` in `BrowseScreen.kt`.

**The bottom bar has exactly one destination.** `LibraryFrame.kt` draws a
Material 3 `NavigationBar` containing a single item labelled "Library", with
`selected = true` and `onClick = {}`. A bar with one button that leads nowhere.
The same file draws a `NavigationRail` for the wide-short arrangement, equally
inert.

**Nothing swipes.** There is no `HorizontalPager` in the app.

**Now Playing covers everything.** Tapping the mini player opens a sheet that
fills the screen: the bottom bar is gone, the mini player is gone, and the only
ways back are a small `⌄` in the top-right corner, a thin drag handle at the top
edge, or the system back gesture. The owner's words: *"wirkt wie nen voll
abgekoppelter Bereich"* — and that is exactly right, because while it is open
the app shows no navigation at all.

## What this changes

**The bottom bar becomes the tab control.** Four destinations, in this order:
**Titel, Interpret, Alben, Favoriten**. The chips at the top go — two controls
for one choice is how they drift apart. The full track list stays as a
destination: it is the only place to find something when the artist does not
come to mind, and it is where search lives.

**The screen swipes between all four**, and the bar follows. Tapping moves the
pager, swiping moves the selection, one source of truth. At the first and last
destination a swipe simply stops — no wrap.

**The chosen destination is remembered across restarts.** Use the durable
preference path the app already has; do not add a database migration for one
enum.

**Now Playing stops above the bar.** The sheet expands to just below the
navigation bar instead of over it, and — this is the part that matters — the bar
stays **interactive**, not merely visible. If the sheet carries a scrim, the
scrim must not cover the bar. Leaving Now Playing then means tapping any
destination, the way the owner is used to from Musicolet, and the app never
stops showing where you are.

**The rail gets the same treatment**, so turning the phone does not lose the
destination.

## The traps, named

**Pre-composition.** A pager composes its neighbours. Each destination loads a
library window; a neighbour must not fetch anything until it is actually shown,
or every swipe pays for two screens and the first frame after launch pays for
three. Prove this one — it is the difference between a smooth swipe and a
stuttering one on a real phone.

**Gesture conflicts.** Vertical drags belong to the lists and to the Now Playing
sheet; the pager must not swallow them, and it must not steal a horizontal
gesture from anything inside a destination that scrolls sideways.

**One selected destination.** Today every item claims `selected = true`, which is
also wrong out loud: TalkBack announces every destination as selected. After
this, exactly one is.

## Proof

Compose tests, each red before the change:

1. Tapping a destination shows that destination's content and leaves exactly one
   selected.
2. Swiping left moves to the next destination and the bar's selection follows.
3. A swipe at the last destination stays there.
4. The destination chosen before an `ActivityScenario.recreate` is the one shown
   after it — the project already uses recreate for exactly this kind of claim.
5. A destination that was never shown has requested no library window.
6. With Now Playing expanded, the navigation bar is still displayed **and still
   receives a tap** — and that tap closes Now Playing by moving to that
   destination.

Name the production line you reverted and the test that went red for each.

## Out of scope

What the four destinations contain. Nothing about the content of a tab changes
here — only which one is showing, how you get there, that the bar tells the
truth, and that Now Playing no longer hides it.
