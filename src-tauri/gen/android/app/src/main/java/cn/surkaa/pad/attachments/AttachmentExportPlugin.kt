package cn.surkaa.pad.attachments

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.util.Locale
import org.json.JSONObject

@InvokeArg
class CreateExportFileArgs {
    var directoryUri: String = ""
    var filename: String = ""
    var mimetype: String = "application/octet-stream"
}

@InvokeArg
class DeleteExportFileArgs { var uri: String = "" }

@InvokeArg
class OpenExportLocationArgs { var uri: String = ""; var directory: Boolean = false }

/** 使用用户本次授权的 SAF 目录创建新文件，不申请整个存储空间的访问权限。 */
@TauriPlugin
class AttachmentExportPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun openLocation(invoke: Invoke) {
        val args = invoke.parseArgs(OpenExportLocationArgs::class.java)
        try {
            val uri = Uri.parse(args.uri)
            require(uri.scheme == "content" && DocumentsContract.isDocumentUri(activity, uri).let {
                it || DocumentsContract.isTreeUri(uri)
            }) { "无效的下载位置" }
            // 树 URI 转为目录文档后交给文件管理器；不申请额外存储权限，不授予写权限。
            val initial = if (args.directory && DocumentsContract.isTreeUri(uri))
                DocumentsContract.buildDocumentUriUsingTree(uri, DocumentsContract.getTreeDocumentId(uri)) else uri
            if (args.directory) {
                try {
                    activity.startActivity(Intent(Intent.ACTION_VIEW).apply {
                        setDataAndType(initial, DocumentsContract.Document.MIME_TYPE_DIR)
                        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    })
                    invoke.resolve(JSObject()); return
                } catch (_: ActivityNotFoundException) {
                    // 部分系统没有目录查看器，继续用系统文档浏览界面定位。
                }
            }
            // INITIAL_URI 接受文件或目录 URI；单文件保存时不猜测/拼接提供方的父目录 ID。
            activity.startActivity(Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = "*/*"
                putExtra(DocumentsContract.EXTRA_INITIAL_URI, initial)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            })
            invoke.resolve(JSObject())
        } catch (error: Exception) {
            invoke.reject(error.message ?: "系统无法打开下载位置，请使用文件管理器查看")
        }
    }

    @Command
    fun pickDirectory(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        }
        startActivityForResult(invoke, intent, "directorySelected")
    }

    @ActivityCallback
    fun directorySelected(invoke: Invoke, result: ActivityResult) {
        val response = JSObject()
        val uri = if (result.resultCode == Activity.RESULT_OK) result.data?.data else null
        response.put("uri", uri?.toString() ?: JSONObject.NULL)
        invoke.resolve(response)
    }

    @Command
    fun createFile(invoke: Invoke) {
        val args = invoke.parseArgs(CreateExportFileArgs::class.java)
        Thread {
            try {
                require(args.filename.isNotBlank() && args.filename !in listOf(".", "..") &&
                    args.filename.none { it == '/' || it == '\\' || it.isISOControl() }) { "无效的附件文件名" }
                val directory = Uri.parse(args.directoryUri)
                require(directory.scheme == "content" && DocumentsContract.isTreeUri(directory)) { "请选择系统授权的目录" }
                val resolver = activity.contentResolver
                val parentId = DocumentsContract.getTreeDocumentId(directory)
                val parent = DocumentsContract.buildDocumentUriUsingTree(directory, parentId)
                val children = DocumentsContract.buildChildDocumentsUriUsingTree(directory, parentId)
                val names = mutableSetOf<String>()
                resolver.query(children, arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME), null, null, null)?.use { cursor ->
                    while (cursor.moveToNext()) names.add(cursor.getString(0).lowercase(Locale.ROOT))
                } ?: error("无法读取目标目录，未创建任何文件")
                var name = args.filename
                var index = 1
                val dot = args.filename.lastIndexOf('.')
                while (name.lowercase(Locale.ROOT) in names) {
                    require(index <= 100000) { "同名文件过多" }
                    name = if (dot > 0) "${args.filename.substring(0, dot)} (${index++})${args.filename.substring(dot)}"
                        else "${args.filename} (${index++})"
                }
                // 始终 createDocument，不通过展示名打开现有文件，因此不会覆盖旧文件。
                val document = DocumentsContract.createDocument(resolver, parent, args.mimetype.ifBlank { "application/octet-stream" }, name)
                    ?: error("系统无法在该目录创建文件")
                val result = JSObject()
                result.put("uri", document.toString())
                activity.runOnUiThread { invoke.resolve(result) }
            } catch (error: Exception) {
                activity.runOnUiThread { invoke.reject(error.message ?: "创建附件导出文件失败") }
            }
        }.start()
    }

    @Command
    fun deleteFile(invoke: Invoke) {
        val args = invoke.parseArgs(DeleteExportFileArgs::class.java)
        Thread {
            try {
                check(DocumentsContract.deleteDocument(activity.contentResolver, Uri.parse(args.uri))) { "无法清理未完成文件" }
                activity.runOnUiThread { invoke.resolve(JSObject()) }
            } catch (error: Exception) {
                activity.runOnUiThread { invoke.reject(error.message ?: "清理未完成文件失败") }
            }
        }.start()
    }
}
