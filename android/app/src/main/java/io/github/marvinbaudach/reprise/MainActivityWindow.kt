package io.github.marvinbaudach.reprise

import android.view.View
import android.view.WindowManager
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

/** No scrim behind the system bars: the app's own ground is what shows through. */
private const val TRANSPARENT_SYSTEM_BAR = 0

internal fun MainActivity.configureEdgeToEdge(darkPalette: Boolean) {
    val transparent = SystemBarStyle.auto(
        TRANSPARENT_SYSTEM_BAR,
        TRANSPARENT_SYSTEM_BAR,
    ) { darkPalette }
    enableEdgeToEdge(
        statusBarStyle = transparent,
        navigationBarStyle = transparent,
    )
}

@Suppress("DEPRECATION")
internal fun MainActivity.setDockWindowMode(docked: Boolean) {
    val keepScreenOn = WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON
    if (docked) {
        window.addFlags(keepScreenOn)
    } else {
        window.clearFlags(keepScreenOn)
    }
    WindowCompat.getInsetsController(window, window.decorView).run {
        systemBarsBehavior =
            WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        if (docked) {
            hide(WindowInsetsCompat.Type.systemBars())
        } else {
            show(WindowInsetsCompat.Type.systemBars())
        }
    }
    // Robolectric exposes this legacy request while the compat controller
    // above is the API the device follows. Keeping both also covers API 26.
    window.decorView.systemUiVisibility = if (docked) {
        View.SYSTEM_UI_FLAG_FULLSCREEN or
            View.SYSTEM_UI_FLAG_HIDE_NAVIGATION or
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
    } else {
        View.SYSTEM_UI_FLAG_VISIBLE
    }
}
