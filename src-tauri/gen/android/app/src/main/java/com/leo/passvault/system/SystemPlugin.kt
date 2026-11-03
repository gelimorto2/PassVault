package com.leo.passvault.system

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.view.autofill.AutofillManager
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

// Hand-written local plugin (safe from `tauri android init` regeneration).
// Autofill status + jumping to the system "Autofill service" picker
// (Samsung: Settings → General management → Passwords and autofill).
@TauriPlugin
class SystemPlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun autofillStatus(invoke: Invoke) {
        val ret = JSObject()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val afm = activity.getSystemService(AutofillManager::class.java)
            ret.put("supported", afm?.isAutofillSupported == true)
            ret.put("enabled", afm?.hasEnabledAutofillServices() == true)
        } else {
            ret.put("supported", false)
            ret.put("enabled", false)
        }
        invoke.resolve(ret)
    }

    @Command
    fun openAutofillSettings(invoke: Invoke) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            try {
                val i = Intent(Settings.ACTION_REQUEST_SET_AUTOFILL_SERVICE)
                i.data = Uri.parse("package:" + activity.packageName)
                activity.startActivity(i)
            } catch (e: Exception) {
                // Some OEM builds lack the direct picker; fall back to settings.
                activity.startActivity(Intent(Settings.ACTION_SETTINGS))
            }
        }
        invoke.resolve()
    }
}
