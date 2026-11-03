package com.leo.passvault.biometric

import android.app.Activity
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

private const val KEY_ALIAS = "passvault_biometric_key"
private const val KEYSTORE = "AndroidKeyStore"
private const val GCM_IV_LEN = 12
private const val GCM_TAG_LEN_BITS = 128

@InvokeArg
class SaveSecretArgs {
    lateinit var secret: String
}

// Not managed by `tauri android init` — a small hand-written plugin local to
// this app, safe from regeneration (same pattern as StorageAccessPlugin).
@TauriPlugin
class BiometricPlugin(private val activity: Activity) : Plugin(activity) {
    private val fragmentActivity = activity as androidx.fragment.app.FragmentActivity
    private fun storeFile(): File = File(activity.filesDir, "biometric_secret.bin")

    private fun keyStore(): KeyStore =
        KeyStore.getInstance(KEYSTORE).apply { load(null) }

    private fun getOrCreateKey(): SecretKey {
        val ks = keyStore()
        (ks.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }

        val keyGenerator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        val spec = KeyGenParameterSpec.Builder(
            KEY_ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setUserAuthenticationRequired(true)
            .setInvalidatedByBiometricEnrollment(true)
            .build()
        keyGenerator.init(spec)
        return keyGenerator.generateKey()
    }

    @Command
    fun isAvailable(invoke: Invoke) {
        val manager = BiometricManager.from(activity)
        val result = manager.canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG)
        val ret = JSObject()
        ret.put("available", result == BiometricManager.BIOMETRIC_SUCCESS)
        ret.put("hasSavedSecret", storeFile().exists())
        invoke.resolve(ret)
    }

    @Command
    fun clearSecret(invoke: Invoke) {
        storeFile().delete()
        try {
            keyStore().deleteEntry(KEY_ALIAS)
        } catch (_: Exception) {
            // no key yet, nothing to clear
        }
        invoke.resolve()
    }

    @Command
    fun saveSecret(invoke: Invoke) {
        val args = invoke.parseArgs(SaveSecretArgs::class.java)
        activity.runOnUiThread {
            try {
                val cipher = Cipher.getInstance("AES/GCM/NoPadding")
                cipher.init(Cipher.ENCRYPT_MODE, getOrCreateKey())
                val prompt = buildPrompt(invoke) { result ->
                    try {
                        val c = result.cryptoObject?.cipher
                            ?: throw IllegalStateException("no cipher")
                        val ciphertext = c.doFinal(args.secret.toByteArray(Charsets.UTF_8))
                        val iv = c.iv
                        storeFile().writeBytes(iv + ciphertext)
                        invoke.resolve()
                    } catch (e: Exception) {
                        invoke.reject("Could not save secret: ${e.message}")
                    }
                }
                prompt.authenticate(
                    promptInfo("Enable fingerprint unlock"),
                    BiometricPrompt.CryptoObject(cipher)
                )
            } catch (e: Exception) {
                invoke.reject("Could not start biometric enrollment: ${e.message}")
            }
        }
    }

    @Command
    fun loadSecret(invoke: Invoke) {
        val file = storeFile()
        if (!file.exists()) {
            invoke.reject("No fingerprint-saved secret yet.")
            return
        }
        activity.runOnUiThread {
            try {
                val bytes = file.readBytes()
                val iv = bytes.copyOfRange(0, GCM_IV_LEN)
                val ciphertext = bytes.copyOfRange(GCM_IV_LEN, bytes.size)

                val cipher = Cipher.getInstance("AES/GCM/NoPadding")
                cipher.init(
                    Cipher.DECRYPT_MODE,
                    getOrCreateKey(),
                    GCMParameterSpec(GCM_TAG_LEN_BITS, iv)
                )
                val prompt = buildPrompt(invoke) { result ->
                    try {
                        val c = result.cryptoObject?.cipher
                            ?: throw IllegalStateException("no cipher")
                        val plaintext = c.doFinal(ciphertext)
                        val ret = JSObject()
                        ret.put("secret", String(plaintext, Charsets.UTF_8))
                        invoke.resolve(ret)
                    } catch (e: Exception) {
                        invoke.reject("Could not unlock: ${e.message}")
                    }
                }
                prompt.authenticate(
                    promptInfo("Unlock PassVault"),
                    BiometricPrompt.CryptoObject(cipher)
                )
            } catch (e: Exception) {
                // Most commonly: key was invalidated (new fingerprint enrolled).
                storeFile().delete()
                invoke.reject("Fingerprint unlock is no longer valid; use your password once to re-enable it.")
            }
        }
    }

    private fun buildPrompt(
        invoke: Invoke,
        onSuccess: (BiometricPrompt.AuthenticationResult) -> Unit
    ): BiometricPrompt {
        val executor = ContextCompat.getMainExecutor(activity)
        return BiometricPrompt(
            fragmentActivity,
            executor,
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    onSuccess(result)
                }

                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                    invoke.reject("Biometric error: $errString")
                }

                override fun onAuthenticationFailed() {
                    // A single failed attempt (e.g. wrong finger); the
                    // system prompt stays open for retries, so don't
                    // reject the invoke here.
                }
            }
        )
    }

    private fun promptInfo(title: String): BiometricPrompt.PromptInfo =
        BiometricPrompt.PromptInfo.Builder()
            .setTitle(title)
            .setNegativeButtonText("Cancel")
            .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG)
            .build()
}
