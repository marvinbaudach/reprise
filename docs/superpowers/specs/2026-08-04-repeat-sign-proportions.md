# Repeat-sign proportions

The Reprise mark remains a musical repeat sign: two dots, a thin barline and a
thick barline. Its 96-unit geometry is fixed at circles `(30,39,5.5)` and
`(30,57,5.5)`, then rectangles `(41,20,5,56,1)` and `(52,20,15,56,1.5)`.
The 1:3 barline ratio prevents the mark reading as a rest, while the 5.5-unit
dot-to-barline gap binds the four shapes without collapsing them.

`data/brand/palette.toml` is the only maintained colour source. Variant A uses
`#A855F7` on the three small elements and `#4FDBD4` on the thick barline;
variant B reverses those roles. Light-ground forms replace them with `#7C3AED`
and `#00706B`. The carrier is a solid `#33363F` rounded 88-unit square. No mark
or carrier uses a gradient.

At 512 px the rendered ink box is x 24.562–66.938 and y 20.062–75.938 viewBox
units. Both variants retain four components at 22, 24, 28 and 32 px. At 16 px
they retain two: both dots join the thin barline, while the thick barline stays
separate. This is reported rather than hidden by changing the geometry. If a
distinct 16 px mark becomes necessary, review a dedicated hinted symbolic
drawing; this specification does not authorise one.

Measured WCAG contrasts are 3.05 (`#A855F7`) and 7.13 (`#4FDBD4`) on the
carrier, 4.99 and 11.68 on `#0a0a0e`, 5.70 (`#7C3AED`) and 5.95 (`#00706B`)
on white, and 4.92 and 5.13 on `#eceef5`. All exceed the 3:1 graphical-object
floor. The fixed Android `translate(6,6)` placement loses 0.000436 of rendered
ink under the centred 66-dp circle and keeps all four components.

The gate therefore uses exact 512 px ink bounds instead of edge fill, gates
four components at 28 px while reporting the other shipped stages, checks both
variants' contrast and silhouette, and measures clipped ink rather than a
weaker corner radius. GTK has no loaded project CSS or GResource icon tree, so
no unused GTK colour-token layer is introduced; the hicolor symbolic asset is
the named theme-coloured UI resource.
