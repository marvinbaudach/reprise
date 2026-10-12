package io.github.marvinbaudach.reprise

import io.github.marvinbaudach.reprise.scene.SceneState
import io.github.marvinbaudach.reprise.scene.SpectrogramFrames
import java.lang.reflect.Proxy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.reprise_android_ffi.AndroidBassPressure
import uniffi.reprise_android_ffi.AndroidVisualEngineInterface
import uniffi.reprise_android_ffi.AndroidVisualFrame

/** Every call across the FFI boundary costs main-thread time, so the calls themselves are the contract. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class NativeVisualSceneEngineCallsTest {
    private val native = CountingNative()
    private var nowNanos = 0L
    private val engine = NativeVisualSceneEngine(native.proxy) { nowNanos }

    @Test
    fun aLiveFrameCrossesTheBoundaryOnceForTheTickAndBothQueries() {
        native.frame = liveFrame(kick = 0.81f)

        engine.tick()
        val audible = engine.hasLiveAudio()
        val pressure = engine.bassPressure()

        assertEquals(listOf("frame"), native.calls)
        assertTrue(audible)
        assertEquals(0.81f, pressure.kick, 0f)
        assertEquals(0.63f, pressure.pressure, 0f)
    }

    @Test
    fun theNextFramesQueriesAreAnsweredByTheFrameBeforeIt() {
        native.frame = liveFrame(kick = 0.5f)
        engine.tick()
        nowNanos += FRAME_NANOS
        native.frame = liveFrame(kick = 0.9f)

        // SceneDriver reads the pressure before it ticks, so it sees the frame before it.
        assertEquals(0.5f, engine.bassPressure().kick, 0f)
        engine.tick()
        assertEquals(0.9f, engine.bassPressure().kick, 0f)
        assertEquals(listOf("frame", "frame"), native.calls)
    }

    @Test
    fun withoutATickTheQueriesAskTheNativeEngine() {
        native.hasLiveAudio = true
        native.pressure = pressure(kick = 0.3f)

        assertTrue(engine.hasLiveAudio())
        assertEquals(0.3f, engine.bassPressure().kick, 0f)
        assertEquals(listOf("hasLiveAudio", "bassPressure"), native.calls)
    }

    @Test
    fun aFrameAnswerOlderThanAFewFramesIsAskedAgain() {
        native.frame = liveFrame(kick = 0.5f)
        engine.tick()
        nowNanos += STALE_NANOS
        native.hasLiveAudio = false
        native.pressure = pressure(kick = 0f)

        assertFalse(engine.hasLiveAudio())
        assertEquals(0f, engine.bassPressure().kick, 0f)
        assertEquals(listOf("frame", "hasLiveAudio", "bassPressure"), native.calls)
    }

    @Test
    fun settingThePlayingValueItAlreadyHasStaysOnThisSideOfTheBoundary() {
        engine.setPlaying(true)
        engine.setPlaying(true)
        engine.setPlaying(true)
        engine.setPlaying(false)
        engine.setPlaying(false)

        assertEquals(listOf("setPlaying:true", "setPlaying:false"), native.calls)
    }

    @Test
    fun settingTheAccentItAlreadyHasStaysOnThisSideOfTheBoundary() {
        engine.setAccent(0.1f, 0.2f, 0.3f)
        engine.setAccent(0.1f, 0.2f, 0.3f)
        engine.setAccent(0.1f, 0.2f, 0.4f)
        engine.setAccent(0.1f, 0.2f, 0.4f)

        assertEquals(listOf("setAccent:0.1,0.2,0.3", "setAccent:0.1,0.2,0.4"), native.calls)
    }

    @Test
    fun aChangedPlayingValueDropsTheFrameAnswerItInvalidates() {
        native.frame = liveFrame(kick = 0.5f)
        engine.tick()
        native.hasLiveAudio = false

        engine.setPlaying(false)

        assertFalse(engine.hasLiveAudio())
        assertEquals(listOf("frame", "setPlaying:false", "hasLiveAudio"), native.calls)
    }

    @Test
    fun everyStreamBoundaryDropsTheFrameAnswerAndReopensTheSetters() {
        val boundaries = mapOf<String, () -> Unit>(
            "noteTrackChanged" to { engine.noteTrackChanged() },
            "resetAudioStream" to { engine.resetAudioStream() },
            "resetAudioHistory" to { engine.resetAudioHistory() },
            "adoptShape" to { engine.adoptShape(floatArrayOf(0.5f)) },
            "setPlaybackIntent" to { engine.setPlaybackIntent(true) },
        )
        for ((name, boundary) in boundaries) {
            native.calls.clear()
            native.frame = liveFrame(kick = 0.5f)
            native.hasLiveAudio = false
            engine.tick()
            assertTrue(name, engine.hasLiveAudio())

            boundary()

            assertFalse("$name must not leave a pre-boundary answer standing", engine.hasLiveAudio())
        }
    }

    @Test
    fun aBoundaryDuringTheFrameCallLeavesNoPreBoundaryAnswerStanding() {
        // The audio thread resets the stream while the UI thread is inside native.frame().
        native.frame = liveFrame(kick = 0.5f)
        native.hasLiveAudio = false
        native.onFrame = { engine.resetAudioStream() }

        engine.tick()

        assertFalse(engine.hasLiveAudio())
        assertEquals(0f, engine.bassPressure().kick, 0f)
    }

    @Test
    fun aStreamBoundaryMakesTheNextPlayingValueCrossEvenWhenUnchanged() {
        engine.setPlaying(true)

        engine.noteTrackChanged()
        engine.setPlaying(true)

        assertEquals(
            listOf("setPlaying:true", "noteTrackChanged", "setPlaying:true"),
            native.calls,
        )
    }

    @Test
    fun aLiveSceneFrameCostsExactlyOneCrossingThroughTheDriver() {
        native.frame = liveFrame(kick = 0.81f)
        val state = SceneState(SpectrogramFrames(24, 20, ByteArray(0)))
        var driverNanos = 0L
        val sink = visualSceneFrameSink(engine)
        val driver = SceneDriver(
            state = state,
            clock = SceneClock { driverNanos },
            positionSource = ScenePositionSource { error("live audio must not read the playhead") },
            frameSink = sink,
            framesAllowed = { true },
        )
        engine.tick()
        native.calls.clear()

        repeat(FRAMES) {
            driverNanos += FRAME_NANOS
            nowNanos += FRAME_NANOS
            val audible = sink.hasLiveAudio()
            driver.tick(audible)
        }

        assertEquals(List(FRAMES) { "frame" }, native.calls)
        assertEquals(0.81f, state.bassPressure, 0f)
    }

    @Test
    fun aClosedEngineAnswersEveryDrawAndTickCallWithoutTouchingTheDestroyedNativeObject() {
        // The service destroys the shared native engine while the stopped activity's composition
        // still holds a lease; the next draw on return must not reach the destroyed UniFFI object.
        native.frame = liveFrame(kick = 0.81f)
        engine.tick()
        engine.close()
        native.calls.clear()
        native.failAfterClose = true

        engine.tick()
        val scene = engine.sceneBytes(100f, 100f)
        val tinted = engine.sceneBytesTinted(100f, 100f, 0.1f, 0.2f, 0.3f)
        val audible = engine.hasLiveAudio()
        val pressure = engine.bassPressure()

        assertEquals(0, scene.size)
        assertEquals(0, tinted.size)
        assertFalse(audible)
        assertEquals(VisualBassPressure.SILENT, pressure)
        assertEquals(emptyList<String>(), native.calls)
    }

    @Test
    fun aClosedEngineIgnoresEveryOtherCallInsteadOfThrowing() {
        engine.close()
        native.calls.clear()
        native.failAfterClose = true

        engine.setAccent(0.1f, 0.2f, 0.3f)
        engine.setPlaying(true)
        engine.noteTrackChanged()
        engine.ingestBands(floatArrayOf(0.5f))
        engine.adoptShape(floatArrayOf(0.5f))
        engine.setPlaybackIntent(true)
        engine.ingestPcm16(ByteArray(4), 4, 44_100, 2)
        engine.resetAudioStream()
        engine.resetAudioHistory()

        assertEquals(0, engine.currentBands().size)
        assertEquals(0, engine.adoptableBands().size)
        assertFalse(engine.adoptableBandsAreLive())
        assertEquals(emptyList<String>(), native.calls)
    }

    @Test
    fun closingTwiceClosesTheNativeEngineOnce() {
        engine.close()
        engine.close()

        assertEquals(listOf("close"), native.calls)
    }

    private fun liveFrame(kick: Float) = AndroidVisualFrame(
        advanced = true,
        hasLiveAudio = true,
        bassPressure = pressure(kick),
    )

    private fun pressure(kick: Float) = AndroidBassPressure(
        levelDbfs = -20f,
        baselineDbfs = -30f,
        impact = 0.72f,
        aura = 0.44f,
        kick = kick,
        pressure = 0.63f,
    )

    private companion object {
        const val FRAME_NANOS = 16_666_667L
        const val STALE_NANOS = 60_000_000L
        const val FRAMES = 5
    }
}

/** A stand-in for the UniFFI engine that logs the calls it receives and answers the few that matter. */
private class CountingNative {
    val calls = mutableListOf<String>()
    var frame: AndroidVisualFrame? = null
    var hasLiveAudio = false
    var onFrame: () -> Unit = {}

    /** Mirrors UniFFI: once `close` was called, any other call throws. */
    var failAfterClose = false
    var pressure = AndroidBassPressure(-140f, -140f, 0f, 0f, 0f, 0f)

    val proxy: AndroidVisualEngineInterface = Proxy.newProxyInstance(
        AndroidVisualEngineInterface::class.java.classLoader,
        arrayOf(AndroidVisualEngineInterface::class.java, AutoCloseable::class.java),
    ) { _, method, args -> answer(method.name, method.returnType, args.orEmpty()) }
        as AndroidVisualEngineInterface

    private fun answer(name: String, returnType: Class<*>, arguments: Array<out Any?>): Any? {
        val label = if (name == "setPlaying" || name == "setAccent") {
            "$name:${arguments.joinToString(",")}"
        } else {
            name
        }
        calls += label
        check(!(failAfterClose && name != "close")) {
            "AndroidVisualEngine object has already been destroyed"
        }
        return when (name) {
            "frame" -> frame.also { onFrame() }
            "hasLiveAudio" -> hasLiveAudio
            "bassPressure" -> pressure
            else -> if (returnType == java.lang.Boolean.TYPE) false else null
        }
    }
}
