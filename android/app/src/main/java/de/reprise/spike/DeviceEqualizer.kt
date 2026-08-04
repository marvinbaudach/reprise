package de.reprise.spike

import android.media.audiofx.Equalizer
import kotlin.math.ln
import kotlin.math.roundToInt

internal data class EqualizerCurvePoint(
    val frequencyHz: Double,
    val gainDb: Double,
)

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
) {
    private var engine: EqualizerEngine? = null
    private var enabled = false
    private var curve = emptyList<EqualizerCurvePoint>()
    private var currentSnapshot: DeviceEqualizerSnapshot? = null

    fun configure(enabled: Boolean, curve: List<EqualizerCurvePoint>) {
        requireValidCurve(curve)
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
        val bands = (0 until target.numberOfBands).map { band ->
            val frequencyHz = target.centerFrequencyMilliHz(band).toDouble() / MILLIHERTZ_PER_HERTZ
            val projectedGainDb = projectCurve(frequencyHz, curve)
            val levelMilliBel = (projectedGainDb * MILLIBEL_PER_DECIBEL)
                .roundToInt()
                .coerceIn(levelRange)
            target.setBandLevelMilliBel(band, levelMilliBel)
            DeviceEqualizerBand(
                frequencyHz = frequencyHz,
                gainDb = levelMilliBel / MILLIBEL_PER_DECIBEL,
                minimumGainDb = levelRange.first / MILLIBEL_PER_DECIBEL,
                maximumGainDb = levelRange.last / MILLIBEL_PER_DECIBEL,
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

private fun requireValidCurve(curve: List<EqualizerCurvePoint>) {
    require(curve.isNotEmpty()) { "equalizer curve must contain at least one point" }
    var previousFrequency = 0.0
    curve.forEach { point ->
        require(point.frequencyHz.isFinite() && point.frequencyHz > previousFrequency) {
            "equalizer curve frequencies must be finite, positive, and strictly increasing"
        }
        require(point.gainDb.isFinite()) { "equalizer curve gains must be finite" }
        previousFrequency = point.frequencyHz
    }
}

private fun projectCurve(frequencyHz: Double, curve: List<EqualizerCurvePoint>): Double {
    val upperIndex = curve.indexOfFirst { it.frequencyHz >= frequencyHz }
    if (upperIndex <= 0) {
        return if (upperIndex == 0) curve[0].gainDb else curve.last().gainDb
    }

    val lower = curve[upperIndex - 1]
    val upper = curve[upperIndex]
    val fraction = (ln(frequencyHz) - ln(lower.frequencyHz)) /
        (ln(upper.frequencyHz) - ln(lower.frequencyHz))
    return lower.gainDb + fraction * (upper.gainDb - lower.gainDb)
}

private const val AUDIO_SESSION_ID_UNSET = 0
private const val EFFECT_PRIORITY = 0
private const val MILLIHERTZ_PER_HERTZ = 1_000.0
private const val MILLIBEL_PER_DECIBEL = 100.0
