package com.laptopstream.audio.update

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import com.laptopstream.audio.BuildConfig
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.io.File
import java.security.MessageDigest
import java.util.concurrent.TimeUnit

data class VesselUpdate(
    val version: String,
    val notes: String,
    val apkUrl: String,
    val checksumsUrl: String,
    val apkName: String
)

class UpdateChecker(private val context: Context) {
    private val client = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(30, TimeUnit.SECONDS)
        .build()

    /** Called only from MainActivity.onCreate: there is intentionally no periodic polling. */
    fun checkAtStartup(): VesselUpdate? {
        val request = Request.Builder()
            .url("https://api.github.com/repos/shah-ashish/vessel/releases/latest")
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .build()
        return try {
            client.newCall(request).execute().use { response ->
                if (!response.isSuccessful) return null
                val release = JSONObject(response.body?.string() ?: return null)
                val tag = release.optString("tag_name").removePrefix("v")
                if (compareVersions(tag, BuildConfig.VERSION_NAME) <= 0) return null

                val assets = release.optJSONArray("assets") ?: return null
                var apkUrl: String? = null
                var apkName: String? = null
                var checksumsUrl: String? = null
                for (index in 0 until assets.length()) {
                    val asset = assets.getJSONObject(index)
                    val name = asset.optString("name")
                    val url = asset.optString("browser_download_url")
                    if (name.startsWith("Vessel-Android-") && name.endsWith(".apk")) {
                        apkName = name
                        apkUrl = url
                    } else if (name == "SHA256SUMS.txt") {
                        checksumsUrl = url
                    }
                }
                if (apkUrl.isNullOrBlank() || apkName.isNullOrBlank() || checksumsUrl.isNullOrBlank()) return null
                VesselUpdate(
                    version = tag,
                    notes = release.optString("body", "A new version of Vessel is available."),
                    apkUrl = apkUrl,
                    checksumsUrl = checksumsUrl,
                    apkName = apkName
                )
            }
        } catch (_: Exception) {
            null
        }
    }

    /** Download, verify SHA-256, and hand the APK to Android's package installer. */
    fun downloadAndInstall(update: VesselUpdate) {
        Thread {
            try {
                val folder = File(context.cacheDir, "updates").apply { mkdirs() }
                val apkFile = File(folder, update.apkName)
                val checksumsFile = File(folder, "SHA256SUMS.txt")
                download(update.apkUrl, apkFile)
                download(update.checksumsUrl, checksumsFile)

                val expected = checksumsFile.readLines()
                    .firstOrNull { it.trim().endsWith(update.apkName) }
                    ?.trim()?.substringBefore(Regex("\\s+"))
                    ?: throw IllegalStateException("No checksum for ${update.apkName}")
                val actual = sha256(apkFile)
                require(expected.equals(actual, ignoreCase = true)) { "APK checksum verification failed" }

                val install = {
                    val authority = "${context.packageName}.fileprovider"
                    val uri = FileProvider.getUriForFile(context, authority, apkFile)
                    val intent = Intent(Intent.ACTION_VIEW).apply {
                        setDataAndType(uri, "application/vnd.android.package-archive")
                        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    }
                    context.startActivity(intent)
                }

                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
                    !context.packageManager.canRequestPackageInstalls()
                ) {
                    val settingsIntent = Intent(
                        Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                        Uri.parse("package:${context.packageName}")
                    ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    context.startActivity(settingsIntent)
                } else {
                    install()
                }
            } catch (error: Exception) {
                android.util.Log.e("VesselUpdateChecker", "Update failed: ${error.message}", error)
            }
        }.start()
    }

    private fun download(url: String, target: File) {
        val request = Request.Builder().url(url).build()
        client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) throw IllegalStateException("Download failed: HTTP ${response.code}")
            val body = response.body ?: throw IllegalStateException("Empty download")
            target.outputStream().use { output -> body.byteStream().copyTo(output) }
        }
    }

    private fun sha256(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input ->
            val buffer = ByteArray(8192)
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                digest.update(buffer, 0, read)
            }
        }
        return digest.digest().joinToString("") { "%02x".format(it) }
    }

    private fun compareVersions(left: String, right: String): Int {
        val a = left.split(".").map { it.toIntOrNull() ?: 0 }
        val b = right.split(".").map { it.toIntOrNull() ?: 0 }
        for (index in 0 until maxOf(a.size, b.size)) {
            val difference = (a.getOrElse(index) { 0 }).compareTo(b.getOrElse(index) { 0 })
            if (difference != 0) return difference
        }
        return 0
    }
}
