package io.github.marvinbaudach.reprise.library

import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Process
import androidx.media3.common.util.UnstableApi
import androidx.media3.session.MediaSession
import java.security.MessageDigest

/**
 * Who may read the library through the browse tree.
 *
 * The service has to be exported for Android Auto and the system to bind it,
 * which also lets every installed app connect. The tree lists titles, playlists
 * and listening history, so a controller is let in only if one of these holds,
 * and is refused otherwise:
 *
 *  1. It is this app: the same uid, which a package name cannot fake.
 *  2. Media3 vouches for it ([platformTrusted]): the system UI, the media
 *     notification, and apps the platform already lets control media. Media3
 *     decides that from the caller's uid.
 *  3. It is a pinned package (Android Auto, Wear OS) whose uid owns that package
 *     *and* whose single signing certificate matches a pinned digest. A name alone is
 *     never enough: a sideloaded app can call itself anything.
 *
 * Known edges, each accepted on purpose:
 *
 *  - [platformTrusted] is Media3's `isTrusted`, which is true for the system UI
 *    and for every app the user has given notification access or the platform's
 *    media-control permission. Such an app can already read and drive any media
 *    session through the platform, so letting it browse adds no new exposure.
 *  - The decision is a snapshot of the controller as it connected. A package
 *    that is replaced by another signer afterwards is caught only because every
 *    entry point asks again; the connect-time command sets are not revisited.
 *  - The pins fail closed. A rotated Android Auto or Wear OS signing key, a
 *    device below API 28 (no `GET_SIGNING_CERTIFICATES`), and a package that is
 *    invisible to this app (a work profile, a missing `<queries>` entry) all
 *    come out as "refused", never as "allowed"; the cost is a head unit that
 *    shows an empty library until the pin list is updated.
 */
internal fun isAllowedBrowser(
    packageName: String,
    uid: Int,
    platformTrusted: Boolean,
    ownUid: Int,
    signersOf: (packageName: String, uid: Int) -> Set<String>?,
): Boolean {
    if (uid == ownUid || platformTrusted) return true
    val pinned = PINNED_SIGNERS[packageName] ?: return false
    val actual = signersOf(packageName, uid)
    return actual != null && actual.size == 1 && pinned.containsAll(actual)
}

/**
 * SHA-256 of the signing certificate, lowercase hex without separators, per
 * package. Release keys only, taken from the Android Open Source Project's UAMP
 * sample (`allowed_media_browser_callers.xml`); development keys are left out.
 */
internal val PINNED_SIGNERS: Map<String, Set<String>> = mapOf(
    "com.google.android.projection.gearhead" to setOf(
        "fdb00c43dbde8b51cb312aa81d3b5fa17713adb94b28f598d77f8eb89daceedf",
        "1ca8dcc0bed3cbd872d2cb791200c0292ca9975768a82d676b8b424fb65b5295",
    ),
    "com.google.android.wearable.app" to setOf(
        "85cd5973541be6f477d847a0bcc6aa2527684b819cd5968529664cb07157b6fe",
    ),
)

internal fun certificateDigest(certificate: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(certificate)
        .joinToString("") { byte -> "%02x".format(byte) }

/** The one authorization decision, asked at every entry point of the browse session. */
internal fun interface BrowserAccess {
    fun isAllowed(controller: MediaSession.ControllerInfo): Boolean
}

/** [BrowserAccess] answered from the caller's uid and the installed package's signers. */
@androidx.annotation.OptIn(UnstableApi::class)
internal class PackageBrowserAccess(private val context: Context) : BrowserAccess {
    override fun isAllowed(controller: MediaSession.ControllerInfo): Boolean =
        isAllowedBrowser(
            packageName = controller.packageName,
            uid = controller.uid,
            platformTrusted = controller.isTrusted,
            ownUid = Process.myUid(),
            signersOf = ::signersOf,
        )

    /**
     * `null` when the package is not installed, not visible, or not owned by [uid],
     * and below API 28, which has no `GET_SIGNING_CERTIFICATES`.
     */
    internal fun signersOf(packageName: String, uid: Int): Set<String>? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) return null
        return try {
            @Suppress("DEPRECATION") // the flags overload needs API 33; minSdk is 26
            val info = context.packageManager.getPackageInfo(
                packageName,
                PackageManager.GET_SIGNING_CERTIFICATES,
            )
            val owner = info.applicationInfo?.uid
            val signers = info.signingInfo?.apkContentsSigners
            if (owner != uid || signers == null) {
                null
            } else {
                signers.map { signer -> certificateDigest(signer.toByteArray()) }.toSet()
            }
        } catch (error: PackageManager.NameNotFoundException) {
            null
        }
    }
}
