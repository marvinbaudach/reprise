package de.reprise.spike.settings

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import de.reprise.spike.BuildConfig

@Composable
internal fun AboutSettingsPage(back: () -> Unit) {
    Column(modifier = Modifier.fillMaxSize()) {
        SettingsTopAppBar(
            title = "About Reprise",
            backContentDescription = "Back to Settings",
            back = back,
        )
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 16.dp),
        ) {
            item { SettingsSectionTitle("Versions") }
            item {
                AboutValue(
                    label = "Reprise Mobile",
                    value = "${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})",
                )
            }
            item {
                AboutValue(
                    label = "Reprise core",
                    value = BuildConfig.REPRISE_CORE_VERSION,
                )
            }
            item { SettingsSectionTitle("Licence") }
            item {
                AboutValue(
                    label = "Mobile frontend",
                    value = BuildConfig.REPRISE_MOBILE_LICENSE,
                )
            }
            item {
                AboutValue(
                    label = "Reprise core",
                    value = BuildConfig.REPRISE_CORE_LICENSE,
                )
            }
        }
    }
}

@Composable
private fun AboutValue(label: String, value: String) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp),
    ) {
        Text(label, style = MaterialTheme.typography.bodyLarge)
        Text(
            value,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            style = MaterialTheme.typography.bodyMedium,
        )
    }
}
