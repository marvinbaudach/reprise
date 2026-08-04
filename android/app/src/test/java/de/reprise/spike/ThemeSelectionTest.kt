package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.reprise_android_ffi.AndroidAppearanceSettings
import uniffi.reprise_android_ffi.AndroidColorScheme
import uniffi.reprise_android_ffi.AndroidStoredTheme
import uniffi.reprise_android_ffi.AndroidThemeChoice

class ThemeSelectionTest {
    @Test
    fun unknownStoredThemeFallsBackWithoutWritingItBack() {
        val port = RecordingThemeSettingsPort(
            AndroidAppearanceSettings(
                theme = AndroidStoredTheme.Unsupported("night-terrain"),
                colorScheme = AndroidColorScheme.DARK,
            ),
        )

        val selection = ThemeController(port, dynamicAvailable = true).load()

        assertEquals(MobileTheme.NOCTURNE, selection.palette)
        assertTrue("startup fallback must not overwrite the shared id", port.writes.isEmpty())
    }

    @Test
    fun explicitThemeChoiceWritesTheTypedMobileId() {
        val port = RecordingThemeSettingsPort(
            AndroidAppearanceSettings(
                theme = AndroidStoredTheme.Unset,
                colorScheme = AndroidColorScheme.SYSTEM,
            ),
        )
        val controller = ThemeController(port, dynamicAvailable = true)

        val selection = controller.select(controller.load(), MobileTheme.DYNAMIC)

        assertEquals(MobileTheme.DYNAMIC, selection.palette)
        assertEquals(listOf(AndroidThemeChoice.DYNAMIC), port.writes)
    }

    @Test
    fun storedDynamicThemeAppliesAtStartupWhenTheRuntimeSupportsIt() {
        val port = RecordingThemeSettingsPort(
            AndroidAppearanceSettings(
                theme = AndroidStoredTheme.Dynamic,
                colorScheme = AndroidColorScheme.SYSTEM,
            ),
        )

        val selection = ThemeController(port, dynamicAvailable = true).load()

        assertEquals(MobileTheme.DYNAMIC, selection.palette)
        assertEquals(AndroidColorScheme.SYSTEM, selection.colorScheme)
        assertTrue(port.writes.isEmpty())
    }

    @Test
    fun dynamicThemeIsNotOfferedWhenTheRuntimeCannotRenderIt() {
        val port = RecordingThemeSettingsPort(
            AndroidAppearanceSettings(
                theme = AndroidStoredTheme.Dynamic,
                colorScheme = AndroidColorScheme.LIGHT,
            ),
        )

        val selection = ThemeController(port, dynamicAvailable = false).load()

        assertEquals(MobileTheme.NOCTURNE, selection.palette)
        assertEquals(listOf(MobileTheme.NOCTURNE), selection.availableThemes)
        assertTrue("an API fallback must not rewrite dynamic", port.writes.isEmpty())
    }

    @Test
    fun onlyDynamicThemeUsesTheSharedColorScheme() {
        val nocturne = MobileThemeSelection(
            palette = MobileTheme.NOCTURNE,
            colorScheme = AndroidColorScheme.LIGHT,
            dynamicAvailable = true,
        )
        val dynamicLight = nocturne.copy(
            palette = MobileTheme.DYNAMIC,
            colorScheme = AndroidColorScheme.LIGHT,
        )
        val dynamicSystem = dynamicLight.copy(colorScheme = AndroidColorScheme.SYSTEM)

        assertTrue(nocturne.usesDarkPalette(systemIsDark = false))
        assertEquals(false, dynamicLight.usesDarkPalette(systemIsDark = true))
        assertEquals(false, dynamicSystem.usesDarkPalette(systemIsDark = false))
        assertTrue(dynamicSystem.usesDarkPalette(systemIsDark = true))
    }
}

private class RecordingThemeSettingsPort(
    private val settings: AndroidAppearanceSettings,
) : ThemeSettingsPort {
    val writes = mutableListOf<AndroidThemeChoice>()

    override fun appearanceSettings(): AndroidAppearanceSettings = settings

    override fun setTheme(theme: AndroidThemeChoice) {
        writes += theme
    }
}
