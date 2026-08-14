package de.reprise.spike.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.IconButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import de.reprise.spike.MaterialSymbol
import de.reprise.spike.MobileTheme
import de.reprise.spike.MobileThemeSelection

internal enum class SettingsRoute(val route: String) {
    OVERVIEW("overview"),
    LIBRARY("library"),
    AUDIO("audio"),
    APPEARANCE("appearance"),
    ONLINE_SOURCES("online-sources"),
    ABOUT("about"),
}

private data class SettingsSection(
    val destination: SettingsRoute,
    val symbol: String,
    val title: String,
    val subtitle: String,
)

@Composable
internal fun SettingsOverview(
    titleCount: Long,
    themeSelection: MobileThemeSelection,
    onlineSourcesEnabled: Boolean,
    versionName: String,
    error: String?,
    close: () -> Unit,
    open: (SettingsRoute) -> Unit,
) {
    val titleNoun = if (titleCount == 1L) "title" else "titles"
    val sections = listOf(
        SettingsSection(
            destination = SettingsRoute.LIBRARY,
            symbol = "library_music",
            title = "Library & scan folder",
            subtitle = "$titleCount $titleNoun · 1 folder",
        ),
        SettingsSection(
            destination = SettingsRoute.AUDIO,
            symbol = "equalizer",
            title = "Audio",
            subtitle = "Gapless, Equalizer",
        ),
        SettingsSection(
            destination = SettingsRoute.APPEARANCE,
            symbol = "palette",
            title = "Appearance",
            subtitle = themeSelection.palette.displayName(),
        ),
        SettingsSection(
            destination = SettingsRoute.ONLINE_SOURCES,
            symbol = "cloud",
            title = "Online sources",
            subtitle = if (onlineSourcesEnabled) "On" else "Off",
        ),
        SettingsSection(
            destination = SettingsRoute.ABOUT,
            symbol = "info",
            title = "About Reprise",
            subtitle = versionName,
        ),
    )

    Column(modifier = Modifier.fillMaxSize()) {
        SettingsTopAppBar(
            title = "Settings",
            backContentDescription = "Back to Library",
            back = close,
        )
        LazyColumn(modifier = Modifier.fillMaxSize()) {
            items(sections, key = { section -> section.destination.route }) { section ->
                SettingsOverviewRow(section = section, open = { open(section.destination) })
            }
            error?.let { message ->
                item {
                    Text(
                        text = message,
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                    )
                }
            }
        }
    }
}

@Composable
private fun SettingsOverviewRow(section: SettingsSection, open: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .height(72.dp)
            .testTag("settings-overview-row")
            .clickable(onClickLabel = "Open ${section.title}", onClick = open),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .padding(horizontal = 16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            MaterialSymbol(section.symbol, "")
            Column(
                modifier = Modifier
                    .weight(1f)
                    .padding(horizontal = 16.dp),
            ) {
                Text(section.title, style = MaterialTheme.typography.bodyLarge)
                Text(
                    section.subtitle,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
            MaterialSymbol("chevron_right", "Open ${section.title}")
        }
        HorizontalDivider(modifier = Modifier.padding(horizontal = 16.dp))
    }
}

@Composable
internal fun SettingsTopAppBar(
    title: String,
    backContentDescription: String,
    back: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(64.dp)
            .padding(horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = back) {
            MaterialSymbol("arrow_back", backContentDescription)
        }
        Text(title, style = MaterialTheme.typography.titleLarge)
    }
}

@Composable
internal fun SettingsSectionTitle(title: String) {
    Text(
        title,
        color = MaterialTheme.colorScheme.primary,
        style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 8.dp),
    )
}

internal fun MobileTheme.displayName(): String = when (this) {
    MobileTheme.NOCTURNE -> "Nocturne"
    MobileTheme.DYNAMIC -> "Dynamic colour"
}
