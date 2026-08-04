# Repeat-sign proportions

The Reprise mark remains a musical repeat sign: two dots, a thin barline and a
thick barline. Its 96-unit geometry is fixed at circles `(30,39,5.5)` and
`(30,57,5.5)`, then rectangles `(41,20,5,56,1)` and `(52,20,15,56,1.5)`.
The 1:3 barline ratio prevents the mark reading as a rest, while the 5.5-unit
dot-to-barline gap binds the four shapes without collapsing them.

`data/brand/palette.toml` is the only maintained colour source. The mark puts
`#FF6F5E` on the three small elements — the two dots and the thin barline — and
`#4FDBD4` on the thick barline. The light-ground form replaces them with
`#C4362A` and `#00706B`. The carrier is a solid `#33363F` rounded 88-unit
square. Nothing uses a gradient.

The split was chosen against the alternative that swapped the two colours,
compared at dock size. Teal on the thick barline reads farther because the
largest field carries the brighter colour: 7.13:1 against the carrier where
coral manages 4.42:1. Coral still holds as the accent on the small elements.
Only the chosen split is generated — keeping the loser "for comparison" would
mean a second mark to hold in step, and the comparison is over.

Coral is deliberate rather than saturated red. `#EF4444` beside teal is
reserved for interface error states and its lower lightness would outweigh the
teal. Coral separates just as clearly without that connotation and sits closer
to the teal in lightness. Any future adjustment stays within
`#FF6F5E` … `#FF8A72`; it never moves toward red.

At 512 px the rendered ink box is x 24.562–66.938 and y 20.062–75.938 viewBox
units. Four components survive at 22, 24, 28 and 32 px. At 16 px the counter
reports two — but nothing has merged there. Measuring the raw pixel groups
shows all four still separate, at 20, 8, 2 and 2 pixels; the two dots simply
fall under the detector's 3 px noise floor and stop being counted. The
distinction matters because the two failure modes call for opposite fixes:
strokes running together would mean the geometry is too tight, whereas strokes
surviving but uncountable means the sign is below its useful size. The report
derives its wording from the group sizes so it cannot assert a cause nobody
measured.

The 96-unit geometry is therefore not used at 16 px. That stage ships from
`reprise-mark-16.svg`, the same sign redrawn on the 16-unit grid with every
edge on a whole raster line: dots 3x3 at (3,5) and (3,9), barlines 1 and 3
units wide at x 7 and x 9, all spanning y 3–12, on a 14-unit carrier inset by
one. It renders four groups of 30, 10, 9 and 9 pixels — every element at least
three times the noise floor, and the 1:3 ratio still legible.

The dots are 3x3 rather than 2x2 deliberately. At 2x2 they measure 4 pixels
against a 3-pixel floor: countable, but one rounding decision away from
vanishing. Ink runs to 64% of the carrier's width against 48% in the 96-unit
drawing, because thin features disappear at this size.

Everything from 22 px up still comes from the 96-unit drawing. The gate asserts
that the two sources render differently at 16 px, so a wiring mistake that
silently fell back to downscaling cannot pass.

The symbolic SVG keeps the 96-unit geometry: it is rendered at whatever size
the shell asks for, and grid-fitting it to 16 would misfit every other size.
A themed 16 px surface can use `reprise-mark-16-mono.svg`.

Measured WCAG contrasts are 4.42 (`#FF6F5E`) and 7.13 (`#4FDBD4`) on the
carrier, 7.23 and 11.68 on `#0a0a0e`, 5.37 (`#C4362A`) and 5.95 (`#00706B`)
on white, and 4.64 and 5.13 on `#eceef5`. All exceed the 3:1 graphical-object
floor. The fixed Android `translate(6,6)` placement loses 0.000436 of rendered
ink under the centred 66-dp circle and keeps all four components.

The gate therefore uses exact 512 px ink bounds instead of edge fill, gates
four components at 28 px while reporting the other shipped stages, checks
contrast and silhouette, and measures clipped ink rather than a weaker corner
radius. GTK has no loaded project CSS or GResource icon tree, so
no unused GTK colour-token layer is introduced; the hicolor symbolic asset is
the named theme-coloured UI resource.
