package io.github.marvinbaudach.reprise.widget

import android.content.Context
import android.content.SharedPreferences
import androidx.core.content.edit

private const val PREFERENCES = "reprise_widget"
private const val KEY_TRACK_ID = "track_id"
private const val KEY_TITLE = "title"
private const val KEY_ARTIST = "artist"
private const val KEY_ARTWORK = "artwork"
private const val NO_TRACK = -1L

/**
 * The widget's last known state, kept across process death.
 *
 * The launcher redraws the widget whenever it likes, including after a reboot
 * when no playback service is running; without a stored state it would show an
 * empty widget for a user who played music yesterday.
 *
 * Whether music is *playing* is deliberately not stored: it is only true while
 * this process is playing, so it lives in memory and is false in any process
 * that has not been told otherwise.
 */
internal class WidgetStateStore(private val preferences: SharedPreferences) {
    constructor(context: Context) : this(
        context.applicationContext.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE),
    )

    fun load(): WidgetNowPlaying {
        val trackId = preferences.getLong(KEY_TRACK_ID, NO_TRACK).takeIf { it != NO_TRACK }
            ?: return WidgetNowPlaying.Empty
        return WidgetNowPlaying(
            trackId = trackId,
            title = preferences.getString(KEY_TITLE, "").orEmpty(),
            artist = preferences.getString(KEY_ARTIST, "").orEmpty(),
            isPlaying = playingInThisProcess,
            artworkPath = preferences.getString(KEY_ARTWORK, null),
        )
    }

    fun save(state: WidgetNowPlaying) {
        playingInThisProcess = state.isPlaying
        preferences.edit(commit = true) {
            if (state.trackId == null) {
                clear()
            } else {
                putLong(KEY_TRACK_ID, state.trackId)
                putString(KEY_TITLE, state.title)
                putString(KEY_ARTIST, state.artist)
                putString(KEY_ARTWORK, state.artworkPath)
            }
        }
    }

    internal companion object {
        @Volatile
        private var playingInThisProcess = false

        /** For tests: a new process starts out not playing. */
        fun resetProcessState() {
            playingInThisProcess = false
        }
    }
}
