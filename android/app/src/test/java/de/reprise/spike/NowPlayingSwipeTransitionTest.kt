package de.reprise.spike

import android.graphics.Bitmap
import android.graphics.Color
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import de.reprise.spike.ui.theme.RepriseTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidPlaybackState

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36], qualifiers = "w500dp-h1000dp")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NowPlayingSwipeTransitionTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun nextSwipeKeepsTheWholeOutgoingCardTogetherUntilItIsOffscreen() {
        compose.mainClock.autoAdvance = false
        val outgoing = swipeTrack(id = 830, uri = "content://tracks/outgoing")
        val incoming = swipeTrack(id = 831, uri = "content://tracks/incoming")
        val liveTrack = mutableStateOf(outgoing)
        val controls = SwipeRecordingControls()
        val engineFactory = SwipeRecordingEngineFactory()
        val motion = AmbientMotionController()
        val artwork = swipeArtwork(outgoing, incoming)

        try {
            compose.setContent {
                val track = liveTrack.value
                RepriseTheme(swipeTheme(), darkPalette = true) {
                    CompositionLocalProvider(
                        LocalPlaybackControls provides controls,
                        LocalTrackArtwork provides artwork,
                        LocalVisualSceneEngineFactory provides engineFactory,
                        LocalAmbientMotionController provides motion,
                    ) {
                        NowPlayingSheet(
                            track = track,
                            playback = swipePlayback(track),
                            close = {},
                        )
                    }
                }
            }
            compose.runOnUiThread {
                motion.attach()
                motion.runtimeChanged(
                    resumed = true,
                    screenInteractive = true,
                    animationsEnabled = true,
                )
            }
            compose.mainClock.advanceTimeByFrame()
            compose.mainClock.advanceTimeByFrame()
            val baselineFogBlue = captureScene().blueMassOutsideCover()

            compose.onNodeWithTag("now-playing-gestures").performTouchInput {
                down(Offset(width * 0.75f, height * 0.3f))
                moveTo(Offset(width * 0.35f, height * 0.3f))
                up()
            }

            var sawExitAfterTransport = false
            var incomingCoverWasDisplaced = false
            var incomingFogChangedWhileDisplaced = false
            var visualizerResetWhileDisplaced = false
            var incomingArrived = false
            var framesSinceTransport = 0
            var outgoingWasDisplaced = false
            var outgoingReturnedToCentre = false
            repeat(MAX_TRANSITION_FRAMES) {
                compose.mainClock.advanceTimeByFrame()
                if (controls.nextCalls > 0) framesSinceTransport += 1
                if (!incomingArrived && framesSinceTransport == FAKE_FLIP_DELAY_FRAMES) {
                    compose.runOnUiThread { liveTrack.value = incoming }
                    incomingArrived = true
                }

                val frame = captureScene()
                val cover = frame.strongCoverRun()
                val displaced = cover == null ||
                    kotlin.math.abs(cover.center - frame.width / 2f) > 2f
                if (controls.nextCalls > 0 && displaced) sawExitAfterTransport = true
                if (displaced) {
                    outgoingWasDisplaced = outgoingWasDisplaced || cover?.incoming == false
                    incomingCoverWasDisplaced = incomingCoverWasDisplaced || cover?.incoming == true
                    incomingFogChangedWhileDisplaced = incomingFogChangedWhileDisplaced ||
                        frame.blueMassOutsideCover() > baselineFogBlue + FOG_BLUE_TOLERANCE
                    visualizerResetWhileDisplaced = visualizerResetWhileDisplaced ||
                        engineFactory.engine.trackChanges > 1
                }
                if (
                    controls.nextCalls > 0 &&
                    !incomingArrived &&
                    outgoingWasDisplaced &&
                    cover?.incoming == false &&
                    kotlin.math.abs(cover.center - frame.width / 2f) <= 2f
                ) {
                    outgoingReturnedToCentre = true
                }
                if (
                    incomingArrived &&
                    cover?.incoming == true &&
                    kotlin.math.abs(cover.center - frame.width / 2f) <= 2f &&
                    engineFactory.engine.trackChanges == 2
                ) {
                    return@repeat
                }
            }

            // The fake chooses its own flip timing, so this proves the invariant regardless of
            // when that flip lands. It is a regression pin, not a reproduction of device latency.
            assertTrue("the card never began its exit after transport advanced", sawExitAfterTransport)
            assertTrue("the fake incoming track never arrived", incomingArrived)
            assertFalse("the incoming cover appeared on a displaced card", incomingCoverWasDisplaced)
            assertFalse("the incoming fog appeared on a displaced card", incomingFogChangedWhileDisplaced)
            assertFalse("the discarded outgoing card returned to centre", outgoingReturnedToCentre)
            assertFalse(
                "the visualizer reset to the incoming track on a displaced card",
                visualizerResetWhileDisplaced,
            )
            assertEquals(1, controls.nextCalls)
            assertEquals(2, engineFactory.engine.trackChanges)
        } finally {
            motion.detach()
            artwork.shutdown()
        }
    }

    @Test
    fun aCommittedSwipeThatNeverReceivesANewTrackReturnsToCentre() {
        compose.mainClock.autoAdvance = false
        val outgoing = swipeTrack(id = 830, uri = "content://tracks/outgoing")
        val incoming = swipeTrack(id = 831, uri = "content://tracks/incoming")
        val controls = SwipeRecordingControls()
        val engineFactory = SwipeRecordingEngineFactory()
        val motion = AmbientMotionController()
        val artwork = swipeArtwork(outgoing, incoming)

        try {
            compose.setContent {
                RepriseTheme(swipeTheme(), darkPalette = true) {
                    CompositionLocalProvider(
                        LocalPlaybackControls provides controls,
                        LocalTrackArtwork provides artwork,
                        LocalVisualSceneEngineFactory provides engineFactory,
                        LocalAmbientMotionController provides motion,
                    ) {
                        NowPlayingSheet(
                            track = outgoing,
                            playback = swipePlayback(outgoing),
                            close = {},
                        )
                    }
                }
            }
            enableMotion(motion)
            compose.mainClock.advanceTimeByFrame()
            compose.mainClock.advanceTimeByFrame()

            swipeNext()
            compose.mainClock.advanceTimeBy(EXIT_SETTLE_MS)

            val frame = captureScene()
            val cover = frame.strongCoverRun()
            assertEquals(1, controls.nextCalls)
            assertTrue("the outgoing cover stayed offscreen", cover?.incoming == false)
            assertEquals(frame.width / 2f, cover?.center ?: -1f, 2f)
        } finally {
            motion.detach()
            artwork.shutdown()
        }
    }

    @Test
    fun aRapidSecondSwipeCannotReplaceTheInFlightExit() {
        compose.mainClock.autoAdvance = false
        val outgoing = swipeTrack(id = 830, uri = "content://tracks/outgoing")
        val incoming = swipeTrack(id = 831, uri = "content://tracks/incoming")
        val controls = SwipeRecordingControls()
        val engineFactory = SwipeRecordingEngineFactory()
        val motion = AmbientMotionController()
        val artwork = swipeArtwork(outgoing, incoming)

        try {
            compose.setContent {
                RepriseTheme(swipeTheme(), darkPalette = true) {
                    CompositionLocalProvider(
                        LocalPlaybackControls provides controls,
                        LocalTrackArtwork provides artwork,
                        LocalVisualSceneEngineFactory provides engineFactory,
                        LocalAmbientMotionController provides motion,
                    ) {
                        NowPlayingSheet(
                            track = outgoing,
                            playback = swipePlayback(outgoing),
                            close = {},
                        )
                    }
                }
            }
            enableMotion(motion)
            compose.mainClock.advanceTimeByFrame()
            compose.mainClock.advanceTimeByFrame()

            swipeNext()
            compose.mainClock.advanceTimeByFrame()
            compose.onNodeWithTag("now-playing-gestures").performTouchInput {
                down(Offset(width * 0.25f, height * 0.3f))
                moveTo(Offset(width * 0.65f, height * 0.3f))
                up()
            }
            compose.mainClock.advanceTimeBy(EXIT_SETTLE_MS)

            assertEquals(1, controls.nextCalls)
            assertEquals(0, controls.previousCalls)
        } finally {
            motion.detach()
            artwork.shutdown()
        }
    }

    @Test
    fun leavingCompositionDuringTheExitCannotLoseTheTransportCommand() {
        compose.mainClock.autoAdvance = false
        val mounted = mutableStateOf(true)
        val outgoing = swipeTrack(id = 830, uri = "content://tracks/outgoing")
        val controls = SwipeRecordingControls()
        val engineFactory = SwipeRecordingEngineFactory()
        val motion = AmbientMotionController()

        try {
            compose.setContent {
                if (mounted.value) {
                    RepriseTheme(swipeTheme(), darkPalette = true) {
                        CompositionLocalProvider(
                            LocalPlaybackControls provides controls,
                            LocalVisualSceneEngineFactory provides engineFactory,
                            LocalAmbientMotionController provides motion,
                        ) {
                            NowPlayingSheet(
                                track = outgoing,
                                playback = swipePlayback(outgoing),
                                close = {},
                            )
                        }
                    }
                }
            }
            enableMotion(motion)
            compose.mainClock.advanceTimeByFrame()
            compose.mainClock.advanceTimeByFrame()

            swipeNext()
            compose.runOnUiThread { mounted.value = false }
            compose.mainClock.advanceTimeByFrame()

            assertEquals(1, controls.nextCalls)
        } finally {
            motion.detach()
        }
    }

    @Test
    fun reducedMotionAdvancesImmediatelyBecauseThereIsNoExitWindow() {
        val controls = SwipeRecordingControls()
        val engineFactory = SwipeRecordingEngineFactory()
        val motion = AmbientMotionController()
        try {
            compose.setContent {
                RepriseTheme(swipeTheme(), darkPalette = true) {
                    CompositionLocalProvider(
                        LocalPlaybackControls provides controls,
                        LocalVisualSceneEngineFactory provides engineFactory,
                        LocalAmbientMotionController provides motion,
                    ) {
                        NowPlayingSheet(
                            track = swipeTrack(830, "content://tracks/outgoing"),
                            playback = swipePlayback(swipeTrack(830, "content://tracks/outgoing")),
                            close = {},
                        )
                    }
                }
            }
            compose.runOnUiThread {
                motion.attach()
                motion.runtimeChanged(
                    resumed = true,
                    screenInteractive = true,
                    animationsEnabled = false,
                )
            }
            compose.waitForIdle()
            compose.mainClock.autoAdvance = false

            compose.onNodeWithTag("now-playing-gestures").performTouchInput {
                down(Offset(width * 0.75f, height * 0.3f))
                moveTo(Offset(width * 0.35f, height * 0.3f))
                up()
            }

            assertEquals(1, controls.nextCalls)
        } finally {
            motion.detach()
        }
    }

    private fun captureScene(): Bitmap = compose.onNodeWithTag("now-playing-scene")
        .captureToImage()
        .asAndroidBitmap()

    private fun enableMotion(motion: AmbientMotionController) {
        compose.runOnUiThread {
            motion.attach()
            motion.runtimeChanged(
                resumed = true,
                screenInteractive = true,
                animationsEnabled = true,
            )
        }
    }

    private fun swipeNext() {
        compose.onNodeWithTag("now-playing-gestures").performTouchInput {
            down(Offset(width * 0.75f, height * 0.3f))
            moveTo(Offset(width * 0.35f, height * 0.3f))
            up()
        }
    }

    private fun swipeArtwork(outgoing: LibraryTrack, incoming: LibraryTrack): TrackArtwork {
        val cache = ArtworkCache()
        listOf(outgoing to Color.RED, incoming to Color.BLUE).forEach { (track, colour) ->
            val bitmap = Bitmap.createBitmap(32, 32, Bitmap.Config.ARGB_8888).apply {
                eraseColor(colour)
            }
            val visual = ArtworkVisual(bitmap.asImageBitmap(), ambientColors = null)
            cache.putArtwork(track.artworkRequest(), visual)
            SharedArtworkCache.putFog(
                visual.image,
                prepareCoverFogBitmap(bitmap, Color.BLACK),
            )
        }
        return TrackArtwork(
            resolve = { _, _ -> error("the seeded swipe artwork must not resolve") },
            cache = cache,
        )
    }

    private fun LibraryTrack.artworkRequest() = ArtworkRequest(
        trackUri = uri,
        size = AndroidArtworkSize.NOW_PLAYING,
        title = title,
        artist = artist,
    )

    private fun Bitmap.strongCoverRun(): CoverRun? {
        val y = (height * PLAYED_CENTRE_FRACTION).toInt()
        var best: CoverRun? = null
        var start = -1
        var incoming = false
        for (x in 0..width) {
            val colour = if (x < width) getPixel(x, y) else Color.BLACK
            val isOutgoing = Color.red(colour) >= STRONG_CHANNEL &&
                Color.green(colour) <= WEAK_CHANNEL && Color.blue(colour) <= WEAK_CHANNEL
            val isIncoming = Color.blue(colour) >= STRONG_CHANNEL &&
                Color.red(colour) <= WEAK_CHANNEL && Color.green(colour) <= WEAK_CHANNEL
            if (isOutgoing || isIncoming) {
                if (start < 0) {
                    start = x
                    incoming = isIncoming
                }
            } else if (start >= 0) {
                val run = CoverRun(start, x - 1, incoming)
                if (run.length > (best?.length ?: 0)) best = run
                start = -1
            }
        }
        return best?.takeIf { it.length >= MIN_COVER_RUN }
    }

    private fun Bitmap.blueMassOutsideCover(): Long {
        val coverTop = (height * PLAYED_CENTRE_FRACTION - 170f).toInt()
        val coverBottom = (height * PLAYED_CENTRE_FRACTION + 170f).toInt()
        var mass = 0L
        for (y in 0 until height) {
            if (y in coverTop..coverBottom) continue
            for (x in 0 until width) {
                val pixel = getPixel(x, y)
                mass += (Color.blue(pixel) - maxOf(Color.red(pixel), Color.green(pixel)))
                    .coerceAtLeast(0)
            }
        }
        return mass
    }
}

private data class CoverRun(val left: Int, val right: Int, val incoming: Boolean) {
    val length: Int get() = right - left + 1
    val center: Float get() = (left + right) / 2f
}

private class SwipeRecordingControls : PlaybackControls by DisconnectedPlaybackControls {
    var nextCalls = 0
        private set
    var previousCalls = 0
        private set

    override fun next() {
        nextCalls += 1
    }

    override fun previous() {
        previousCalls += 1
    }
}

private class SwipeRecordingEngineFactory : VisualSceneEngineFactory {
    val engine = SwipeRecordingEngine()
    override fun create(): VisualSceneEngine = engine
}

private class SwipeRecordingEngine : VisualSceneEngine {
    var trackChanges = 0
        private set

    override fun setAccent(red: Float, green: Float, blue: Float) = Unit
    override fun setPlaying(playing: Boolean) = Unit
    override fun noteTrackChanged() {
        trackChanges += 1
    }
    override fun ingestBands(bands: FloatArray) = Unit
    override fun tick() = Unit
    override fun close() = Unit
}

private fun swipeTheme() = MobileThemeSelection(
    palette = MobileTheme.NOCTURNE,
    colorScheme = AndroidColorScheme.SYSTEM,
    dynamicAvailable = false,
)

private fun swipeTrack(id: Long, uri: String) = LibraryTrack(
    id = id,
    uri = uri,
    title = "Song",
    artist = "Artist",
    album = "Album",
    durationMs = 100_000,
    playCount = 0,
    rating = 0,
)

private fun swipePlayback(track: LibraryTrack) = PlaybackUiState(
    ready = true,
    state = AndroidPlaybackState.PAUSED,
    currentIndex = 0,
    currentTrackId = track.id,
    currentTrackUri = track.uri,
    positionMs = 20_000,
    durationMs = 100_000,
)

private const val MAX_TRANSITION_FRAMES = 90
private const val FAKE_FLIP_DELAY_FRAMES = 5
private const val EXIT_SETTLE_MS = 2_000L
private const val PLAYED_CENTRE_FRACTION = 0.34f
private const val FOG_BLUE_TOLERANCE = 2_000L
private const val STRONG_CHANNEL = 180
private const val WEAK_CHANNEL = 60
private const val MIN_COVER_RUN = 40
