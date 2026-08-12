package de.reprise.spike

import uniffi.reprise_android_ffi.AndroidLibraryDestinationChoice
import uniffi.reprise_android_ffi.AndroidStoredLibraryDestination

internal fun AndroidStoredLibraryDestination.toBrowseTab(): BrowseTab = when (this) {
    AndroidStoredLibraryDestination.Titles -> BrowseTab.TITLES
    AndroidStoredLibraryDestination.Artists -> BrowseTab.ARTISTS
    AndroidStoredLibraryDestination.Unset,
    is AndroidStoredLibraryDestination.Unsupported,
    -> BrowseTab.TITLES
}

internal fun BrowseTab.toLibraryDestinationChoice(): AndroidLibraryDestinationChoice? = when (this) {
    BrowseTab.TITLES -> AndroidLibraryDestinationChoice.TITLES
    BrowseTab.ARTISTS -> AndroidLibraryDestinationChoice.ARTISTS
    BrowseTab.QUEUE -> null
}
