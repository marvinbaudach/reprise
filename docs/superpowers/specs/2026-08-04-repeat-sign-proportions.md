# Repeat-sign proportions

The Reprise mark remains a musical repeat sign: two dots, a thin barline and a
thick barline. Its 96-unit geometry is fixed at circles `(30,39,5.5)` and
`(30,57,5.5)`, then rectangles `(41,20,5,56,1)` and `(52,20,15,56,1.5)`.
The 1:3 barline ratio prevents the mark reading as a rest, while the 5.5-unit
dot-to-barline gap binds the four shapes without collapsing them.

`data/brand/palette.toml` is the only maintained colour source. The mark puts
`#A855F7` on the three small elements — the two dots and the thin barline — and
`#4FDBD4` on the thick barline. The light-ground form replaces them with
`#7C3AED` and `#00706B`. The carrier is a solid `#33363F` rounded 88-unit
square. Nothing uses a gradient.

The split was chosen against the alternative that swapped the two colours,
compared at dock size. Teal on the thick barline reads farther because the
largest field carries the brighter colour: 7.13:1 against the carrier where
violet manages 3.05:1. Violet still holds as the accent on the small elements.
Only the chosen split is generated — keeping the loser "for comparison" would
mean a second mark to hold in step, and the comparison is over.

At 512 px the rendered ink box is x 24.562–66.938 and y 20.062–75.938 viewBox
units. Four components survive at 22, 24, 28 and 32 px. At 16 px the counter
reports two — but nothing has merged there. Measuring the raw pixel groups
shows all four still separate, at 20, 8, 2 and 2 pixels; the two dots simply
fall under the detector's 3 px noise floor and stop being counted. The
distinction matters because the two failure modes call for opposite fixes:
strokes running together would mean the geometry is too tight, whereas strokes
surviving but uncountable means the sign is below its useful size. The report
derives its wording from the group sizes so it cannot assert a cause nobody
measured. This is reported rather than hidden by changing the geometry. If a
distinct 16 px mark becomes necessary, review a dedicated hinted symbolic
drawing; this specification does not authorise one.

Measured WCAG contrasts are 3.05 (`#A855F7`) and 7.13 (`#4FDBD4`) on the
carrier, 4.99 and 11.68 on `#0a0a0e`, 5.70 (`#7C3AED`) and 5.95 (`#00706B`)
on white, and 4.92 and 5.13 on `#eceef5`. All exceed the 3:1 graphical-object
floor. The fixed Android `translate(6,6)` placement loses 0.000436 of rendered
ink under the centred 66-dp circle and keeps all four components.

The gate therefore uses exact 512 px ink bounds instead of edge fill, gates
four components at 28 px while reporting the other shipped stages, checks
contrast and silhouette, and measures clipped ink rather than a weaker corner
radius. GTK has no loaded project CSS or GResource icon tree, so
no unused GTK colour-token layer is introduced; the hicolor symbolic asset is
the named theme-coloured UI resource.
