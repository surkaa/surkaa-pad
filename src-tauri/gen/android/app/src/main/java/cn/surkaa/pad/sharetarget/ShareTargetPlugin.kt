package cn.surkaa.pad.sharetarget

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.OpenableColumns
import android.text.Html
import android.webkit.WebView
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONArray
import java.util.UUID

@InvokeArg
class AckPendingShareArgs {
    var batchId: String = ""
}

private data class PendingShareItem(
    val id: String,
    val uri: String,
    val displayName: String,
    val mimeType: String?,
    val size: Long?,
)

private data class PendingShareBatch(
    val id: String,
    val subject: String?,
    val text: String?,
    val items: List<PendingShareItem>,
)

/**
 * Android 系统分享收件箱。
 *
 * Intent 会先进入原生内存队列，WebView 事件仅用于提醒前端重新读取；即使冷启动时
 * 监听器尚未注册，内容也会一直保留到前端明确确认或用户丢弃。
 */
@TauriPlugin
class ShareTargetPlugin(private val activity: Activity) : Plugin(activity) {
    private val queueLock = Any()
    private val pendingBatches = mutableListOf<PendingShareBatch>()
    private val receivedIntentFingerprints = LinkedHashSet<String>()

    override fun load(webView: WebView) {
        enqueueIntent(activity.intent, notifyWebView = false)
    }

    override fun onNewIntent(intent: Intent) {
        // 让 Activity 在进程被临时重建时尽可能保留最近一次分享 Intent。
        activity.intent = intent
        enqueueIntent(intent, notifyWebView = true)
    }

    @Command
    fun listPendingShares(invoke: Invoke) {
        val batches = synchronized(queueLock) { pendingBatches.toList() }
        val result = JSObject()
        result.put("batches", JSONArray().apply {
            batches.forEach { put(it.toJson()) }
        })
        invoke.resolve(result)
    }

    @Command
    fun ackPendingShare(invoke: Invoke) {
        val args = invoke.parseArgs(AckPendingShareArgs::class.java)
        val acknowledged = synchronized(queueLock) {
            pendingBatches.removeAll { it.id == args.batchId }
        }
        invoke.resolve(JSObject().apply { put("acknowledged", acknowledged) })
    }

    private fun enqueueIntent(intent: Intent?, notifyWebView: Boolean) {
        if (intent?.action != Intent.ACTION_SEND && intent?.action != Intent.ACTION_SEND_MULTIPLE) {
            return
        }

        val fingerprint = intent.toUri(Intent.URI_INTENT_SCHEME)
        val batch = parseIntent(intent) ?: return
        val pendingCount = synchronized(queueLock) {
            if (!receivedIntentFingerprints.add(fingerprint)) {
                return
            }
            pendingBatches.add(batch)
            // 仅用于进程内防止系统重复派发同一 Intent，限制集合避免无界增长。
            while (receivedIntentFingerprints.size > 64) {
                receivedIntentFingerprints.remove(receivedIntentFingerprints.first())
            }
            pendingBatches.size
        }

        if (notifyWebView) {
            trigger("pending-share", JSObject().apply { put("pendingCount", pendingCount) })
        }
    }

    private fun parseIntent(intent: Intent): PendingShareBatch? {
        val subject = intent.getCharSequenceExtra(Intent.EXTRA_SUBJECT)
            ?.toString()
            ?.trim()
            ?.takeIf { it.isNotEmpty() }
        val text = extractText(intent)
        val uris = extractUris(intent)
        val items = uris.mapIndexed { index, uri -> resolveItem(index, uri, intent.type) }

        if (subject == null && text == null && items.isEmpty()) {
            return null
        }

        return PendingShareBatch(
            id = UUID.randomUUID().toString(),
            subject = subject,
            text = text,
            items = items,
        )
    }

    private fun extractText(intent: Intent): String? {
        val extraText = intent.getCharSequenceExtra(Intent.EXTRA_TEXT)
            ?.toString()
            ?.takeIf { it.isNotBlank() }
        if (extraText != null) return extraText

        val clipText = intent.clipData
            ?.let { clip ->
                (0 until clip.itemCount)
                    .mapNotNull { clip.getItemAt(it).text?.toString() }
                    .filter { it.isNotBlank() }
                    .joinToString("\n")
                    .takeIf { it.isNotBlank() }
            }
        if (clipText != null) return clipText

        val htmlText = intent.getStringExtra(Intent.EXTRA_HTML_TEXT)?.takeIf { it.isNotBlank() }
            ?: return null
        @Suppress("DEPRECATION")
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            Html.fromHtml(htmlText, Html.FROM_HTML_MODE_LEGACY).toString()
        } else {
            Html.fromHtml(htmlText).toString()
        }
    }

    private fun extractUris(intent: Intent): List<Uri> {
        val uris = LinkedHashSet<Uri>()
        when (intent.action) {
            Intent.ACTION_SEND -> getSingleStream(intent)?.let(uris::add)
            Intent.ACTION_SEND_MULTIPLE -> uris.addAll(getMultipleStreams(intent))
        }

        intent.clipData?.let { clip ->
            for (index in 0 until clip.itemCount) {
                clip.getItemAt(index).uri?.let(uris::add)
            }
        }

        if (uris.isEmpty()) {
            intent.data?.let(uris::add)
        }
        return uris.filter { it.scheme == "content" || it.scheme == "file" }
    }

    private fun getSingleStream(intent: Intent): Uri? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra(Intent.EXTRA_STREAM)
        }

    private fun getMultipleStreams(intent: Intent): List<Uri> =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM).orEmpty()
        }

    private fun resolveItem(index: Int, uri: Uri, fallbackMimeType: String?): PendingShareItem {
        var displayName: String? = null
        var size: Long? = null
        try {
            activity.contentResolver.query(
                uri,
                arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE),
                null,
                null,
                null,
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val nameColumn = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (nameColumn >= 0 && !cursor.isNull(nameColumn)) {
                        displayName = cursor.getString(nameColumn)
                    }
                    val sizeColumn = cursor.getColumnIndex(OpenableColumns.SIZE)
                    if (sizeColumn >= 0 && !cursor.isNull(sizeColumn)) {
                        size = cursor.getLong(sizeColumn).takeIf { it >= 0 }
                    }
                }
            }
        } catch (_: Exception) {
            // 部分 ContentProvider 不支持元数据查询，实际读取阶段再报告权限或内容错误。
        }

        val safeName = displayName
            ?.trim()
            ?.takeIf { it.isNotEmpty() }
            ?: uri.lastPathSegment?.substringAfterLast('/')?.takeIf { it.isNotBlank() }
            ?: "共享文件-${index + 1}"

        return PendingShareItem(
            id = index.toString(),
            uri = uri.toString(),
            displayName = safeName,
            mimeType = try {
                activity.contentResolver.getType(uri)?.takeIf { it.isNotBlank() }
                    ?: fallbackMimeType?.takeIf { it.isNotBlank() && it != "*/*" }
            } catch (_: Exception) {
                fallbackMimeType?.takeIf { it.isNotBlank() && it != "*/*" }
            },
            size = size,
        )
    }
}

private fun PendingShareBatch.toJson() = JSObject().apply {
    put("id", id)
    subject?.let { put("subject", it) }
    text?.let { put("text", it) }
    put("items", JSONArray().apply {
        this@toJson.items.forEach { put(it.toJson()) }
    })
}

private fun PendingShareItem.toJson() = JSObject().apply {
    put("id", id)
    put("uri", uri)
    put("displayName", displayName)
    mimeType?.let { put("mimeType", it) }
    size?.let { put("size", it) }
}
