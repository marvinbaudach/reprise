package io.github.marvinbaudach.reprise

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element

/**
 * Pins what Android Auto and the system's media browsers read
 * from the manifest. None of them can be proven on the JVM, and each fails
 * silently when its line is missing: the head unit simply does not list the
 * app.
 */
class ManifestMediaSurfacesTest {
    private val manifest = File("src/main/AndroidManifest.xml").inputStream().use { stream ->
        DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(stream)
    }
    private val application = manifest.documentElement.children("application").single()

    @Test
    fun theServiceIsFoundThroughEveryActionABrowserBindsWith() {
        val service = application.children("service")
            .single { it.name == ".ReprisePlaybackService" }
        val actions = service.children("intent-filter")
            .flatMap { it.children("action") }
            .map { it.getAttribute("android:name") }

        assertTrue(actions.toString(), "androidx.media3.session.MediaSessionService" in actions)
        assertTrue(actions.toString(), "androidx.media3.session.MediaLibraryService" in actions)
        assertTrue(actions.toString(), "android.media.browse.MediaBrowserService" in actions)
        assertEquals("true", service.getAttribute("android:exported"))
        assertEquals("mediaPlayback", service.getAttribute("android:foregroundServiceType"))
    }

    @Test
    fun theAppDeclaresItselfAMediaAppToAndroidAuto() {
        val declaration = application.children("meta-data")
            .single { it.getAttribute("android:name") == "com.google.android.gms.car.application" }

        assertEquals("@xml/automotive_app_desc", declaration.getAttribute("android:resource"))
        val description = File("src/main/res/xml/automotive_app_desc.xml").readText()
        assertTrue(description, Regex("""<uses\s+name="media"\s*/>""").containsMatchIn(description))
    }

    private val Element.name: String get() = getAttribute("android:name")

    private fun Element.children(tagName: String): List<Element> {
        val nodes = childNodes
        return (0 until nodes.length)
            .mapNotNull { nodes.item(it) as? Element }
            .filter { it.tagName == tagName }
    }
}
