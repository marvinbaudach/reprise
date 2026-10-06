package io.github.marvinbaudach.reprise

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.DrawScope
import io.github.marvinbaudach.reprise.scene.SceneState

/**
 * The one stationary fog canvas behind every panel.
 *
 * The fog used to live inside each panel's own canvas, which rides the swipe
 * -- translated, scaled, faded with its neighbour -- so cutting the film
 * there on a track change cut it exactly where the swipe would otherwise
 * carry it (spec §1). This layer draws once, behind the panels, at a centre
 * that never moves with them. It knows nothing about panels at all: it reads
 * only what the live one publishes through [LiveSceneHandle], and crossfades
 * between one cover's light and the next on its own clock.
 */
@Composable
internal fun NowPlayingFogLayer(
    liveScene: LiveSceneHandle,
    motion: AmbientMotionController,
    visualizerLight: Float,
    modifier: Modifier = Modifier,
) {
    val arrival = remember { Animatable(1f) }
    var handover by remember { mutableStateOf(FogHandover.EMPTY) }
    val clocks = remember { FogClocks() }
    val animationsEnabled = motion.sceneAnimationsEnabled

    // Keyed on the fog object itself, not a track id: a cover that finishes
    // preparing late, after the scene has already moved on, must still hand
    // over the moment it lands. Keying on the animations toggle too means a
    // runtime gate flip snaps a fade already in flight, the same way the
    // panels' own crossfades do.
    LaunchedEffect(liveScene.fog, animationsEnabled) {
        val next = fogHandover(handover, liveScene.fog, arrival.value)
        if (next === handover) {
            // Nothing changed about the fog itself, but the gate may have:
            // a resting fade (already at 1) needs no snap, one in flight does.
            if (!animationsEnabled) arrival.snapTo(1f)
            return@LaunchedEffect
        }
        handover = next
        if (!animationsEnabled || next.outgoing == null) {
            arrival.snapTo(1f)
        } else {
            arrival.snapTo(0f)
            arrival.animateTo(1f, tween(FOG_CROSSFADE_MS, easing = LinearEasing))
        }
    }

    Canvas(modifier.fillMaxSize()) {
        observeSceneFrame(liveScene.drawRevision)
        // Read directly, not through a local val the draw lambda would close
        // over stale: a DrawScope lambda re-reads whatever state it touches
        // on its own, and Animatable's `.value` is exactly such a state read
        // -- it is what invalidates this draw while the fade is in flight.
        val state = liveScene.state ?: return@Canvas
        clocks.adoptIfNewState(state)
        drawNowPlayingFogLayer(
            handover = handover,
            arrival = arrival.value,
            state = state,
            clocks = clocks.offsets,
            visualizerLight = visualizerLight,
            center = Offset(size.width / 2f, size.height * PLAYED_CENTRE_FRACTION),
            rotationsEnabled = motion.sceneRenderPower().fogRotates,
        )
        clocks.rememberShown(state)
    }
}

/**
 * Carries [FogClockOffsets] across a handover to a different panel's
 * [SceneState], and remembers what the layer last actually drew so the next
 * handover's offset can be computed from it.
 *
 * A plain mutable holder, in the same register as [FrozenSceneBytes]: it is
 * read and written on the main thread only, from inside one draw lambda, so
 * it needs no snapshot state of its own -- only [LiveSceneHandle]'s fields
 * and the [Animatable] above have to survive recomposition and invalidate a
 * redraw, and they already do.
 */
private class FogClocks {
    var offsets: FogClockOffsets = FogClockOffsets(0f, 0.0)
        private set
    private var lastShownFilmSeconds: Float = 0f
    private var lastShownShimmerSeconds: Double = 0.0
    private var computedFor: SceneState? = null

    /**
     * Recomputes [offsets] the moment [state] is a different [SceneState]
     * than the one they were last computed for -- the first frame after a
     * handover to a new panel, and no other. Must run before the draw that
     * reads [offsets], or that frame reads the outgoing panel's raw seconds
     * and jumps.
     */
    fun adoptIfNewState(state: SceneState) {
        if (computedFor === state) return
        val previous = computedFor
        offsets = if (previous == null) {
            FogClockOffsets(0f, 0.0)
        } else {
            continuedFogClocks(
                lastShownFilmSeconds,
                lastShownShimmerSeconds,
                state.oilFilmSeconds,
                state.shimmerElapsedSeconds,
            )
        }
        computedFor = state
    }

    /** What this draw actually showed, so the next handover can continue from it. */
    fun rememberShown(state: SceneState) {
        lastShownFilmSeconds = shownFilmSeconds(state, offsets)
        lastShownShimmerSeconds = shownShimmerSeconds(state, offsets)
    }
}

private fun shownFilmSeconds(state: SceneState, offsets: FogClockOffsets): Float =
    state.oilFilmSeconds + offsets.filmSeconds

private fun shownShimmerSeconds(state: SceneState, offsets: FogClockOffsets): Double =
    (state.shimmerElapsedSeconds + offsets.shimmerSeconds) % SHIMMER_TURN_SECONDS

/**
 * The pure draw behind [NowPlayingFogLayer], kept apart from it so the
 * rendered-pixel tests can call it directly without a Compose harness --
 * exactly the role `drawPlayedCover` already plays for the panel's own cover canvas.
 */
internal fun DrawScope.drawNowPlayingFogLayer(
    handover: FogHandover,
    arrival: Float,
    state: SceneState,
    clocks: FogClockOffsets,
    visualizerLight: Float,
    center: Offset,
    rotationsEnabled: Boolean,
) {
    drawNowPlayingFog(
        // Behind the spectrum there is no artwork to read a palette from, so
        // the film borrows the ramp the bars themselves are drawn from and
        // follows the cross-fade across to it -- on the same slow clock the
        // crossfade itself now runs on, not the bars' fast one (see
        // NowPlayingSheet's visualizerLight).
        palette = handover.paletteAt(arrival)?.blendedTo(VisualizerRampPalette, visualizerLight),
        center = center,
        seconds = shownFilmSeconds(state, clocks),
        level = state.oilFilmLevel,
        opacity = 1f,
        driftEnabled = rotationsEnabled,
    )
    val shimmerSeconds = shownShimmerSeconds(state, clocks)
    drawFogDisc(
        fog = handover.outgoing,
        center = center,
        elapsedSeconds = shimmerSeconds,
        swell = state.fogLevel,
        alpha = handover.outgoingDiscAlpha(arrival),
        rotationsEnabled = rotationsEnabled,
    )
    drawFogDisc(
        fog = handover.incoming,
        center = center,
        elapsedSeconds = shimmerSeconds,
        swell = state.fogLevel,
        alpha = handover.incomingDiscAlpha(arrival),
        rotationsEnabled = rotationsEnabled,
    )
}

/** One of the two discs [drawNowPlayingFogLayer] can draw; skipped outright at zero alpha. */
private fun DrawScope.drawFogDisc(
    fog: CoverFogBitmap?,
    center: Offset,
    elapsedSeconds: Double,
    swell: Float,
    alpha: Float,
    rotationsEnabled: Boolean,
) {
    if (fog == null || alpha <= 0f) return
    drawNowPlayingShimmer(
        fog = fog,
        center = center,
        coverDiameterDp = COVER_SIZE_DP.toFloat(),
        elapsedSeconds = elapsedSeconds,
        swell = swell,
        opacity = alpha,
        rotationsEnabled = rotationsEnabled,
    )
}
