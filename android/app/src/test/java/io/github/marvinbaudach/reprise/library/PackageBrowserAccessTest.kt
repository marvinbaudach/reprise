package io.github.marvinbaudach.reprise.library

import android.content.Context
import android.os.Process
import androidx.test.core.app.ApplicationProvider
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.AUTO_PACKAGE
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.CERTIFICATE
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.OTHER_UID
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.OWNER_UID
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.SIGNED_PACKAGE
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.controller
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.install
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

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
        install(context, SIGNED_PACKAGE, OWNER_UID, withSigningInfo = true)

        assertEquals(setOf(certificateDigest(CERTIFICATE)), access.signersOf(SIGNED_PACKAGE, OWNER_UID))
    }

    @Test
    fun os_9_a_package_the_callers_uid_does_not_own_has_no_signers() {
        install(context, SIGNED_PACKAGE, OWNER_UID, withSigningInfo = true)

        // Same package, so the lookup succeeds, but another app is the one calling.
        assertNull(access.signersOf(SIGNED_PACKAGE, OTHER_UID))
    }

    @Test
    fun aPackageThatIsNotInstalledHasNoSigners() {
        assertNull(access.signersOf("com.example.missing", OWNER_UID))
    }

    @Test
    fun aPackageWithoutSigningInfoHasNoSigners() {
        install(context, SIGNED_PACKAGE, OWNER_UID, withSigningInfo = false)

        assertNull(access.signersOf(SIGNED_PACKAGE, OWNER_UID))
    }

    @Test
    fun theAppItselfIsLetInByUidWhateverItIsCalled() {
        assertTrue(access.isAllowed(controller("any.name.at.all", Process.myUid(), platformTrusted = false)))
    }

    @Test
    fun os_9_a_controller_the_platform_vouches_for_is_let_in_through_media3s_trust_flag() {
        assertTrue(access.isAllowed(controller("com.android.systemui", OTHER_UID, platformTrusted = true)))
    }

    @Test
    fun aStrangerThePlatformDoesNotVouchForIsRefused() {
        assertFalse(access.isAllowed(controller("com.example.snoop", OTHER_UID, platformTrusted = false)))
    }

    /**
     * Pins refusal only. The positive path, a pinned package with its pinned
     * certificate, cannot be set up here (the pins are Google's real digests) and is
     * covered by the policy-level `os_9_android_auto_is_trusted_only_with_its_pinned_certificate`.
     */
    @Test
    fun aStrangerClaimingAnAutoPackageNameIsRefused() {
        install(context, AUTO_PACKAGE, OTHER_UID, withSigningInfo = true)

        assertFalse(access.isAllowed(controller(AUTO_PACKAGE, OTHER_UID, platformTrusted = false)))
    }
}
