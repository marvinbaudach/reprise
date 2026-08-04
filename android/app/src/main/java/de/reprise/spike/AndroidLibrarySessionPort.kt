package de.reprise.spike

import android.content.ContentResolver
import android.content.Intent
import android.content.SharedPreferences
import android.net.Uri
import android.util.Log
import uniffi.reprise_android_ffi.AlbumRow
import uniffi.reprise_android_ffi.AlbumWindow as FfiAlbumWindow
import uniffi.reprise_android_ffi.ArtistRow
import uniffi.reprise_android_ffi.ArtistWindow as FfiArtistWindow
import uniffi.reprise_android_ffi.MusicLibrary
import uniffi.reprise_android_ffi.TrackRow
import uniffi.reprise_android_ffi.TrackWindow as FfiTrackWindow
import uniffi.reprise_android_ffi.WindowRange as FfiWindowRange

private const val TAG = "RepriseScan"
private const val TREE_URI_PREFERENCE = "library_tree_uri"

internal class AndroidLibrarySessionPort(
    private val resolver: ContentResolver,
    private val preferences: SharedPreferences,
    private val library: MusicLibrary,
) : LibrarySessionPort {
    override fun rememberedTreeUri(): String? =
        preferences.getString(TREE_URI_PREFERENCE, null)

    override fun rememberTreeUri(treeUri: String) {
        check(preferences.edit().putString(TREE_URI_PREFERENCE, treeUri).commit()) {
            "Could not save the selected folder"
        }
    }

    override fun persistReadPermission(treeUri: String) {
        resolver.takePersistableUriPermission(
            Uri.parse(treeUri),
            Intent.FLAG_GRANT_READ_URI_PERMISSION,
        )
    }

    override fun isTreeReadable(treeUri: String): Boolean {
        val uri = Uri.parse(treeUri)
        val hasGrant = resolver.persistedUriPermissions.any { permission ->
            permission.isReadPermission && permission.uri == uri
        }
        if (!hasGrant) {
            return false
        }
        return runCatching {
            AndroidSafSource(resolver, uri).probe(treeUri, false) != null
        }.getOrDefault(false)
    }

    override fun configureTree(treeUri: String) {
        val uri = Uri.parse(treeUri)
        library.setTreeUri(treeUri, AndroidSafSource(resolver, uri))
    }

    override fun scan(report: (LibraryScreenState.Scanning) -> Unit) {
        val summary = library.scan(UiProgress(report))
        Log.i(
            TAG,
            "Scan completed: added=${summary.added} updated=${summary.updated} " +
                "errors=${summary.errors}",
        )
    }

    override fun searchTracks(
        text: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> = library.searchTracks(text, window.toFfi()).toLibraryTracks()

    override fun listAlbums(window: LibraryWindowRange): LibraryWindow<LibraryAlbum> =
        library.listAlbums(window.toFfi()).toLibraryAlbums()

    override fun listArtists(window: LibraryWindowRange): LibraryWindow<LibraryArtist> =
        library.listArtists(window.toFfi()).toLibraryArtists()

    override fun listAlbumTracks(
        album: String,
        albumArtist: String,
        window: LibraryWindowRange,
    ): LibraryWindow<LibraryTrack> =
        library.listAlbumTracks(album, albumArtist, window.toFfi()).toLibraryTracks()
}

private fun LibraryWindowRange.toFfi() = FfiWindowRange(offset = offset, limit = limit)

private fun FfiTrackWindow.toLibraryTracks() = LibraryWindow(
    total = total,
    rows = rows.map(TrackRow::toLibraryTrack),
    hasMore = hasMore,
)

private fun FfiAlbumWindow.toLibraryAlbums() = LibraryWindow(
    total = total,
    rows = rows.map(AlbumRow::toLibraryAlbum),
    hasMore = hasMore,
)

private fun FfiArtistWindow.toLibraryArtists() = LibraryWindow(
    total = total,
    rows = rows.map(ArtistRow::toLibraryArtist),
    hasMore = hasMore,
)

private fun TrackRow.toLibraryTrack() = LibraryTrack(
    uri = uri,
    title = title,
    artist = artist,
    album = album,
    durationMs = durationMs,
    playCount = playCount,
    rating = rating,
)

private fun AlbumRow.toLibraryAlbum() = LibraryAlbum(
    title = album,
    artist = albumArtist,
    representativeUri = representativeUri,
    trackCount = trackCount,
    year = year,
    totalDurationMs = totalDurationMs,
)

private fun ArtistRow.toLibraryArtist() = LibraryArtist(
    name = artist,
    trackCount = trackCount,
    albumCount = albumCount,
    representativeUri = representativeUri,
)
