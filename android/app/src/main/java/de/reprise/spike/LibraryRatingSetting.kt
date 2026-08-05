package de.reprise.spike

import uniffi.reprise_android_ffi.AndroidStoredLibraryRating
import uniffi.reprise_android_ffi.MusicLibrary

internal interface LibraryRatingSettingPort {
    fun libraryRatingSetting(): AndroidStoredLibraryRating

    fun setLibraryRating(enabled: Boolean)
}

internal class AndroidLibraryRatingSettingPort(
    private val library: MusicLibrary,
) : LibraryRatingSettingPort {
    override fun libraryRatingSetting(): AndroidStoredLibraryRating =
        library.libraryRatingSetting()

    override fun setLibraryRating(enabled: Boolean) {
        library.setLibraryRating(enabled)
    }
}

/** Resolves the surface-scoped stored value without turning fallback into a write. */
internal class LibraryRatingSettingController(
    private val port: LibraryRatingSettingPort,
) {
    fun load(): Boolean = when (port.libraryRatingSetting()) {
        AndroidStoredLibraryRating.On -> true
        AndroidStoredLibraryRating.Off,
        AndroidStoredLibraryRating.Unset,
        is AndroidStoredLibraryRating.Unsupported,
        -> false
    }

    fun select(enabled: Boolean): Boolean {
        port.setLibraryRating(enabled)
        return enabled
    }
}
