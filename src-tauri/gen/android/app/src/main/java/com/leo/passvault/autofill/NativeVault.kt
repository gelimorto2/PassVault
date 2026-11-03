package com.leo.passvault.autofill

import org.json.JSONObject

/** JNI bridge into the Rust vault code (src-tauri/src/autofill_jni.rs). */
object NativeVault {
    init {
        System.loadLibrary("tauri_app_lib")
    }

    @JvmStatic
    external fun decrypt(configDir: String, payloadJson: String): String

    data class Login(val name: String, val username: String, val password: String, val url: String)

    fun load(configDir: String, payloadJson: String): Result<List<Login>> {
        val obj = JSONObject(decrypt(configDir, payloadJson))
        if (!obj.optBoolean("ok")) {
            return Result.failure(IllegalStateException(obj.optString("error", "Couldn't open the vault.")))
        }
        val arr = obj.getJSONArray("accounts")
        val out = ArrayList<Login>(arr.length())
        for (i in 0 until arr.length()) {
            val a = arr.getJSONObject(i)
            out.add(Login(a.optString("name"), a.optString("username"), a.optString("password"), a.optString("url")))
        }
        return Result.success(out)
    }
}
