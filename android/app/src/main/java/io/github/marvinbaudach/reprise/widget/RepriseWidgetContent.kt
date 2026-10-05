package io.github.marvinbaudach.reprise.widget

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceModifier
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import androidx.glance.LocalSize
import androidx.glance.action.Action
import androidx.glance.action.clickable
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.background
import androidx.glance.color.ColorProvider
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxHeight
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import io.github.marvinbaudach.reprise.MainActivity
import io.github.marvinbaudach.reprise.R

/** The two sizes the widget is drawn at; the launcher picks the nearest. */
internal val WIDE_SIZE = DpSize(250.dp, 56.dp)
internal val SQUARE_SIZE = DpSize(110.dp, 110.dp)

private val WidgetBackground = ColorProvider(day = Color(0xFFF2F4F6), night = Color(0xFF1E2128))
private val PrimaryText = ColorProvider(day = Color(0xFF14171C), night = Color(0xFFF1F3F5))
private val SecondaryText = ColorProvider(day = Color(0xFF5A6270), night = Color(0xFFB4BBC6))
private val ControlTint = ColorProvider(day = Color(0xFF14171C), night = Color(0xFFF1F3F5))
private val Scrim = ColorProvider(day = Color(0x66000000), night = Color(0x66000000))
private val OnScrim = ColorProvider(day = Color.White, night = Color.White)

private val CORNER = 20.dp
private val COVER_WIDE = 56.dp
private val CONTROL = 40.dp
private val CONTROL_PADDING = 8.dp
private val SQUARE_PLAY = 48.dp

/** Whether [size] is room enough for the wide layout. */
internal fun isWide(size: DpSize): Boolean = size.width >= 180.dp && size.height < 100.dp

/**
 * The widget's content. [cover] is already decoded and downscaled: Glance turns
 * a bitmap into a RemoteViews payload that crosses a binder, which has a hard
 * size limit.
 */
@Composable
internal fun RepriseWidgetContent(state: WidgetNowPlaying, cover: Bitmap?) {
    val context = LocalContext.current
    val openApp = actionStartActivity(Intent(context, MainActivity::class.java))
    when {
        state.isEmpty -> EmptyWidget(context, openApp)
        isWide(LocalSize.current) -> WideWidget(context, state, cover, openApp)
        else -> SquareWidget(context, state, cover, openApp)
    }
}

@Composable
private fun EmptyWidget(context: Context, openApp: Action) {
    Row(
        modifier = GlanceModifier.fillMaxSize().background(WidgetBackground).cornerRadius(CORNER)
            .padding(12.dp).clickable(openApp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Image(
            provider = ImageProvider(R.mipmap.ic_launcher),
            contentDescription = context.getString(R.string.widget_open_app),
            modifier = GlanceModifier.size(40.dp).clickable(openApp),
        )
        Spacer(GlanceModifier.width(12.dp))
        Text(
            text = context.getString(R.string.widget_empty_title),
            style = TextStyle(color = PrimaryText, fontSize = 18.sp, fontWeight = FontWeight.Bold),
        )
    }
}

@Composable
private fun WideWidget(context: Context, state: WidgetNowPlaying, cover: Bitmap?, openApp: Action) {
    Row(
        modifier = GlanceModifier.fillMaxSize().background(WidgetBackground).cornerRadius(CORNER),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Cover(context, cover, GlanceModifier.size(COVER_WIDE).fillMaxHeight(), openApp)
        Column(
            modifier = GlanceModifier.defaultWeight().padding(horizontal = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = state.title.ifBlank { context.getString(R.string.widget_unknown_title) },
                maxLines = 1,
                style = TextStyle(color = PrimaryText, fontSize = 14.sp, fontWeight = FontWeight.Bold),
            )
            if (state.artist.isNotBlank()) {
                Text(
                    text = state.artist,
                    maxLines = 1,
                    style = TextStyle(color = SecondaryText, fontSize = 12.sp),
                )
            }
        }
        Control(context, R.drawable.ic_widget_previous, R.string.widget_previous, actionRunCallback<PreviousAction>())
        Control(context, playIcon(state), playDescription(state), actionRunCallback<TogglePlayAction>())
        Control(context, R.drawable.ic_widget_next, R.string.widget_next, actionRunCallback<NextAction>())
    }
}

@Composable
private fun SquareWidget(context: Context, state: WidgetNowPlaying, cover: Bitmap?, openApp: Action) {
    Box(
        modifier = GlanceModifier.fillMaxSize().background(WidgetBackground).cornerRadius(CORNER),
        contentAlignment = Alignment.BottomEnd,
    ) {
        Cover(context, cover, GlanceModifier.fillMaxSize(), openApp)
        Box(
            modifier = GlanceModifier.padding(8.dp),
        ) {
            Image(
                provider = ImageProvider(playIcon(state)),
                contentDescription = context.getString(playDescription(state)),
                colorFilter = ColorFilter.tint(OnScrim),
                modifier = GlanceModifier.size(SQUARE_PLAY).background(Scrim).cornerRadius(SQUARE_PLAY)
                    .padding(CONTROL_PADDING).clickable(actionRunCallback<TogglePlayAction>()),
            )
        }
    }
}

@Composable
private fun Cover(context: Context, cover: Bitmap?, modifier: GlanceModifier, openApp: Action) {
    Image(
        provider = if (cover != null) ImageProvider(cover) else ImageProvider(R.mipmap.ic_launcher),
        contentDescription = context.getString(R.string.widget_open_app),
        modifier = modifier.clickable(openApp),
    )
}

@Composable
private fun Control(context: Context, icon: Int, description: Int, action: Action) {
    Image(
        provider = ImageProvider(icon),
        contentDescription = context.getString(description),
        colorFilter = ColorFilter.tint(ControlTint),
        modifier = GlanceModifier.size(CONTROL).padding(CONTROL_PADDING).clickable(action),
    )
}

internal fun playIcon(state: WidgetNowPlaying): Int =
    if (state.isPlaying) R.drawable.ic_widget_pause else R.drawable.ic_widget_play

internal fun playDescription(state: WidgetNowPlaying): Int =
    if (state.isPlaying) R.string.widget_pause else R.string.widget_play
