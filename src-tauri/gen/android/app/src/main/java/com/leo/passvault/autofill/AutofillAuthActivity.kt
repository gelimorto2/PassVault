package com.leo.passvault.autofill

import android.app.Activity
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.service.autofill.Dataset
import android.service.autofill.FillResponse
import android.view.autofill.AutofillId
import android.view.autofill.AutofillManager
import android.view.autofill.AutofillValue
import android.widget.Toast
import androidx.annotation.RequiresApi
import androidx.appcompat.app.AppCompatActivity
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Shown over the app being filled: Samsung's fingerprint sheet (Android
 * BiometricPrompt) unlocks the same Keystore-protected secret the app's
 * fingerprint unlock uses, the vault is decrypted in Rust via JNI, and the
 * matching logins are handed back to the autofill framework.
 */
@RequiresApi(Build.VERSION_CODES.O)
class AutofillAuthActivity : AppCompatActivity() {
    companion object {
        const val EXTRA_DOMAIN = "pv_domain"
        const val EXTRA_PACKAGE = "pv_package"
        const val EXTRA_USER_ID = "pv_user_id"
        const val EXTRA_PASS_ID = "pv_pass_id"
        const val EXTRA_INLINE_SPEC = "pv_inline_spec"
        private const val KEY_ALIAS = "passvault_biometric_key" // same key as BiometricPlugin
        private const val GCM_IV_LEN = 12
        private const val MAX_FALLBACK = 40
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val file = File(filesDir, "biometric_secret.bin")
        if (!file.exists()) {
            return fail("Open PassVault → Settings and turn on Fingerprint unlock to use autofill.")
        }
        if (BiometricManager.from(this).canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG) != BiometricManager.BIOMETRIC_SUCCESS) {
            return fail("No fingerprint available on this phone.")
        }
        val cipher = try {
            val bytes = file.readBytes()
            val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
            val key = ks.getKey(KEY_ALIAS, null) as? SecretKey ?: return fail("Re-enable fingerprint unlock in PassVault.")
            Cipher.getInstance("AES/GCM/NoPadding").apply {
                init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, bytes.copyOfRange(0, GCM_IV_LEN)))
            }
        } catch (e: KeyPermanentlyInvalidatedException) {
            return fail("Fingerprints changed — open PassVault, unlock with your password and turn fingerprint unlock back on.")
        } catch (e: Exception) {
            return fail("Couldn't start fingerprint check: ${e.message}")
        }

        val prompt = BiometricPrompt(this, ContextCompat.getMainExecutor(this), object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                val c = result.cryptoObject?.cipher ?: return fail("Fingerprint check failed.")
                val bytes = file.readBytes()
                val payload = try {
                    String(c.doFinal(bytes.copyOfRange(GCM_IV_LEN, bytes.size)), Charsets.UTF_8)
                } catch (e: Exception) {
                    return fail("Couldn't read the saved key: ${e.message}")
                }
                // Argon2 takes ~1 s: keep it off the UI thread.
                Thread {
                    val res = NativeVault.load(applicationInfo.dataDir, payload)
                    runOnUiThread { deliver(res) }
                }.start()
            }

            override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                setResult(Activity.RESULT_CANCELED)
                finish()
            }
        })
        val info = BiometricPrompt.PromptInfo.Builder()
            .setTitle("Fill with PassVault")
            .setSubtitle(intent.getStringExtra(EXTRA_DOMAIN) ?: intent.getStringExtra(EXTRA_PACKAGE) ?: "")
            .setNegativeButtonText("Cancel")
            .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG)
            .build()
        prompt.authenticate(info, BiometricPrompt.CryptoObject(cipher))
    }

    private fun deliver(res: Result<List<NativeVault.Login>>) {
        val logins = res.getOrElse { return fail(it.message ?: "Couldn't open the vault.") }
        val domain = intent.getStringExtra(EXTRA_DOMAIN)
        val pkg = intent.getStringExtra(EXTRA_PACKAGE) ?: ""
        @Suppress("DEPRECATION") val userId = intent.getParcelableExtra<AutofillId>(EXTRA_USER_ID)
        @Suppress("DEPRECATION") val passId = intent.getParcelableExtra<AutofillId>(EXTRA_PASS_ID)
        val matched = logins.filter { Matcher.matches(it, domain, pkg) }
        val list = (if (matched.isNotEmpty()) matched else logins.sortedBy { it.name.lowercase() }.take(MAX_FALLBACK))
        if (list.isEmpty()) return fail("Your vault has no logins yet.")

        val inlineSpec = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            @Suppress("DEPRECATION") intent.getParcelableExtra<android.widget.inline.InlinePresentationSpec>(EXTRA_INLINE_SPEC)
        } else null

        val response = FillResponse.Builder()
        for (l in list) {
            val pres = Presentations.row(this, l.name, l.username.ifEmpty { Matcher.host(l.url) })
            @Suppress("DEPRECATION")
            val ds = Dataset.Builder(pres)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && inlineSpec != null) {
                Presentations.inline(this, inlineSpec, l.name, l.username)?.let { ds.setInlinePresentation(it) }
            }
            var any = false
            if (userId != null && l.username.isNotEmpty()) {
                @Suppress("DEPRECATION") ds.setValue(userId, AutofillValue.forText(l.username), pres); any = true
            }
            if (passId != null) {
                @Suppress("DEPRECATION") ds.setValue(passId, AutofillValue.forText(l.password), pres); any = true
            }
            if (any) response.addDataset(ds.build())
        }
        val reply = Intent().putExtra(AutofillManager.EXTRA_AUTHENTICATION_RESULT, response.build())
        setResult(Activity.RESULT_OK, reply)
        finish()
    }

    private fun fail(msg: String) {
        Toast.makeText(this, msg, Toast.LENGTH_LONG).show()
        setResult(Activity.RESULT_CANCELED)
        finish()
    }
}
