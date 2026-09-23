package io.github.marvinbaudach.reprise

/**
 * What the shared fog layer draws while one cover's light gives way to the
 * next.
 *
 * At rest this is a plain pair: [outgoing] is `null`, [incoming] is the live
 * fog, [outgoingAlpha] is `1f` because there is nothing left to fade. Only
 * [fogHandover] ever produces a new instance — nothing here mutates, and the
 * layer that reads it never decides anything for itself.
 */
internal data class FogHandover(
    val outgoing: CoverFogBitmap?,
    val outgoingPalette: OilFilmPalette?,
    val outgoingAlpha: Float,
    val incoming: CoverFogBitmap?,
) {
    /**
     * The film mixture on screen once the fade has reached [t] of its way in.
     *
     * Before the very first fog neither side exists yet; after a restart the
     * outgoing side can be `null` too (a handover straight out of that first,
     * empty state). Both cases are just "whichever side exists" — there is
     * nothing to blend towards or away from.
     */
    fun paletteAt(t: Float): OilFilmPalette? {
        val incomingPalette = incoming?.palette
        return when {
            outgoingPalette != null && incomingPalette != null ->
                outgoingPalette.blendedTo(incomingPalette, t)
            incomingPalette != null -> incomingPalette
            else -> outgoingPalette
        }
    }

    /** The outgoing disc's opacity at [t]; `0f` once there is nothing outgoing left to draw. */
    fun outgoingDiscAlpha(t: Float): Float =
        if (outgoing == null) 0f else outgoingAlpha * (1f - t.coerceIn(0f, 1f))

    /**
     * The incoming disc's opacity at [t] — except at the very first fog, which has no outgoing
     * disc to fade from underneath and so is drawn at full strength from the first frame.
     */
    fun incomingDiscAlpha(t: Float): Float =
        if (outgoing == null) 1f else t.coerceIn(0f, 1f)

    internal companion object {
        val EMPTY =
            FogHandover(outgoing = null, outgoingPalette = null, outgoingAlpha = 1f, incoming = null)
    }
}

/**
 * Folds one live-fog change into [previous], or hands it back unchanged.
 *
 * The rule fires on the live fog *object* changing, not on a track id
 * changing — so it also fires when a cover finishes preparing late, after the
 * scene already moved on. [arrival] is the caller's own fade clock, read
 * once, at the moment the object changed; `fogHandover` never advances a
 * clock itself.
 *
 * Three cases:
 * - the same fog object again → [previous] itself, so a caller can tell
 *   nothing happened without comparing fields;
 * - no live fog yet → [previous] itself, so a newly live panel cannot erase
 *   the handover while its own fog is still preparing;
 * - no fog has ever landed yet → the new fog becomes [FogHandover.incoming]
 *   outright, nothing outgoing, nothing to fade;
 * - a change while [previous] was already at rest (`arrival >= 1`) → a plain
 *   handover: the old incoming becomes outgoing at full alpha;
 * - a change mid-fade (`arrival < 1`) → a restart. The mixture the screen was
 *   already showing at `t` becomes the new outgoing palette, so the fade
 *   continues from what the eye actually saw rather than jumping back to the
 *   old incoming's pure colour; the disc that was fading in becomes the disc
 *   fading out, at the alpha it had already reached. The older outgoing disc
 *   is dropped — the layer never carries three.
 */
internal fun fogHandover(
    previous: FogHandover,
    liveFog: CoverFogBitmap?,
    arrival: Float,
): FogHandover {
    if (liveFog === previous.incoming) return previous
    if (liveFog == null) return previous
    val previousIncoming = previous.incoming ?: return FogHandover(
        outgoing = null,
        outgoingPalette = null,
        outgoingAlpha = 1f,
        incoming = liveFog,
    )
    val t = arrival.coerceIn(0f, 1f)
    return if (t >= 1f) {
        FogHandover(
            outgoing = previousIncoming,
            outgoingPalette = previousIncoming.palette,
            outgoingAlpha = 1f,
            incoming = liveFog,
        )
    } else {
        FogHandover(
            outgoing = previousIncoming,
            outgoingPalette = (previous.outgoingPalette ?: previousIncoming.palette)
                .blendedTo(previousIncoming.palette, t),
            outgoingAlpha = t,
            incoming = liveFog,
        )
    }
}

/** How far [continuedFogClocks] has to carry a clock forward before it reads the same as before. */
internal data class FogClockOffsets(val filmSeconds: Float, val shimmerSeconds: Double)

/**
 * The offset each clock needs so the frame right after a handover reads
 * exactly as it did the frame before.
 *
 * `oilFilmSeconds` and `shimmerElapsedSeconds` live on the per-panel
 * `SceneState`, so a handover to a new panel hands the layer a clock that
 * restarted at its own zero. Reading it unadjusted would jump the film and
 * the disc's rotation by however far the two panels' clocks disagree — the
 * very cut this design exists to remove. Carrying an offset instead means the
 * layer keeps drawing continuously through the handover and only drifts away
 * from the old panel's timing as slowly as the clock itself moves.
 *
 * The shimmer offset is wrapped into `[0, SHIMMER_TURN_SECONDS)`: the turn
 * has no meaningful sign or magnitude past one lap, and an unwrapped offset
 * would still read correctly today but drift towards a `Double` no longer
 * precise enough to matter after enough handovers in one session.
 */
internal fun continuedFogClocks(
    shownFilmSeconds: Float,
    shownShimmerSeconds: Double,
    newFilmSeconds: Float,
    newShimmerSeconds: Double,
): FogClockOffsets {
    val filmSeconds = shownFilmSeconds - newFilmSeconds
    val shimmerSeconds =
        ((shownShimmerSeconds - newShimmerSeconds) % SHIMMER_TURN_SECONDS + SHIMMER_TURN_SECONDS) %
            SHIMMER_TURN_SECONDS
    return FogClockOffsets(filmSeconds, shimmerSeconds)
}

/**
 * How long the fog layer's crossfade takes, linear.
 *
 * Linear on purpose, carried over verbatim from the desktop's own reasoning
 * (`crates/reprise-gnome/src/ui/now_playing/cover_cloud.rs`, `cover_fade`):
 * the outgoing and incoming discs are painted one over the other, so an eased
 * pair would both be part-way out at the midpoint and the light would dip
 * there — a crossfade wants its two halves to sum to 1, not to ease.
 */
internal const val FOG_CROSSFADE_MS = 1000

/**
 * How long one shimmer rotation takes, mirrored from the private constant of
 * the same value on `scene.SceneState` — that one drives the clock this
 * measures against, but it lives across a package boundary this file cannot
 * reach into, the same way `NowPlayingShimmer.kt`'s own `TURN_SECONDS`
 * already does.
 */
internal const val SHIMMER_TURN_SECONDS = 60.0
