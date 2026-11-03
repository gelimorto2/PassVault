package com.leo.passvault.autofill

import android.app.assist.AssistStructure
import android.os.Build
import android.text.InputType
import android.view.View
import android.view.autofill.AutofillId
import androidx.annotation.RequiresApi

/** Finds username/password fields and the site/app they belong to. */
@RequiresApi(Build.VERSION_CODES.O)
class FieldParser(private val structure: AssistStructure) {
    var usernameId: AutofillId? = null
    var passwordId: AutofillId? = null
    var webDomain: String? = null
    val packageName: String = structure.activityComponent.packageName
    private var lastTextId: AutofillId? = null

    fun parse(): FieldParser {
        for (i in 0 until structure.windowNodeCount) walk(structure.getWindowNodeAt(i).rootViewNode)
        // Typical login form: the text field right before the password one.
        if (usernameId == null && passwordId != null) usernameId = lastTextBeforePassword
        return this
    }

    private var lastTextBeforePassword: AutofillId? = null

    private fun walk(node: AssistStructure.ViewNode) {
        node.webDomain?.takeIf { it.isNotBlank() }?.let { if (webDomain == null) webDomain = it }
        val id = node.autofillId
        if (id != null && node.autofillType == View.AUTOFILL_TYPE_TEXT) {
            when (classify(node)) {
                Kind.PASSWORD -> if (passwordId == null) {
                    passwordId = id
                    lastTextBeforePassword = lastTextId
                }
                Kind.USERNAME -> if (usernameId == null) usernameId = id
                Kind.OTHER_TEXT -> lastTextId = id
                Kind.NONE -> {}
            }
        }
        for (i in 0 until node.childCount) walk(node.getChildAt(i))
    }

    private enum class Kind { USERNAME, PASSWORD, OTHER_TEXT, NONE }

    private fun classify(n: AssistStructure.ViewNode): Kind {
        val hints = n.autofillHints?.map { it.lowercase() } ?: emptyList()
        if (hints.any { it.contains("password") }) return Kind.PASSWORD
        if (hints.any { it.contains("username") || it.contains("email") || it.contains("phone") }) return Kind.USERNAME

        val html = n.htmlInfo
        if (html != null && html.tag.equals("input", true)) {
            val attrs = html.attributes?.associate { it.first.lowercase() to (it.second ?: "").lowercase() } ?: emptyMap()
            val type = attrs["type"] ?: "text"
            if (type == "password") return Kind.PASSWORD
            if (type == "hidden" || type == "submit" || type == "checkbox" || type == "radio") return Kind.NONE
            val blob = listOf(attrs["name"], attrs["id"], attrs["autocomplete"], attrs["placeholder"]).joinToString(" ")
            if (type == "email" || LOGIN_WORDS.any { blob.contains(it) }) return Kind.USERNAME
            return Kind.OTHER_TEXT
        }

        val it = n.inputType
        val variation = it and InputType.TYPE_MASK_VARIATION
        if (it and InputType.TYPE_MASK_CLASS == InputType.TYPE_CLASS_TEXT) {
            if (variation == InputType.TYPE_TEXT_VARIATION_PASSWORD ||
                variation == InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD ||
                variation == InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD) return Kind.PASSWORD
            if (variation == InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS ||
                variation == InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS) return Kind.USERNAME
        }
        if (it and InputType.TYPE_MASK_CLASS == InputType.TYPE_CLASS_NUMBER &&
            variation == InputType.TYPE_NUMBER_VARIATION_PASSWORD) return Kind.PASSWORD

        val blob = listOfNotNull(n.idEntry, n.hint, n.contentDescription?.toString()).joinToString(" ").lowercase()
        if (blob.contains("pass") || blob.contains("mot de passe") || blob.contains("mdp")) return Kind.PASSWORD
        if (LOGIN_WORDS.any { blob.contains(it) }) return Kind.USERNAME
        return if (n.className?.contains("EditText") == true) Kind.OTHER_TEXT else Kind.NONE
    }

    companion object {
        private val LOGIN_WORDS = listOf("user", "login", "email", "e-mail", "mail", "identifiant", "account", "phone", "username")
    }
}

/** Ranks saved logins against the site (browser) or app package being filled. */
object Matcher {
    private val NOISE = setOf("com", "org", "net", "fr", "android", "app", "apps", "mobile", "www", "m", "client", "lite", "google", "samsung", "sec")

    fun host(url: String): String {
        val u = url.trim().lowercase()
        val noScheme = u.substringAfter("://")
        return noScheme.substringBefore('/').substringBefore(':').removePrefix("www.").removePrefix("m.")
    }

    /** "accounts.google.com" -> "google.com" (good enough for matching). */
    private fun base(h: String): String {
        val p = h.split('.').filter { it.isNotEmpty() }
        return if (p.size >= 2) p.takeLast(2).joinToString(".") else h
    }

    fun matches(login: NativeVault.Login, webDomain: String?, pkg: String): Boolean {
        val h = host(login.url)
        if (webDomain != null && webDomain.isNotBlank()) {
            val d = webDomain.lowercase().removePrefix("www.")
            return h.isNotEmpty() && (d == h || d.endsWith(".$h") || h.endsWith(".$d") || base(d) == base(h))
        }
        if (h == pkg) return true // android://…@com.foo.bar entries from Chrome exports
        val tokens = pkg.lowercase().split('.').filter { it.length >= 3 && it !in NOISE }
        val hay = (h + " " + login.name).lowercase()
        return tokens.any { hay.contains(it) }
    }
}
