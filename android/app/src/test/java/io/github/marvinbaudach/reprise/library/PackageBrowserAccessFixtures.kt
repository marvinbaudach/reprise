package io.github.marvinbaudach.reprise.library

import android.content.Context
import android.content.pm.ApplicationInfo
import android.content.pm.PackageInfo
import android.content.pm.Signature
import android.content.pm.SigningInfo
import android.os.Bundle
import androidx.media3.session.MediaSession
import org.robolectric.Shadows.shadowOf

/** What the two `PackageBrowserAccess` test files share: one installed package and the controllers asking about it. */
internal object PackageBrowserAccessFixtures {
    const val OWNER_UID = 10_321
    const val OTHER_UID = 10_322
    const val SIGNED_PACKAGE = "com.example.signed"
    const val AUTO_PACKAGE = "com.google.android.projection.gearhead"
    val CERTIFICATE = byteArrayOf(1, 2, 3, 4)

    /** Installs [packageName] owned by [uid], with the signing info of [CERTIFICATE] when asked. */
    fun install(context: Context, packageName: String, uid: Int, withSigningInfo: Boolean) {
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
        shadowOf(context.packageManager).installPackage(info)
    }

    fun controller(packageName: String, uid: Int, platformTrusted: Boolean): MediaSession.ControllerInfo =
        MediaSession.ControllerInfo.createTestOnlyControllerInfo(
            packageName, 1, uid, 1, 1, platformTrusted, Bundle.EMPTY, false,
        )
}
