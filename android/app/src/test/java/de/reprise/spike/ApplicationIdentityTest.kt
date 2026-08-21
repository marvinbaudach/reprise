package de.reprise.spike

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [36])
class ApplicationIdentityTest {
    @Test
    fun applicationLabelComesFromTheTranslatableStringResource() {
        val application = RuntimeEnvironment.getApplication()
        val applicationInfo = application.applicationInfo

        assertTrue(applicationInfo.labelRes != 0)
        assertNull(applicationInfo.nonLocalizedLabel)
        assertEquals("Reprise", application.getString(applicationInfo.labelRes))
        assertEquals(
            application.getString(applicationInfo.labelRes),
            application.packageManager.getApplicationLabel(applicationInfo).toString(),
        )
    }

    @Test
    fun installedIdentityUsesTheProductIdAndMobileVersion() {
        val application = RuntimeEnvironment.getApplication()
        val packageInfo = application.packageManager.getPackageInfo(application.packageName, 0)

        assertEquals("io.github.marvinbaudach.reprise", BuildConfig.APPLICATION_ID)
        assertEquals(BuildConfig.APPLICATION_ID, application.packageName)
        assertEquals(BuildConfig.VERSION_NAME, packageInfo.versionName)
    }
}
