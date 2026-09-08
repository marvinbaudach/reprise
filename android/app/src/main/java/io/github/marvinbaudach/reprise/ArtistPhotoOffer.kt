package io.github.marvinbaudach.reprise

import android.content.SharedPreferences
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.core.content.edit

internal const val PREFERENCES_NAME = "reprise_android"
internal const val NOTIFICATION_PERMISSION_ASKED = "notification_permission_asked"
internal const val ARTIST_PHOTO_OFFER_SETTLED = "artist_photo_offer_settled"

internal fun shouldOfferArtistPhotos(
    gateEnabled: Boolean,
    settled: Boolean,
    artistCount: Long,
): Boolean = !gateEnabled && !settled && artistCount >= 1

@Composable
internal fun rememberArtistPhotoOffer(preferences: SharedPreferences): ArtistPhotoOfferState =
    remember(preferences) { ArtistPhotoOfferState(preferences) }

internal class ArtistPhotoOfferState(
    private val preferences: SharedPreferences,
) {
    var settled by mutableStateOf(preferences.getBoolean(ARTIST_PHOTO_OFFER_SETTLED, false))
        private set

    fun downloadArtistPhotos(enableOnlineSources: () -> Unit) {
        settle()
        enableOnlineSources()
    }

    fun notNow() {
        settle()
    }

    private fun settle() {
        preferences.edit {
            putBoolean(ARTIST_PHOTO_OFFER_SETTLED, true)
        }
        settled = true
    }
}
