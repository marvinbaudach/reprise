package de.reprise.spike.ui.theme

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.colorResource
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import de.reprise.spike.AmbientArtworkColors
import de.reprise.spike.MobileThemeSelection
import de.reprise.spike.R

private val NocturneBackground = Color(0xFF161826)
private val NocturneSurface = Color(0xFF232532)
private val NocturneSurfaceContainer = Color(0xFF292B31)
private val NocturneOnPrimary = Color(0xFF003735)
private val NocturnePrimaryContainer = Color(0xFF00504E)
private val NocturneOnPrimaryContainer = Color(0xFF71F8F0)
private val NocturneSecondaryContainer = Color(0xFF324B4A)
private val NocturneOnSecondaryContainer = Color(0xFFCCE8E6)
private val NocturneTertiary = Color(0xFF9184D9)
private val NocturneTertiaryContainer = Color(0xFF3F386B)
private val NocturneOnTertiaryContainer = Color(0xFFC0BDFF)
private val NocturneOutline = Color(0xFF3F424D)
private val NocturneText = Color(0xFFE9E9ED)
private val NocturneMutedText = Color(0xFFB2B6CA)
private val AmbientFallbackOne = Color(0xFF203C58)
private val AmbientFallbackTwo = Color(0xFF4A275E)
private val AmbientFallbackThree = Color(0xFF124B45)
internal val AmbientTrueBlack = Color(0xFF000000)

internal fun ambientFieldColors(artwork: AmbientArtworkColors?): List<Color> =
    artwork?.asList()?.map { argb -> Color(argb) }
        ?: listOf(AmbientFallbackOne, AmbientFallbackTwo, AmbientFallbackThree)

@Composable
internal fun nocturneColorScheme() = colorResource(R.color.reprise_teal).let { primary ->
    darkColorScheme(
        primary = primary,
        onPrimary = NocturneOnPrimary,
        primaryContainer = NocturnePrimaryContainer,
        onPrimaryContainer = NocturneOnPrimaryContainer,
        secondary = NocturneOnSecondaryContainer,
        onSecondary = NocturneSecondaryContainer,
        secondaryContainer = NocturneSecondaryContainer,
        onSecondaryContainer = NocturneOnSecondaryContainer,
        tertiary = NocturneTertiary,
        onTertiary = NocturneBackground,
        tertiaryContainer = NocturneTertiaryContainer,
        onTertiaryContainer = NocturneOnTertiaryContainer,
        background = NocturneBackground,
        onBackground = NocturneText,
        surface = NocturneSurface,
        onSurface = NocturneText,
        surfaceDim = NocturneBackground,
        surfaceBright = NocturneSurface,
        surfaceContainerLowest = NocturneBackground,
        surfaceContainerLow = NocturneSurface,
        surfaceContainer = NocturneSurfaceContainer,
        surfaceContainerHigh = NocturneSurfaceContainer,
        surfaceContainerHighest = NocturneSurfaceContainer,
        onSurfaceVariant = NocturneMutedText,
        outline = NocturneOutline,
        outlineVariant = NocturneOutline,
        inverseSurface = NocturneText,
        inverseOnSurface = NocturneBackground,
        inversePrimary = NocturnePrimaryContainer,
        surfaceTint = primary,
    )
}

private val RobotoFlex = FontFamily(
    Font(R.font.roboto_flex, weight = FontWeight.Normal),
    Font(R.font.roboto_flex, weight = FontWeight.Medium),
    Font(R.font.roboto_flex, weight = FontWeight.Bold),
)

/**
 * Material Symbols carries "filled" on the variable-font axis `FILL`, not in a
 * second glyph: the `star_outline` ligature resolves to the *same* glyph id as
 * `star`, and nothing in the font's `gvar` moves a point unless `FILL` moves.
 * Asking for a different ligature name therefore drew the identical outline,
 * which is how a rating could be written correctly and still never be seen. The
 * state has to ride the axis, so a filled symbol needs its own family — the
 * same ttf pinned at the other end of `FILL`.
 *
 * `variationSettings` is honoured from API 26 up, which is this module's
 * `minSdk`; on anything older Compose ignores it rather than failing, so the
 * worst case stays the outline we already drew.
 */
@OptIn(ExperimentalTextApi::class)
private fun materialSymbolsAt(fill: Float) = FontFamily(
    Font(
        R.font.material_symbols_rounded,
        weight = FontWeight.Normal,
        variationSettings = FontVariation.Settings(FontVariation.Setting("FILL", fill)),
    ),
)

internal val MaterialSymbolsRounded = materialSymbolsAt(fill = 0f)

internal val MaterialSymbolsRoundedFilled = materialSymbolsAt(fill = 1f)

private fun TextStyle.onRobotoFlex(
    fontSize: androidx.compose.ui.unit.TextUnit = this.fontSize,
    lineHeight: androidx.compose.ui.unit.TextUnit = this.lineHeight,
    fontWeight: FontWeight? = this.fontWeight,
) = copy(
    fontFamily = RobotoFlex,
    fontSize = fontSize,
    lineHeight = lineHeight,
    fontWeight = fontWeight,
)

private val MaterialBaselineTypography = Typography()

internal val NocturneTypography = Typography(
    displayLarge = MaterialBaselineTypography.displayLarge.onRobotoFlex(),
    displayMedium = MaterialBaselineTypography.displayMedium.onRobotoFlex(),
    displaySmall = MaterialBaselineTypography.displaySmall.onRobotoFlex(),
    headlineLarge = MaterialBaselineTypography.headlineLarge.onRobotoFlex(),
    headlineMedium = MaterialBaselineTypography.headlineMedium.onRobotoFlex(
        fontSize = 28.sp,
        lineHeight = 36.sp,
    ),
    headlineSmall = MaterialBaselineTypography.headlineSmall.onRobotoFlex(),
    titleLarge = MaterialBaselineTypography.titleLarge.onRobotoFlex(
        fontSize = 22.sp,
        lineHeight = 28.sp,
    ),
    titleMedium = MaterialBaselineTypography.titleMedium.onRobotoFlex(
        fontSize = 16.sp,
        lineHeight = 24.sp,
        fontWeight = FontWeight.Medium,
    ),
    titleSmall = MaterialBaselineTypography.titleSmall.onRobotoFlex(),
    bodyLarge = MaterialBaselineTypography.bodyLarge.onRobotoFlex(
        fontSize = 16.sp,
        lineHeight = 24.sp,
    ),
    bodyMedium = MaterialBaselineTypography.bodyMedium.onRobotoFlex(
        fontSize = 14.sp,
        lineHeight = 20.sp,
    ),
    bodySmall = MaterialBaselineTypography.bodySmall.onRobotoFlex(),
    labelLarge = MaterialBaselineTypography.labelLarge.onRobotoFlex(
        fontSize = 14.sp,
        lineHeight = 20.sp,
        fontWeight = FontWeight.Medium,
    ),
    labelMedium = MaterialBaselineTypography.labelMedium.onRobotoFlex(),
    labelSmall = MaterialBaselineTypography.labelSmall.onRobotoFlex(
        fontSize = 11.sp,
        lineHeight = 16.sp,
    ),
)

internal val NocturneShapes = Shapes(
    extraSmall = RoundedCornerShape(4.dp),
    small = RoundedCornerShape(8.dp),
    medium = RoundedCornerShape(12.dp),
    large = RoundedCornerShape(16.dp),
    extraLarge = RoundedCornerShape(28.dp),
)

@Composable
internal fun RepriseTheme(
    selection: MobileThemeSelection,
    darkPalette: Boolean,
    content: @Composable () -> Unit,
) {
    MaterialTheme(
        colorScheme = androidColorScheme(LocalContext.current, selection, darkPalette),
        typography = NocturneTypography,
        shapes = NocturneShapes,
        content = content,
    )
}
