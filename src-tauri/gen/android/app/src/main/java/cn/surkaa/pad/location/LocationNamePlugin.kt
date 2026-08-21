package cn.surkaa.pad.location

import android.app.Activity
import android.location.Geocoder
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.util.Locale

@InvokeArg
class ReverseGeocodeArgs {
    var latitude: Double = 0.0
    var longitude: Double = 0.0
}

@TauriPlugin
class LocationNamePlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun reverseGeocode(invoke: Invoke) {
        val args = invoke.parseArgs(ReverseGeocodeArgs::class.java)

        Thread {
            try {
                val result = JSObject()
                if (Geocoder.isPresent()) {
                    @Suppress("DEPRECATION")
                    val address = Geocoder(activity, Locale.getDefault())
                        .getFromLocation(args.latitude, args.longitude, 1)
                        ?.firstOrNull()
                    val placeName = address?.getAddressLine(0)?.trim().orEmpty()
                    if (placeName.isNotEmpty()) {
                        result.put("placeName", placeName)
                    }
                }
                activity.runOnUiThread { invoke.resolve(result) }
            } catch (error: Exception) {
                activity.runOnUiThread {
                    invoke.reject(error.message ?: "Android 系统无法解析地点名称")
                }
            }
        }.start()
    }
}
