package de.reprise.spike

import android.Manifest
import android.content.pm.PackageManager
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class ManifestPermissionsTest {
    @Test
    fun theAppRequestsInternetAndNothingUnexpected() {
        val application = RuntimeEnvironment.getApplication()
        val actual = application.packageManager
            .getPackageInfo(application.packageName, PackageManager.GET_PERMISSIONS)
            .requestedPermissions
            .orEmpty()
            .toSet()

        assertTrue("INTERNET must not disappear from the packaged app", Manifest.permission.INTERNET in actual)
        val missing = EXPECTED_PERMISSIONS - actual
        val unexpected = actual - EXPECTED_PERMISSIONS
        assertEquals(
            "manifest permissions changed: missing=$missing unexpected=$unexpected",
            EXPECTED_PERMISSIONS,
            actual,
        )
    }

    private companion object {
        // ACCESS_NETWORK_STATE and WAKE_LOCK are contributed by
        // androidx.media3:media3-exoplayer:1.10.1. The app-scoped signature
        // permission is contributed by androidx.core:core:1.19.0.
        val EXPECTED_PERMISSIONS = setOf(
            Manifest.permission.FOREGROUND_SERVICE,
            Manifest.permission.FOREGROUND_SERVICE_MEDIA_PLAYBACK,
            Manifest.permission.POST_NOTIFICATIONS,
            Manifest.permission.INTERNET,
            Manifest.permission.ACCESS_NETWORK_STATE,
            Manifest.permission.WAKE_LOCK,
            "org.reprise.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION",
        )
    }
}
