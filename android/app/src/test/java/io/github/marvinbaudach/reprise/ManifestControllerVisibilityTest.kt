package io.github.marvinbaudach.reprise

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertNotNull
import org.junit.Test
import org.w3c.dom.Element

/**
 * Pins the manifest text only. Whether the query actually resolves a
 * controller package on a device is proven by the plan's device run
 * (docs/plans/the-launcher-reprise-could-not-see.md, "Post-landing"), not by
 * this test.
 */
class ManifestControllerVisibilityTest {
    @Test
    fun manifestDeclaresTheNotificationListenerQuery() {
        val manifest = File("src/main/AndroidManifest.xml").inputStream().use { stream ->
            DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(stream)
        }

        val queries = manifest.documentElement.children("queries")
        val action = queries
            .flatMap { it.children("intent") }
            .flatMap { it.children("action") }
            .firstOrNull { it.getAttribute("android:name") == NOTIFICATION_LISTENER_ACTION }

        assertNotNull(
            "manifest/queries/intent/action for $NOTIFICATION_LISTENER_ACTION is missing: " +
                "without it Reprise cannot resolve a controller package through PackageManager, " +
                "media3 treats the controller as untrusted and hands it read-only commands, and a " +
                "launcher's transport buttons silently do nothing (#982, logcat " +
                "\"Package … doesn't exist\")",
            action,
        )
    }

    private fun Element.children(tagName: String): List<Element> {
        val nodes = childNodes
        return (0 until nodes.length)
            .mapNotNull { nodes.item(it) as? Element }
            .filter { it.tagName == tagName }
    }

    private companion object {
        const val NOTIFICATION_LISTENER_ACTION = "android.service.notification.NotificationListenerService"
    }
}
