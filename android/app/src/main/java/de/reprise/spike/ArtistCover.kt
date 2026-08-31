package de.reprise.spike

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import uniffi.reprise_android_ffi.AndroidArtworkSize

internal const val ARTIST_PORTRAIT_DIAMETER_DP = 210

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
    artist: LibraryArtist,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("artist-portrait-head"),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        ArtworkCover(
            visual = visual,
            size = ARTIST_PORTRAIT_DIAMETER_DP,
            modifier = Modifier.testTag("artist-portrait-head-image"),
            shape = MaterialTheme.shapes.extraLarge,
            decorative = true,
        )
        Text(
            text = artist.details(),
            style = MaterialTheme.typography.bodyMedium,
        )
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
