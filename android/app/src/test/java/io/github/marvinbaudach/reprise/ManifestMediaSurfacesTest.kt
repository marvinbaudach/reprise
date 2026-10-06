package io.github.marvinbaudach.reprise

import io.github.marvinbaudach.reprise.library.PINNED_SIGNERS
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element

/**
 * Pins what Android Auto, the system's media browsers and the launcher read
 * from the manifest. None of them can be proven on the JVM, and each fails
 * silently when its line is missing: the head unit simply does not list the
 * app, the picker simply has no widget.
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

    @Test
    fun bothWidgetPlacementsAreReceiversWithTheirProviderInfo() {
        val receivers = application.children("receiver")
        val expected = mapOf(
            ".widget.RepriseWideWidgetReceiver" to "@xml/reprise_widget_wide_info",
            ".widget.RepriseSquareWidgetReceiver" to "@xml/reprise_widget_square_info",
        )

        expected.forEach { (name, info) ->
            val receiver = receivers.singleOrNull { it.name == name }
            assertNotNull("receiver $name is not declared", receiver)
            val actions = receiver!!.children("intent-filter")
                .flatMap { it.children("action") }
                .map { it.getAttribute("android:name") }
            assertTrue(actions.toString(), "android.appwidget.action.APPWIDGET_UPDATE" in actions)
            val provider = receiver.children("meta-data")
                .single { it.getAttribute("android:name") == "android.appwidget.provider" }
            assertEquals(info, provider.getAttribute("android:resource"))
            assertTrue(File("src/main/res/xml/${info.substringAfter('/')}.xml").isFile)
        }
    }

    @Test
    fun theWidgetPickerSizesMatchTheTwoPlacements() {
        fun attribute(file: String, name: String) =
            Regex("""android:$name="([^"]*)"""").find(File("src/main/res/xml/$file").readText())!!.groupValues[1]

        assertEquals("4", attribute("reprise_widget_wide_info.xml", "targetCellWidth"))
        assertEquals("1", attribute("reprise_widget_wide_info.xml", "targetCellHeight"))
        assertEquals("2", attribute("reprise_widget_square_info.xml", "targetCellWidth"))
        assertEquals("2", attribute("reprise_widget_square_info.xml", "targetCellHeight"))
    }

    @Test
    fun theSignaturePinnedPackagesAreVisibleToTheCertificateCheck() {
        val declared = manifest.documentElement.children("queries")
            .flatMap { it.children("package") }
            .map { it.getAttribute("android:name") }
        // Without a <queries> entry the package manager hides the package on
        // API 30+, the certificate lookup fails, and Android Auto is refused.
        assertTrue(declared.toString(), PINNED_SIGNERS.keys.all { it in declared })
    }

    private val Element.name: String get() = getAttribute("android:name")

    private fun Element.children(tagName: String): List<Element> {
        val nodes = childNodes
        return (0 until nodes.length)
            .mapNotNull { nodes.item(it) as? Element }
            .filter { it.tagName == tagName }
    }
}
