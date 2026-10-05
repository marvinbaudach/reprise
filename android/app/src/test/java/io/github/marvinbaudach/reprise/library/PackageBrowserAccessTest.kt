package io.github.marvinbaudach.reprise.library

import android.content.Context
import android.content.pm.ApplicationInfo
import android.content.pm.PackageInfo
import android.content.pm.Signature
import android.content.pm.SigningInfo
import android.os.Bundle
import android.os.Process
import androidx.media3.session.MediaSession
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

internal const val OWNER_UID = 10_321
internal const val OTHER_UID = 10_322
internal const val SIGNED_PACKAGE = "com.example.signed"
private val CERTIFICATE = byteArrayOf(1, 2, 3, 4)

private fun Context.install(packageName: String, uid: Int, withSigningInfo: Boolean) {
    val info = PackageInfo().apply {
        this.packageName = packageName
        applicationInfo = ApplicationInfo().apply {
            this.packageName = packageName
            this.uid = uid
        }
        if (withSigningInfo) {
            signingInfo = SigningInfo().also { shadowOf(it).setSignatures(arrayOf(Signature(CERTIFICATE))) }
        }
    }
    shadowOf(packageManager).installPackage(info)
}

internal fun controller(packageName: String, uid: Int, platformTrusted: Boolean) =
    MediaSession.ControllerInfo.createTestOnlyControllerInfo(
        packageName, 1, uid, 1, 1, platformTrusted, Bundle.EMPTY, false,
    )

/**
 * The part of the access decision that touches the platform: the package
 * manager's answer about who signed a package, and the controller facts the
 * decision is made from. The pure policy is `BrowseTrustPolicyTest`.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class PackageBrowserAccessTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val access = PackageBrowserAccess(context)

    @Test
    fun theSignersOfAnInstalledPackageAreItsCertificateDigestsWhenTheUidOwnsIt() {
        context.install(SIGNED_PACKAGE, OWNER_UID, withSigningInfo = true)

        assertEquals(setOf(certificateDigest(CERTIFICATE)), access.signersOf(SIGNED_PACKAGE, OWNER_UID))
    }

    @Test
    fun aPackageThatTheCallersUidDoesNotOwnHasNoSigners() {
        context.install(SIGNED_PACKAGE, OWNER_UID, withSigningInfo = true)

        // Same package, so the lookup succeeds, but another app is the one calling.
        assertNull(access.signersOf(SIGNED_PACKAGE, OTHER_UID))
    }

    @Test
    fun aPackageThatIsNotInstalledOrNotVisibleHasNoSigners() {
        assertNull(access.signersOf("com.example.missing", OWNER_UID))
    }

    @Test
    fun aPackageWithoutSigningInfoHasNoSigners() {
        context.install(SIGNED_PACKAGE, OWNER_UID, withSigningInfo = false)

        assertNull(access.signersOf(SIGNED_PACKAGE, OWNER_UID))
    }

    @Test
    fun theAppItselfIsLetInByUidWhateverItIsCalled() {
        assertTrue(access.isAllowed(controller("any.name.at.all", Process.myUid(), platformTrusted = false)))
    }

    @Test
    fun aControllerThePlatformVouchesForIsLetInThroughMedia3sTrustFlag() {
        assertTrue(access.isAllowed(controller("com.android.systemui", OTHER_UID, platformTrusted = true)))
    }

    @Test
    fun aStrangerThePlatformDoesNotVouchForIsRefused() {
        assertFalse(access.isAllowed(controller("com.example.snoop", OTHER_UID, platformTrusted = false)))
    }

    @Test
    fun aStrangerClaimingAnAutoPackageNameIsRefused() {
        context.install("com.google.android.projection.gearhead", OTHER_UID, withSigningInfo = true)

        assertFalse(
            access.isAllowed(controller("com.google.android.projection.gearhead", OTHER_UID, platformTrusted = false)),
        )
    }
}
