package io.github.marvinbaudach.reprise

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class ArtistPhotoOfferTest {
    @Test
    fun net_4b_offersArtistPhotosOnTheFreshPopulatedPath() {
        assertTrue(shouldOfferArtistPhotos(gateEnabled = false, settled = false, artistCount = 1))
    }

    @Test
    fun net_4b_doesNotOfferArtistPhotosWhileTheGateIsOn() {
        assertFalse(shouldOfferArtistPhotos(gateEnabled = true, settled = false, artistCount = 1))
    }

    @Test
    fun net_4b_doesNotOfferArtistPhotosAfterTheQuestionWasSettled() {
        assertFalse(shouldOfferArtistPhotos(gateEnabled = false, settled = true, artistCount = 1))
    }

    @Test
    fun net_4b_doesNotOfferArtistPhotosForAnEmptyLibrary() {
        assertFalse(shouldOfferArtistPhotos(gateEnabled = false, settled = false, artistCount = 0))
    }

    @Test
    fun downloadingSettlesTheOfferBeforeUsingTheOnlineSourcesEnablePath() {
        val order = mutableListOf<String>()
        val offer = freshOffer()

        offer.downloadArtistPhotos {
            assertTrue(offer.settled)
            order += "enable"
        }

        assertEquals(listOf("enable"), order)
        assertTrue(offer.settled)
        assertTrue(preferences().getBoolean(ARTIST_PHOTO_OFFER_SETTLED, false))
        assertFalse(shouldOfferArtistPhotos(false, reloadedOffer().settled, 68))
    }

    @Test
    fun notNowSettlesTheOfferWithoutEnablingAnything() {
        val offer = freshOffer()

        offer.notNow()

        assertTrue(offer.settled)
        assertTrue(preferences().getBoolean(ARTIST_PHOTO_OFFER_SETTLED, false))
        assertFalse(shouldOfferArtistPhotos(false, reloadedOffer().settled, 68))
    }

    private fun freshOffer(): ArtistPhotoOfferState {
        preferences().edit().clear().commit()
        return reloadedOffer()
    }

    private fun reloadedOffer() = ArtistPhotoOfferState(preferences())

    private fun preferences() = RuntimeEnvironment.getApplication()
        .getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)
}
