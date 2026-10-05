package io.github.marvinbaudach.reprise.library

import android.content.Context
import android.os.Process
import androidx.test.core.app.ApplicationProvider
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.AUTO_PACKAGE
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.OTHER_UID
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.OWNER_UID
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.SIGNED_PACKAGE
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.controller
import io.github.marvinbaudach.reprise.library.PackageBrowserAccessFixtures.install
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** Below API 28 there is no `GET_SIGNING_CERTIFICATES`, so a pinned package can never be proved. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [26])
class PackageBrowserAccessBelowApi28Test {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val access = PackageBrowserAccess(context)

    @Test
    fun aPackageThatIsInstalledAndOwnedByTheCallerStillHasNoSignersBelowApi28() {
        // Installed, uid matching: without the early return the lookup would go on to read signers.
        install(context, SIGNED_PACKAGE, OWNER_UID, withSigningInfo = false)

        assertNull(access.signersOf(SIGNED_PACKAGE, OWNER_UID))
    }

    @Test
    fun theAppItselfAndPlatformTrustedControllersStillGetInBelowApi28() {
        assertTrue(access.isAllowed(controller("any.name", Process.myUid(), platformTrusted = false)))
        assertTrue(access.isAllowed(controller("com.android.systemui", OTHER_UID, platformTrusted = true)))
    }

    @Test
    fun aForeignControllerClaimingTheAutoPackageIsRefusedBelowApi28() {
        // Installed and owned by the caller, so only the API floor stands between it and a signer lookup.
        install(context, AUTO_PACKAGE, OTHER_UID, withSigningInfo = false)

        assertFalse(access.isAllowed(controller(AUTO_PACKAGE, OTHER_UID, platformTrusted = false)))
    }
}
