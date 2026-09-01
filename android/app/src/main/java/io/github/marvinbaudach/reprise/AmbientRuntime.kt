package io.github.marvinbaudach.reprise

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.database.ContentObserver
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.provider.Settings
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner

internal data class SceneRenderPower(
    val fogRotates: Boolean,
)

/** Projects the shared runtime truth onto only the scene effects it may suppress. */
internal fun AmbientMotionController.sceneRenderPower(): SceneRenderPower = SceneRenderPower(
    fogRotates = sceneAnimationsEnabled,
)

/** Binds ambient scheduling to the activity, screen and animator-scale truth. */
@Composable
internal fun BindAmbientRuntime(
    controller: AmbientMotionController,
    animationsEnabled: () -> Boolean,
) {
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(context, lifecycle, controller, animationsEnabled) {
        val power = context.getSystemService(Context.POWER_SERVICE) as PowerManager
        fun update() {
            controller.runtimeChanged(
                resumed = lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED),
                screenInteractive = power.isInteractive,
                animationsEnabled = animationsEnabled(),
            )
        }
        val lifecycleObserver = LifecycleEventObserver { _, _ -> update() }
        val screenReceiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context?, intent: Intent?) = update()
        }
        val animatorObserver = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) = update()
        }
        lifecycle.addObserver(lifecycleObserver)
        ContextCompat.registerReceiver(
            context,
            screenReceiver,
            IntentFilter().apply {
                addAction(Intent.ACTION_SCREEN_ON)
                addAction(Intent.ACTION_SCREEN_OFF)
            },
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        context.contentResolver.registerContentObserver(
            Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE),
            false,
            animatorObserver,
        )
        update()
        onDispose {
            lifecycle.removeObserver(lifecycleObserver)
            context.unregisterReceiver(screenReceiver)
            context.contentResolver.unregisterContentObserver(animatorObserver)
            controller.stop()
        }
    }
}
