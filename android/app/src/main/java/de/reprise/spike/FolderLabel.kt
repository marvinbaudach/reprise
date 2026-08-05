package de.reprise.spike

import android.net.Uri
import android.provider.DocumentsContract

/**
 * The name a listener would recognise, out of the tree URI the app holds.
 *
 * The settings screen has to say *which* folder it scans, and "1 folder" says
 * only what the listener already knows. The name is in the URI: a tree's
 * document id is `<volume>:<path>` for the storage provider every music folder
 * on a phone comes from, so `primary:Music/Live` becomes `Music/Live` without
 * asking the provider anything.
 *
 * The whole relative path is kept rather than its last segment, because
 * `Live` alone is the one thing a listener with `Music/Live` and `Podcasts/Live`
 * cannot tell apart.
 *
 * It answers `null` rather than guessing whenever the id is not shaped like a
 * path — a provider may hand out an opaque token, and a screen that printed
 * that would be worse than the honest generic line it replaces. The volume root
 * (`primary:`) is `null` for the same reason: an empty name is not a name.
 */
internal fun folderLabel(treeUri: String?): String? {
    val documentId = treeUri
        ?.let { runCatching { DocumentsContract.getTreeDocumentId(Uri.parse(it)) }.getOrNull() }
        ?: return null
    if (!documentId.contains(':')) return null
    return documentId.substringAfter(':').trim('/').ifBlank { null }
}
