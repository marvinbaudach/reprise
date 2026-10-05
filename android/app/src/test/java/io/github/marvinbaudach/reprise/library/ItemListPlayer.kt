package io.github.marvinbaudach.reprise.library

import android.os.Looper
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import java.lang.reflect.Proxy

/**
 * A [Player] that is nothing but a playlist and a call log.
 *
 * It records every call by name so a test can tell what a wrapper forwarded,
 * and it keeps the media items so a test can read back what was set.
 */
internal class ItemListPlayer {
    val items = mutableListOf<MediaItem>()
    val calls = mutableListOf<String>()
    var playWhenReady = false

    val player: Player = Proxy.newProxyInstance(
        Player::class.java.classLoader,
        arrayOf(Player::class.java),
    ) { _, method, arguments ->
        val args = arguments ?: emptyArray()
        calls += method.name
        when (method.name) {
            "addListener", "removeListener" -> null
            "getApplicationLooper" -> Looper.getMainLooper()
            "getAudioSessionId" -> C.AUDIO_SESSION_ID_UNSET
            "getPlayWhenReady" -> playWhenReady
            "play" -> {
                playWhenReady = true
                null
            }
            "setMediaItem" -> {
                items.clear()
                items += args[0] as MediaItem
                null
            }
            "setMediaItems" -> {
                items.clear()
                @Suppress("UNCHECKED_CAST")
                items += args[0] as List<MediaItem>
                null
            }
            "addMediaItem" -> {
                items += args.last() as MediaItem
                null
            }
            "addMediaItems" -> {
                @Suppress("UNCHECKED_CAST")
                items += args.last() as List<MediaItem>
                null
            }
            "getMediaItemCount" -> items.size
            "getMediaItemAt" -> items[args[0] as Int]
            "getCurrentMediaItemIndex" -> 0
            "replaceMediaItem" -> {
                items[args[0] as Int] = args[1] as MediaItem
                null
            }
            "removeMediaItems" -> {
                items.subList(args[0] as Int, args[1] as Int).clear()
                null
            }
            "clearMediaItems" -> {
                items.clear()
                null
            }
            else -> defaultFor(method.returnType)
        }
    } as Player

    private fun defaultFor(type: Class<*>): Any? = when (type) {
        Boolean::class.javaPrimitiveType -> false
        Int::class.javaPrimitiveType -> 0
        Long::class.javaPrimitiveType -> 0L
        Float::class.javaPrimitiveType -> 0f
        Double::class.javaPrimitiveType -> 0.0
        else -> null
    }
}
