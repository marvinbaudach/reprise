package io.github.marvinbaudach.reprise

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import java.util.Locale

/** Header control for the timer that remains owned by the playback service. */
@Composable
internal fun SleepTimerControl(timer: SleepTimerUiState) {
    val controls = LocalPlaybackControls.current
    var expanded by remember { mutableStateOf(false) }
    val runningLabel = timer.runningLabel()
    Box {
        IconButton(
            onClick = { expanded = true },
            modifier = Modifier
                .size(height = 48.dp, width = if (timer.active) 88.dp else 48.dp)
                .testTag("sleep-timer-control"),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                MaterialSymbol(
                    name = "bedtime",
                    contentDescription = if (timer.active) {
                        "Sleep timer, $runningLabel${if (timer.remainingSeconds != null) " remaining" else ""}"
                    } else {
                        "Set sleep timer"
                    },
                )
                if (timer.active) Text(runningLabel)
            }
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            SleepTimerController.TIMER_MINUTES.forEach { minutes ->
                DropdownMenuItem(
                    text = { Text("$minutes minutes") },
                    onClick = {
                        expanded = false
                        controls.startSleepTimer(SleepTimerSelection.Minutes(minutes))
                    },
                )
            }
            DropdownMenuItem(
                text = { Text("End of track") },
                onClick = {
                    expanded = false
                    controls.startSleepTimer(SleepTimerSelection.EndOfTrack)
                },
            )
            if (timer.active) {
                DropdownMenuItem(
                    text = { Text("Cancel timer") },
                    onClick = {
                        expanded = false
                        controls.cancelSleepTimer()
                    },
                )
            }
        }
    }
}

private fun SleepTimerUiState.runningLabel(): String = when (selection) {
    is SleepTimerSelection.Minutes -> {
        val seconds = remainingSeconds ?: 0
        String.format(Locale.ROOT, "%d:%02d", seconds / 60, seconds % 60)
    }
    SleepTimerSelection.EndOfTrack -> "end of track"
    null -> ""
}
