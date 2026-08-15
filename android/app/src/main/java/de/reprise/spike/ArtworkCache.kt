package de.reprise.spike

import androidx.compose.ui.graphics.ImageBitmap
import java.util.LinkedHashMap
import uniffi.reprise_android_ffi.AndroidArtworkSize

private sealed interface ArtworkVisualCacheKey

private data class TrackArtworkKey(
    val trackUri: String,
    val size: AndroidArtworkSize,
    val kind: ArtworkKind,
) : ArtworkVisualCacheKey

private data class GeneratedArtworkKey(
    val title: String,
    val artist: String,
    val size: AndroidArtworkSize,
    val kind: ArtworkKind,
) : ArtworkVisualCacheKey

private class ArtworkIdentity(
    private val image: ImageBitmap,
) {
    override fun equals(other: Any?): Boolean =
        other is ArtworkIdentity && image === other.image

    override fun hashCode(): Int = System.identityHashCode(image)
}

/** Small synchronized LRUs shared by the list, player and prefetch workers. */
internal class ArtworkCache(
    private val artworkCapacity: Int = 12,
    private val fogCapacity: Int = 6,
) {
    init {
        require(artworkCapacity > 0)
        require(fogCapacity > 0)
    }

    private val visuals = lruMap<ArtworkVisualCacheKey, ArtworkVisual>(artworkCapacity)
    private val resolvedFallbacks = lruMap<TrackArtworkKey, GeneratedArtworkKey>(artworkCapacity)
    private val fogs = lruMap<ArtworkIdentity, CoverFogBitmap>(fogCapacity)

    @Synchronized
    fun artwork(request: ArtworkRequest): ArtworkVisual? {
        val sourceKey = request.trackKey()
        visuals[sourceKey]?.let { return it }
        val fallbackKey = resolvedFallbacks[sourceKey] ?: return null
        return visuals[fallbackKey].also { visual ->
            if (visual == null) resolvedFallbacks.remove(sourceKey)
        }
    }

    /**
     * Best immediate visual for a surface that will still resolve its own size.
     * Reusing another size here keeps list-to-detail navigation continuous;
     * [artwork] stays exact-size so this seed can never suppress that resolve.
     */
    @Synchronized
    fun seedArtwork(request: ArtworkRequest): ArtworkVisual? {
        artwork(request)?.let { return it }
        val sourceKey = visuals.keys
            .filterIsInstance<TrackArtworkKey>()
            .lastOrNull { key -> key.matchesIdentity(request) }
        sourceKey?.let { key -> visuals[key]?.let { return it } }
        val fallbackKey = resolvedFallbacks.entries
            .lastOrNull { (key, _) -> key.matchesIdentity(request) }
            ?.value
        fallbackKey?.let { key -> visuals[key]?.let { return it } }
        val generatedKey = visuals.keys
            .filterIsInstance<GeneratedArtworkKey>()
            .lastOrNull { key -> key.matchesIdentity(request) }
        return generatedKey?.let(visuals::get)
    }

    @Synchronized
    fun putArtwork(request: ArtworkRequest, visual: ArtworkVisual) {
        visuals[request.trackKey()] = visual
        resolvedFallbacks.remove(request.trackKey())
    }

    @Synchronized
    fun invalidateArtistArtwork(request: ArtworkRequest) {
        if (request.kind != ArtworkKind.ARTIST) return
        visuals.keys.removeAll { key ->
            key is TrackArtworkKey &&
                key.trackUri == request.trackUri &&
                key.kind == ArtworkKind.ARTIST
        }
        resolvedFallbacks.keys.removeAll { key ->
            key.trackUri == request.trackUri && key.kind == ArtworkKind.ARTIST
        }
    }

    @Synchronized
    fun generated(request: ArtworkRequest): ArtworkVisual? = visuals[request.generatedKey()]

    @Synchronized
    fun putGenerated(request: ArtworkRequest, visual: ArtworkVisual, resolved: Boolean = false) {
        val generatedKey = request.generatedKey()
        visuals[generatedKey] = visual
        if (resolved) resolvedFallbacks[request.trackKey()] = generatedKey
    }

    @Synchronized
    fun fog(image: ImageBitmap): CoverFogBitmap? = fogs[ArtworkIdentity(image)]

    @Synchronized
    fun putFog(image: ImageBitmap, fog: CoverFogBitmap) {
        fogs[ArtworkIdentity(image)] = fog
    }

    private fun ArtworkRequest.trackKey() = TrackArtworkKey(trackUri, size, kind)

    private fun ArtworkRequest.generatedKey() = GeneratedArtworkKey(
        title = title.trim().lowercase(),
        artist = artist.trim().lowercase(),
        size = size,
        kind = kind,
    )

    private fun TrackArtworkKey.matchesIdentity(request: ArtworkRequest): Boolean =
        trackUri == request.trackUri && kind == request.kind

    private fun GeneratedArtworkKey.matchesIdentity(request: ArtworkRequest): Boolean =
        title == request.title.trim().lowercase() &&
            artist == request.artist.trim().lowercase() &&
            kind == request.kind

    private fun <K, V> lruMap(capacity: Int) =
        object : LinkedHashMap<K, V>(capacity + 1, 0.75f, true) {
            override fun removeEldestEntry(eldest: MutableMap.MutableEntry<K, V>?): Boolean =
                size > capacity
        }
}

internal val SharedArtworkCache = ArtworkCache()
