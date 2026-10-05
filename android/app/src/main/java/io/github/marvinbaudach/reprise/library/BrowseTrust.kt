package io.github.marvinbaudach.reprise.library

/**
 * Who may read the library through the browse tree.
 *
 * The service has to be exported for Android Auto and the system to bind it,
 * which also lets every installed app connect. The tree lists titles, playlists
 * and listening history, so it is only shown to controllers the platform
 * itself vouches for, the app's own package, and the packages that carry
 * Android Auto and Wear.
 */
internal fun isTrustedBrowser(
    packageName: String,
    platformTrusted: Boolean,
    ownPackage: String,
): Boolean = platformTrusted || packageName == ownPackage || packageName in KNOWN_BROWSER_PACKAGES

/**
 * Android Auto and Wear OS, for devices where the platform's own check does not
 * recognise them: the projection host, Google Play services (which relays Auto
 * and Assistant) and the Wear companion.
 */
private val KNOWN_BROWSER_PACKAGES = setOf(
    "com.google.android.projection.gearhead",
    "com.google.android.gms",
    "com.google.android.wearable.app",
)
