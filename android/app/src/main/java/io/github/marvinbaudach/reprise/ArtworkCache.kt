package io.github.marvinbaudach.reprise

import androidx.compose.ui.graphics.ImageBitmap
import java.util.LinkedHashMap
import uniffi.reprise_android_ffi.AndroidArtworkSize

private sealed interface ArtworkVisualCacheKey

// A phone fits about 11 rows at the smallest 72 dp row height. Retaining the
// current screen plus one screen of scroll, the prefetch window and headroom
// needs about 27 entries; 32 leaves margin for taller or denser screens.
private const val LIST_ARTWORK_CAPACITY = 32
private const val NOW_PLAYING_ARTWORK_CAPACITY = 3
private const val ARTIST_DETAIL_ARTWORK_CAPACITY = 1

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

internal data class ArtworkCacheStats(
    val hits: Long,
    val misses: Long,
)

/** Small synchronized LRUs shared by the list, player and prefetch workers. */
internal class ArtworkCache(
    private val listArtworkCapacity: Int = LIST_ARTWORK_CAPACITY,
    private val nowPlayingArtworkCapacity: Int = NOW_PLAYING_ARTWORK_CAPACITY,
    private val artistDetailArtworkCapacity: Int = ARTIST_DETAIL_ARTWORK_CAPACITY,
    private val fogCapacity: Int = 6,
) {
    init {
        require(listArtworkCapacity > 0)
        require(nowPlayingArtworkCapacity > 0)
        require(artistDetailArtworkCapacity > 0)
        require(fogCapacity > 0)
    }

    private val visualShelves = mapOf(
        AndroidArtworkSize.LIST to
            lruMap<ArtworkVisualCacheKey, ArtworkVisual>(listArtworkCapacity),
        AndroidArtworkSize.NOW_PLAYING to
            lruMap<ArtworkVisualCacheKey, ArtworkVisual>(nowPlayingArtworkCapacity),
        AndroidArtworkSize.ARTIST_DETAIL to
            lruMap<ArtworkVisualCacheKey, ArtworkVisual>(artistDetailArtworkCapacity),
    )
    private val resolvedFallbackShelves = mapOf(
        AndroidArtworkSize.LIST to
            lruMap<TrackArtworkKey, GeneratedArtworkKey>(listArtworkCapacity),
        AndroidArtworkSize.NOW_PLAYING to
            lruMap<TrackArtworkKey, GeneratedArtworkKey>(nowPlayingArtworkCapacity),
        AndroidArtworkSize.ARTIST_DETAIL to
            lruMap<TrackArtworkKey, GeneratedArtworkKey>(artistDetailArtworkCapacity),
    )
    private val fogs = lruMap<ArtworkIdentity, CoverFogBitmap>(fogCapacity)
    private var artworkHits = 0L
    private var artworkMisses = 0L

    @Synchronized
    fun artwork(request: ArtworkRequest): ArtworkVisual? {
        val sourceKey = request.trackKey()
        val visuals = visuals(request.size)
        val resolvedFallbacks = resolvedFallbacks(request.size)
        visuals[sourceKey]?.let { visual ->
            artworkHits += 1
            return visual
        }
        val fallbackKey = resolvedFallbacks[sourceKey]
        val visual = fallbackKey?.let(visuals::get)
        if (visual == null) {
            artworkMisses += 1
            if (fallbackKey != null) resolvedFallbacks.remove(sourceKey)
        } else {
            artworkHits += 1
        }
        return visual
    }

    @Synchronized
    fun artworkStats() = ArtworkCacheStats(hits = artworkHits, misses = artworkMisses)

    /**
     * Best immediate visual for a surface that will still resolve its own size.
     * Reusing another size here keeps list-to-detail navigation continuous;
     * [artwork] stays exact-size so this seed can never suppress that resolve.
     */
    @Synchronized
    fun seedArtwork(request: ArtworkRequest): ArtworkVisual? {
        artwork(request)?.let { return it }
        val sourceKey = visualShelves.values
            .asSequence()
            .flatMap { visuals -> visuals.keys.asSequence() }
            .filterIsInstance<TrackArtworkKey>()
            .lastOrNull { key -> key.matchesIdentity(request) }
        sourceKey?.let { key ->
            visuals(key.size)[key]?.let { visual ->
                artworkHits += 1
                return visual
            }
        }
        val fallbackKey = resolvedFallbackShelves.values
            .asSequence()
            .flatMap { fallbacks -> fallbacks.entries.asSequence() }
            .lastOrNull { (key, _) -> key.matchesIdentity(request) }
            ?.value
        fallbackKey?.let { key ->
            visuals(key.size)[key]?.let { visual ->
                artworkHits += 1
                return visual
            }
        }
        val generatedKey = visualShelves.values
            .asSequence()
            .flatMap { visuals -> visuals.keys.asSequence() }
            .filterIsInstance<GeneratedArtworkKey>()
            .lastOrNull { key -> key.matchesIdentity(request) }
        return generatedKey?.let { key -> visuals(key.size)[key] }.also { visual ->
            if (visual != null) artworkHits += 1
        }
    }

    @Synchronized
    fun putArtwork(request: ArtworkRequest, visual: ArtworkVisual) {
        visuals(request.size)[request.trackKey()] = visual
        resolvedFallbacks(request.size).remove(request.trackKey())
    }

    @Synchronized
    fun invalidateArtistArtwork(request: ArtworkRequest) {
        if (request.kind != ArtworkKind.ARTIST) return
        visualShelves.values.forEach { visuals ->
            visuals.keys.removeAll { key ->
                key is TrackArtworkKey &&
                    key.trackUri == request.trackUri &&
                    key.kind == ArtworkKind.ARTIST
            }
        }
        resolvedFallbackShelves.values.forEach { fallbacks ->
            fallbacks.keys.removeAll { key ->
                key.trackUri == request.trackUri && key.kind == ArtworkKind.ARTIST
            }
        }
    }

    @Synchronized
    fun invalidateArtistArtwork() {
        visualShelves.values.forEach { visuals ->
            visuals.keys.removeAll { key ->
                key is TrackArtworkKey && key.kind == ArtworkKind.ARTIST
            }
        }
        resolvedFallbackShelves.values.forEach { fallbacks ->
            fallbacks.keys.removeAll { key -> key.kind == ArtworkKind.ARTIST }
        }
    }

    @Synchronized
    fun generated(request: ArtworkRequest): ArtworkVisual? =
        visuals(request.size)[request.generatedKey()]

    @Synchronized
    fun putGenerated(request: ArtworkRequest, visual: ArtworkVisual, resolved: Boolean = false) {
        val generatedKey = request.generatedKey()
        visuals(request.size)[generatedKey] = visual
        if (resolved) resolvedFallbacks(request.size)[request.trackKey()] = generatedKey
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

    private fun visuals(size: AndroidArtworkSize) = visualShelves.getValue(size)

    private fun resolvedFallbacks(size: AndroidArtworkSize) =
        resolvedFallbackShelves.getValue(size)

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
