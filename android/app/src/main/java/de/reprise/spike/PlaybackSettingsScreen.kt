package de.reprise.spike

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import java.util.Locale
import kotlin.math.roundToInt

internal data class EqualizerBandUi(
    val frequencyHz: Double,
    val gainDb: Double,
    val minimumGainDb: Double,
    val maximumGainDb: Double,
)

/**
 * Why there are no bands to show. Read only when the list is empty — the screen
 * must not name a cause it does not know, and "start playback" is false while a
 * track is already playing on a device that gave us no equalizer at all.
 */
internal enum class EqualizerBandsAbsence {
    /** No audio session yet, so there is nothing to ask about the bands. */
    NO_PLAYBACK_YET,

    /** There is a session, and this device provided no equalizer for it. */
    NO_EQUALIZER_ON_THIS_DEVICE,
}

internal data class PlaybackSettingsUiState(
    val equalizerEnabled: Boolean,
    val gaplessEnabled: Boolean,
    val equalizerBands: List<EqualizerBandUi>,
    val equalizerBandsAbsence: EqualizerBandsAbsence = EqualizerBandsAbsence.NO_PLAYBACK_YET,
    val error: String? = null,
)

/**
 * The settings screen before its state has arrived. It exists so that a
 * rotation, which recreates the activity and throws the loaded settings away
 * while restoring the fact that this screen was open, cannot leave a blank
 * full-screen surface with no header and no visible way back.
 */
@Composable
internal fun PlaybackSettingsLoading(close: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
    ) {
        SettingsHeader("Settings", "Back to Library", close)
        Text(
            "Reading playback settings…",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            style = MaterialTheme.typography.bodyMedium,
        )
    }
}

@Composable
internal fun PlaybackSettingsScreen(
    state: PlaybackSettingsUiState,
    themeSelection: MobileThemeSelection,
    close: () -> Unit,
    setEqualizerEnabled: (Boolean) -> Unit,
    replaceEqualizerCurve: (List<EqualizerCurvePoint>) -> Unit,
    setGaplessEnabled: (Boolean) -> Unit,
    selectTheme: (MobileTheme) -> Unit,
    pageTitle: String = "Audio",
    backContentDescription: String = "Back to Library",
) {
    var confirmEdit by rememberSaveable { mutableStateOf(false) }
    var editing by rememberSaveable { mutableStateOf(false) }
    var bands by remember(state.equalizerBands, state.error) {
        mutableStateOf(state.equalizerBands)
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .testTag("settings-page-audio")
            .padding(horizontal = 16.dp),
    ) {
        SettingsHeader(pageTitle, backContentDescription, close)
        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item { SettingsSectionTitle("Playback") }
            item {
                SettingsSwitchRow(
                    title = "Gapless playback",
                    supporting = "Move between supported tracks without an added pause.",
                    checked = state.gaplessEnabled,
                    onCheckedChange = setGaplessEnabled,
                )
            }
            item { HorizontalDivider() }
            item { SettingsSectionTitle("Equalizer") }
            item {
                SettingsSwitchRow(
                    title = "Equalizer",
                    supporting = "Use this device's live audio bands.",
                    checked = state.equalizerEnabled,
                    onCheckedChange = setEqualizerEnabled,
                )
            }
            if (bands.isEmpty()) {
                item {
                    Text(
                        when (state.equalizerBandsAbsence) {
                            EqualizerBandsAbsence.NO_PLAYBACK_YET ->
                                "Start playback to read this device's equalizer bands."
                            EqualizerBandsAbsence.NO_EQUALIZER_ON_THIS_DEVICE ->
                                "This device provided no equalizer for what is playing."
                        },
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            } else {
                itemsIndexed(bands) { index, band ->
                    EqualizerBandRow(
                        band = band,
                        editing = editing,
                        change = { gainDb ->
                            bands = bands.toMutableList().also { mutable ->
                                mutable[index] = band.copy(gainDb = gainDb)
                            }
                            replaceEqualizerCurve(
                                bands.map { current ->
                                    EqualizerCurvePoint(current.frequencyHz, current.gainDb)
                                },
                            )
                        },
                    )
                }
                item {
                    Button(
                        onClick = { confirmEdit = true },
                        enabled = !editing,
                    ) {
                        Text(if (editing) "Editing this device's bands" else "Edit equalizer")
                    }
                }
            }
            state.error?.let { error ->
                item {
                    Text(
                        error,
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
            item { androidx.compose.foundation.layout.Spacer(Modifier.height(24.dp)) }
        }
    }

    if (confirmEdit) {
        AlertDialog(
            onDismissRequest = { confirmEdit = false },
            title = { Text("Replace equalizer curve?") },
            text = {
                Text("Editing here replaces the saved equalizer curve with this device's bands.")
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        confirmEdit = false
                        editing = true
                    },
                ) {
                    Text("Continue")
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmEdit = false }) {
                    Text("Cancel")
                }
            },
        )
    }
}

@Composable
private fun SettingsHeader(
    title: String,
    backContentDescription: String,
    close: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(64.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = close) {
            MaterialSymbol("arrow_back", backContentDescription)
        }
        Text(title, style = MaterialTheme.typography.titleLarge)
    }
}

@Composable
private fun SettingsSectionTitle(title: String) {
    Text(
        title,
        color = MaterialTheme.colorScheme.primary,
        style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 8.dp),
    )
}

@Composable
private fun SettingsSwitchRow(
    title: String,
    supporting: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    // The whole row toggles. Measured on a device: only the 52 x 32 dp switch at
    // the right edge answered a tap, so about nine tenths of a row a listener
    // aims at did nothing at all. `toggleable` also collapses the row into one
    // node that announces itself as a switch, which is what a screen reader
    // should hear instead of two texts and a control beside them.
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .toggleable(
                value = checked,
                onValueChange = onCheckedChange,
                role = Role.Switch,
            ),
        horizontalArrangement = Arrangement.spacedBy(16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(
                supporting,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodyMedium,
            )
        }
        Switch(checked = checked, onCheckedChange = null)
    }
}

@Composable
private fun EqualizerBandRow(
    band: EqualizerBandUi,
    editing: Boolean,
    change: (Double) -> Unit,
) {
    val frequency = frequencyLabel(band.frequencyHz)
    Column {
        Row(modifier = Modifier.fillMaxWidth()) {
            Text(frequency, modifier = Modifier.weight(1f))
            Text("${formatGain(band.gainDb)} dB", color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Slider(
            value = band.gainDb.toFloat(),
            onValueChange = { change(it.toDouble()) },
            valueRange = band.minimumGainDb.toFloat()..band.maximumGainDb.toFloat(),
            enabled = editing,
            modifier = Modifier.semantics {
                contentDescription = "$frequency equalizer band"
            },
        )
    }
}

private fun frequencyLabel(frequencyHz: Double): String =
    if (frequencyHz < 1_000.0) {
        "${frequencyHz.roundToInt()} Hz"
    } else {
        val kilohertz = frequencyHz / 1_000.0
        val rendered = if (kilohertz == kilohertz.roundToInt().toDouble()) {
            kilohertz.roundToInt().toString()
        } else {
            "%.1f".format(Locale.ROOT, kilohertz)
        }
        "$rendered kHz"
    }

private fun formatGain(gainDb: Double): String = if (gainDb >= 0.0) {
    "+%.1f".format(Locale.ROOT, gainDb)
} else {
    "%.1f".format(Locale.ROOT, gainDb)
}
