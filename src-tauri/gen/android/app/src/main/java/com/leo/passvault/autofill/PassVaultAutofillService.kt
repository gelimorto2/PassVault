package com.leo.passvault.autofill

import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.os.CancellationSignal
import android.service.autofill.AutofillService
import android.service.autofill.FillCallback
import android.service.autofill.FillRequest
import android.service.autofill.FillResponse
import android.service.autofill.SaveCallback
import android.service.autofill.SaveRequest
import android.view.autofill.AutofillId
import android.widget.RemoteViews
import androidx.annotation.RequiresApi
import com.leo.passvault.R

/**
 * Samsung/Android autofill: appears in Settings → General management →
 * Passwords and autofill → Autofill service. Offers a locked "PassVault"
 * entry on login fields; tapping it opens [AutofillAuthActivity] for a
 * fingerprint check, which then supplies the matching logins.
 * Nothing is decrypted until the fingerprint succeeds.
 */
@RequiresApi(Build.VERSION_CODES.O)
class PassVaultAutofillService : AutofillService() {
    override fun onFillRequest(request: FillRequest, cancellationSignal: CancellationSignal, callback: FillCallback) {
        try {
            val structure = request.fillContexts.lastOrNull()?.structure ?: return callback.onSuccess(null)
            val p = FieldParser(structure).parse()
            if (p.packageName == packageName) return callback.onSuccess(null)
            val ids: Array<AutofillId> = listOfNotNull(p.usernameId, p.passwordId).toTypedArray()
            if (p.passwordId == null && p.usernameId == null) return callback.onSuccess(null)

            val intent = Intent(this, AutofillAuthActivity::class.java).apply {
                putExtra(AutofillAuthActivity.EXTRA_DOMAIN, p.webDomain)
                putExtra(AutofillAuthActivity.EXTRA_PACKAGE, p.packageName)
                putExtra(AutofillAuthActivity.EXTRA_USER_ID, p.usernameId)
                putExtra(AutofillAuthActivity.EXTRA_PASS_ID, p.passwordId)
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    request.inlineSuggestionsRequest?.inlinePresentationSpecs?.firstOrNull()?.let {
                        putExtra(AutofillAuthActivity.EXTRA_INLINE_SPEC, it)
                    }
                }
            }
            val flags = PendingIntent.FLAG_CANCEL_CURRENT or
                (if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) PendingIntent.FLAG_MUTABLE else 0)
            val sender = PendingIntent.getActivity(this, request.id, intent, flags).intentSender

            val label = p.webDomain ?: appLabel(p.packageName)
            val presentation = Presentations.row(this, "PassVault", "Unlock to fill · $label")
            val builder = FillResponse.Builder()
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                val spec = request.inlineSuggestionsRequest?.inlinePresentationSpecs?.firstOrNull()
                val inline = spec?.let { Presentations.inline(this, it, "PassVault", "Unlock", sender) }
                if (inline != null) builder.setAuthentication(ids, sender, presentation, inline)
                else builder.setAuthentication(ids, sender, presentation)
            } else {
                builder.setAuthentication(ids, sender, presentation)
            }
            callback.onSuccess(builder.build())
        } catch (e: Exception) {
            callback.onSuccess(null)
        }
    }

    // We don't offer "save this password?" yet: add new logins in the app.
    override fun onSaveRequest(request: SaveRequest, callback: SaveCallback) {
        callback.onSuccess()
    }

    private fun appLabel(pkg: String): String = try {
        packageManager.getApplicationLabel(packageManager.getApplicationInfo(pkg, 0)).toString()
    } catch (e: Exception) {
        pkg
    }
}

object Presentations {
    fun row(ctx: android.content.Context, title: String, sub: String): RemoteViews =
        RemoteViews(ctx.packageName, R.layout.autofill_item).apply {
            setTextViewText(R.id.af_title, title)
            setTextViewText(R.id.af_sub, sub)
        }

    @RequiresApi(Build.VERSION_CODES.R)
    fun inline(
        ctx: android.content.Context,
        spec: android.widget.inline.InlinePresentationSpec,
        title: String,
        sub: String,
        attribution: android.content.IntentSender? = null,
    ): android.service.autofill.InlinePresentation? = try {
        if (!androidx.autofill.inline.UiVersions.getVersions(spec.style).contains(androidx.autofill.inline.UiVersions.INLINE_UI_VERSION_1)) {
            null
        } else {
            val pi = attribution?.let {
                PendingIntent.getActivity(ctx, 0, Intent(), PendingIntent.FLAG_IMMUTABLE)
            } ?: PendingIntent.getActivity(ctx, 0, Intent(), PendingIntent.FLAG_IMMUTABLE)
            val content = androidx.autofill.inline.v1.InlineSuggestionUi.newContentBuilder(pi)
                .setTitle(title)
                .setSubtitle(sub)
                .setStartIcon(android.graphics.drawable.Icon.createWithResource(ctx, R.drawable.ic_tile))
                .build()
            android.service.autofill.InlinePresentation(content.slice, spec, false)
        }
    } catch (e: Exception) {
        null
    }
}
