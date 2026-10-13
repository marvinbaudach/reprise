package io.github.marvinbaudach.reprise

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import uniffi.reprise_android_ffi.AndroidArtworkSize
import uniffi.reprise_android_ffi.AndroidPortraitCredit

internal const val ARTIST_PORTRAIT_DIAMETER_DP = 210
internal const val ARTIST_PORTRAIT_BOTTOM_GAP_DP = 16

/** The only host a credit link may point at: the Commons file page of the portrait. */
private const val COMMONS_HOST = "commons.wikimedia.org"
private const val CREDIT_TAG = "RepriseArtistCover"

@Composable
internal fun rememberArtistArtworkVisual(
    name: String,
    representativeUri: String,
    artworkSize: AndroidArtworkSize,
    allowFetch: Boolean,
): ArtworkVisual? {
    val artwork = LocalTrackArtwork.current
    val refreshRevision = if (allowFetch) 0L else artwork?.artistPortraitRevision ?: 0L
    val gate = remember { ArtworkRequestGate() }
    val request = remember(name, representativeUri, artworkSize, allowFetch, refreshRevision) {
        artistArtworkRequest(name, representativeUri, artworkSize, allowFetch)
    }
    var visual by remember(request, artwork, refreshRevision) {
        mutableStateOf(artwork?.seedVisual(request))
    }
    DisposableEffect(request, artwork, refreshRevision) {
        val admitted = gate.begin(
            trackUri = representativeUri,
            size = artworkSize,
            title = name,
            kind = ArtworkKind.ARTIST,
            artistName = name,
            allowFetch = allowFetch,
        )
        artwork?.loadVisual(admitted, gate) { loaded -> visual = loaded }
        onDispose { gate.invalidate(admitted) }
    }
    return visual
}

@Composable
internal fun ArtistAvatar(
    visual: ArtworkVisual?,
    sizeDp: Int,
) {
    ArtworkCover(
        visual = visual,
        size = sizeDp,
        modifier = Modifier.testTag("artist-avatar"),
        shape = CircleShape,
        decorative = true,
    )
}

@Composable
internal fun ArtistPortraitHeader(
    visual: ArtworkVisual?,
    credit: AndroidPortraitCredit?,
) {
    val context = LocalContext.current
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(bottom = ARTIST_PORTRAIT_BOTTOM_GAP_DP.dp)
            .testTag("artist-portrait-head"),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        ArtworkCover(
            visual = visual,
            size = ARTIST_PORTRAIT_DIAMETER_DP,
            modifier = Modifier.testTag("artist-portrait-head-image"),
            shape = MaterialTheme.shapes.extraLarge,
            decorative = true,
        )
        credit?.let { value ->
            Text(
                text = artistPortraitCreditText(value),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier
                    .testTag("artist-portrait-credit")
                    .minimumInteractiveComponentSize()
                    .clickable(
                        enabled = isHttpsCreditUrl(value.filePageUrl),
                        onClickLabel = "Open image source",
                        role = Role.Button,
                    ) { openCreditPage(context, value.filePageUrl) }
                    .padding(horizontal = 24.dp, vertical = 8.dp),
            )
        }
    }
}

internal fun artistPortraitCreditText(credit: AndroidPortraitCredit): String =
    "Photo: ${credit.author} · ${credit.license} · Wikimedia Commons"

internal fun isHttpsCreditUrl(url: String): Boolean =
    url.toUri().let { uri ->
        uri.scheme.equals("https", ignoreCase = true) &&
            uri.host.equals(COMMONS_HOST, ignoreCase = true) &&
            uri.userInfo == null
    }

private fun openCreditPage(context: Context, url: String) {
    if (!isHttpsCreditUrl(url)) return
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, url.toUri()))
    } catch (error: ActivityNotFoundException) {
        Log.w(CREDIT_TAG, "No app can open the portrait credit page", error)
    }
}

private fun artistArtworkRequest(
    name: String,
    representativeUri: String,
    artworkSize: AndroidArtworkSize,
    allowFetch: Boolean,
) = ArtworkRequest(
    trackUri = representativeUri,
    size = artworkSize,
    title = name,
    kind = ArtworkKind.ARTIST,
    artistName = name,
    allowFetch = allowFetch,
)
