package de.reprise.spike

import uniffi.reprise_android_ffi.AndroidAppearanceSettings
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidStoredTheme
import uniffi.reprise_android_ffi.AndroidThemeChoice
import uniffi.reprise_android_ffi.MusicLibrary

internal enum class MobileTheme {
    NOCTURNE,
    DYNAMIC,
}

internal data class MobileThemeSelection(
    val palette: MobileTheme,
    val colorScheme: AndroidColorScheme,
    val dynamicAvailable: Boolean,
) {
    val availableThemes: List<MobileTheme>
        get() = if (dynamicAvailable) {
            listOf(MobileTheme.NOCTURNE, MobileTheme.DYNAMIC)
        } else {
            listOf(MobileTheme.NOCTURNE)
        }

    fun usesDarkPalette(systemIsDark: Boolean): Boolean =
        if (palette != MobileTheme.DYNAMIC || !dynamicAvailable) {
            true
        } else {
            when (colorScheme) {
                AndroidColorScheme.LIGHT -> false
                AndroidColorScheme.DARK -> true
                AndroidColorScheme.SYSTEM -> systemIsDark
            }
        }
}

internal interface ThemeSettingsPort {
    fun appearanceSettings(): AndroidAppearanceSettings

    fun setTheme(theme: AndroidThemeChoice)
}

internal class AndroidThemeSettingsPort(
    private val library: MusicLibrary,
) : ThemeSettingsPort {
    override fun appearanceSettings(): AndroidAppearanceSettings = library.appearanceSettings()

    override fun setTheme(theme: AndroidThemeChoice) = library.setTheme(theme)
}

/** Resolves shared settings into what this Android device can render. */
internal class ThemeController(
    private val port: ThemeSettingsPort,
    private val dynamicAvailable: Boolean,
) {
    /** Reads and resolves only. Fallback is never a reason to persist. */
    fun load(): MobileThemeSelection {
        val settings = port.appearanceSettings()
        val palette = when (settings.theme) {
            AndroidStoredTheme.Dynamic -> {
                if (dynamicAvailable) MobileTheme.DYNAMIC else MobileTheme.NOCTURNE
            }
            AndroidStoredTheme.Nocturne,
            AndroidStoredTheme.Unset,
            is AndroidStoredTheme.Unsupported,
            -> MobileTheme.NOCTURNE
        }
        return MobileThemeSelection(
            palette = palette,
            colorScheme = settings.colorScheme,
            dynamicAvailable = dynamicAvailable,
        )
    }

    /** Persists only an explicit choice the current device can render. */
    fun select(current: MobileThemeSelection, palette: MobileTheme): MobileThemeSelection {
        check(palette != MobileTheme.DYNAMIC || dynamicAvailable) {
            "Dynamic colour is unavailable on this Android version"
        }
        port.setTheme(
            when (palette) {
                MobileTheme.NOCTURNE -> AndroidThemeChoice.NOCTURNE
                MobileTheme.DYNAMIC -> AndroidThemeChoice.DYNAMIC
            },
        )
        return current.copy(palette = palette)
    }
}
