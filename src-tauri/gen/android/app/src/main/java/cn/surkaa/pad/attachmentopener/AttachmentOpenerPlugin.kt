package cn.surkaa.pad.attachmentopener

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.Intent
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class OpenAttachmentArgs {
    var path: String = ""
    var mimeType: String = ""
    var displayName: String = ""
}

@TauriPlugin
class AttachmentOpenerPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun openAttachment(invoke: Invoke) {
        val args = invoke.parseArgs(OpenAttachmentArgs::class.java)
        try {
            val allowedRoot = File(activity.cacheDir, "open-attachments").canonicalFile
            val file = File(args.path).canonicalFile
            val allowedPrefix = allowedRoot.path + File.separator
            if (!file.path.startsWith(allowedPrefix) || !file.isFile) {
                invoke.reject("临时附件路径无效或超出允许目录")
                return
            }

            val uri = FileProvider.getUriForFile(
                activity,
                "${activity.packageName}.fileprovider",
                file,
            )
            val viewIntent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, args.mimeType.ifBlank { "text/html" })
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                clipData = ClipData.newUri(activity.contentResolver, args.displayName, uri)
            }
            val chooser = Intent.createChooser(viewIntent, "选择用于打开 HTML 的应用").apply {
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            activity.runOnUiThread {
                try {
                    activity.startActivity(chooser)
                    invoke.resolve(JSObject().apply { put("opened", true) })
                } catch (_: ActivityNotFoundException) {
                    invoke.reject("未找到能够打开 HTML 文件的应用")
                } catch (error: Exception) {
                    invoke.reject(error.message ?: "系统无法打开 HTML 文件")
                }
            }
        } catch (error: Exception) {
            invoke.reject(error.message ?: "准备 HTML 文件失败")
        }
    }
}
