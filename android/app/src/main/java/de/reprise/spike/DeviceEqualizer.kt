package de.reprise.spike

import android.media.audiofx.Equalizer
import kotlin.math.roundToInt

internal data class EqualizerCurvePoint(
    val frequencyHz: Double,
    val gainDb: Double,
)

/** What one of this device's bands sits at, and how far it can move. */
internal data class DeviceEqualizerBandCapability(
    val frequencyHz: Double,
    val minimumGainDb: Double,
    val maximumGainDb: Double,
)

/**
 * Samples an authored curve at this device's band centres.
 *
 * Deliberately not implemented here. Kotlin used to carry its own copy of that
 * arithmetic beside the tested one in `reprise-core`, so the version with the
 * tests guaranteed nothing about what a real phone rendered. The only
 * implementation now lives in the core, behind
 * `uniffi.reprise_android_ffi.projectEqualizerCurve`; this seam exists so the
 * JVM tests can drive the wiring without loading the native library.
 */
internal fun interface EqualizerCurveProjector {
    /** Gains in dB, one per band, in the order the bands were given. */
    fun project(
        curve: List<EqualizerCurvePoint>,
        bands: List<DeviceEqualizerBandCapability>,
    ): List<Double>
}

internal data class DeviceEqualizerBand(
    val frequencyHz: Double,
    val gainDb: Double,
    val minimumGainDb: Double,
    val maximumGainDb: Double,
)

internal data class DeviceEqualizerSnapshot(
    val enabled: Boolean,
    val bands: List<DeviceEqualizerBand>,
)

internal interface EqualizerEngineFactory {
    fun create(audioSessionId: Int): EqualizerEngine
}

internal interface EqualizerEngine {
    val numberOfBands: Int
    val levelRangeMilliBel: IntRange
    var enabled: Boolean

    fun centerFrequencyMilliHz(band: Int): Int

    fun setBandLevelMilliBel(band: Int, level: Int)

    fun release()
}

/** Owns the Equalizer effect for the current Media3 audio session. */
internal class DeviceEqualizer(
    private val factory: EqualizerEngineFactory,
    private val projector: EqualizerCurveProjector,
) {
    private var engine: EqualizerEngine? = null
    private var enabled = false
    private var curve = emptyList<EqualizerCurvePoint>()
    private var currentSnapshot: DeviceEqualizerSnapshot? = null

    fun configure(enabled: Boolean, curve: List<EqualizerCurvePoint>) {
        this.enabled = enabled
        this.curve = curve.toList()
        engine?.let(::applySafely)
    }

    fun onAudioSessionChanged(audioSessionId: Int) {
        engine?.release()
        engine = null
        currentSnapshot = null
        if (audioSessionId <= AUDIO_SESSION_ID_UNSET) {
            return
        }

        runCatching { factory.create(audioSessionId) }.getOrNull()?.also { replacement ->
            engine = replacement
            applySafely(replacement)
        }
    }

    fun snapshot(): DeviceEqualizerSnapshot? = currentSnapshot

    fun release() {
        engine?.release()
        engine = null
        currentSnapshot = null
    }

    private fun applyTo(target: EqualizerEngine) {
        val levelRange = target.levelRangeMilliBel
        val capabilities = (0 until target.numberOfBands).map { band ->
            DeviceEqualizerBandCapability(
                frequencyHz = target.centerFrequencyMilliHz(band).toDouble() / MILLIHERTZ_PER_HERTZ,
                minimumGainDb = levelRange.first / MILLIBEL_PER_DECIBEL,
                maximumGainDb = levelRange.last / MILLIBEL_PER_DECIBEL,
            )
        }
        val gainsDb = projector.project(curve, capabilities)
        require(gainsDb.size == capabilities.size) {
            "the projection answered ${gainsDb.size} gains for ${capabilities.size} bands"
        }
        // Only the device's own representation is decided here: whole millibel,
        // inside the integer range its API accepts. Rounding can land a band one
        // step outside a range the projection already respected.
        val bands = capabilities.mapIndexed { band, capability ->
            val levelMilliBel = (gainsDb[band] * MILLIBEL_PER_DECIBEL)
                .roundToInt()
                .coerceIn(levelRange)
            target.setBandLevelMilliBel(band, levelMilliBel)
            DeviceEqualizerBand(
                frequencyHz = capability.frequencyHz,
                gainDb = levelMilliBel / MILLIBEL_PER_DECIBEL,
                minimumGainDb = capability.minimumGainDb,
                maximumGainDb = capability.maximumGainDb,
            )
        }
        target.enabled = enabled
        currentSnapshot = DeviceEqualizerSnapshot(enabled = enabled, bands = bands)
    }

    private fun applySafely(target: EqualizerEngine) {
        runCatching { applyTo(target) }.onFailure {
            runCatching { target.release() }
            if (engine === target) {
                engine = null
                currentSnapshot = null
            }
        }
    }
}

internal object AndroidEqualizerEngineFactory : EqualizerEngineFactory {
    override fun create(audioSessionId: Int): EqualizerEngine =
        AndroidEqualizerEngine(Equalizer(EFFECT_PRIORITY, audioSessionId))
}

private class AndroidEqualizerEngine(
    private val equalizer: Equalizer,
) : EqualizerEngine {
    override val numberOfBands: Int
        get() = equalizer.numberOfBands.toInt()

    override val levelRangeMilliBel: IntRange
        get() = equalizer.bandLevelRange.let { range -> range[0].toInt()..range[1].toInt() }

    override var enabled: Boolean
        get() = equalizer.enabled
        set(value) {
            equalizer.enabled = value
        }

    override fun centerFrequencyMilliHz(band: Int): Int =
        equalizer.getCenterFreq(band.toShort())

    override fun setBandLevelMilliBel(band: Int, level: Int) {
        equalizer.setBandLevel(band.toShort(), level.toShort())
    }

    override fun release() = equalizer.release()
}

private const val AUDIO_SESSION_ID_UNSET = 0
private const val EFFECT_PRIORITY = 0
private const val MILLIHERTZ_PER_HERTZ = 1_000.0
private const val MILLIBEL_PER_DECIBEL = 100.0
