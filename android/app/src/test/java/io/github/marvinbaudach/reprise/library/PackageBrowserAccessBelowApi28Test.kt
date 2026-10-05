package io.github.marvinbaudach.reprise.library

import android.content.Context
import android.content.pm.ApplicationInfo
import android.content.pm.PackageInfo
import android.os.Process
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
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
        val info = PackageInfo().apply {
            packageName = SIGNED_PACKAGE
            applicationInfo = ApplicationInfo().apply {
                packageName = SIGNED_PACKAGE
                uid = OWNER_UID
            }
        }
        shadowOf(context.packageManager).installPackage(info)

        assertNull(access.signersOf(SIGNED_PACKAGE, OWNER_UID))
    }

    @Test
    fun theAppItselfAndPlatformTrustedControllersStillGetInBelowApi28() {
        assertTrue(access.isAllowed(controller("any.name", Process.myUid(), platformTrusted = false)))
        assertTrue(access.isAllowed(controller("com.android.systemui", OTHER_UID, platformTrusted = true)))
    }
}
