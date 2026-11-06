package com.laptopstream.audio.connection

import android.content.Context
import android.net.DhcpInfo
import android.net.wifi.WifiManager
import android.util.Log
import kotlinx.coroutines.*
import okhttp3.OkHttpClient
import okhttp3.Request
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.util.concurrent.TimeUnit

data class DiscoveredDevice(
    val name: String,
    val ip: String,
    val port: Int = 8080,
    val isAutoDetected: Boolean = true
)

class DeviceDiscoveryManager(private val context: Context) {

    companion object {
        private const val TAG = "DiscoveryManager"
        private const val PREFS_NAME = "audio_stream_prefs"
        private const val KEY_LAST_IP = "last_connected_ip"
        const val DEFAULT_PORT = 8080
        const val UDP_DISCOVERY_PORT = 8082

        fun getLastSavedIp(context: Context): String? =
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .getString(KEY_LAST_IP, null)

        fun saveLastIp(context: Context, ip: String) {
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .edit().putString(KEY_LAST_IP, ip).apply()
        }
    }

    private val httpClient = OkHttpClient.Builder()
        .connectTimeout(800, TimeUnit.MILLISECONDS)
        .readTimeout(800, TimeUnit.MILLISECONDS)
        .build()
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())
    @Volatile private var scanning = false

    /** Discovers all responding Vessel hosts and returns a de-duplicated list. */
    fun startDiscovery(onResult: (List<DiscoveredDevice>) -> Unit) {
        if (scanning) return
        scanning = true
        scope.launch {
            val results = linkedMapOf<String, DiscoveredDevice>()
            try {
                val candidates = linkedSetOf<String>()
                getLastSavedIp(context)?.let { candidates.add(it) }
                getWifiGatewayIp()?.takeIf { it != "0.0.0.0" }?.let { candidates.add(it) }

                candidates.forEach { ip ->
                    if (verifyServerAt(ip)) {
                        results[ip] = DiscoveredDevice(
                            if (ip == getLastSavedIp(context)) "Windows Laptop (Saved)" else "Vessel Server",
                            ip
                        )
                    }
                }

                probeUdpBroadcastAll().forEach { device ->
                    if (device.ip !in results) results[device.ip] = device
                }
            } catch (e: Exception) {
                Log.w(TAG, "Discovery failed: ${e.message}", e)
            } finally {
                scanning = false
                withContext(Dispatchers.Main) { onResult(results.values.toList()) }
            }
        }
    }

    private fun verifyServerAt(ip: String): Boolean = try {
        val request = Request.Builder().url("http://$ip:$DEFAULT_PORT/api/info").build()
        httpClient.newCall(request).execute().use { response ->
            response.isSuccessful && (response.body?.string() ?: "").contains("laptop-audio-stream")
        }
    } catch (_: Exception) {
        false
    }

    private fun probeUdpBroadcastAll(): List<DiscoveredDevice> {
        val found = linkedMapOf<String, DiscoveredDevice>()
        val socket = DatagramSocket()
        try {
            socket.broadcast = true
            socket.soTimeout = 350
            val message = "LAUD_DISCOVER".toByteArray()
            val broadcastAddr = InetAddress.getByName("255.255.255.255")
            socket.send(DatagramPacket(message, message.size, broadcastAddr, UDP_DISCOVERY_PORT))

            val deadline = System.currentTimeMillis() + 1600
            while (System.currentTimeMillis() < deadline) {
                try {
                    val buffer = ByteArray(256)
                    val packet = DatagramPacket(buffer, buffer.size)
                    socket.receive(packet)
                    val reply = String(packet.data, 0, packet.length)
                    if (reply.startsWith("LAUD_HERE")) {
                        val ip = packet.address.hostAddress.orEmpty()
                        if (ip.isNotBlank()) {
                            found[ip] = DiscoveredDevice("Vessel Server", ip)
                        }
                    }
                } catch (_: java.net.SocketTimeoutException) {
                    // Continue until the discovery window expires to collect multiple hosts.
                }
            }
        } catch (e: Exception) {
            Log.d(TAG, "UDP discovery finished: ${e.message}")
        } finally {
            socket.close()
        }
        return found.values.toList()
    }

    private fun getWifiGatewayIp(): String? {
        return try {
            val manager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
                ?: return null
            val dhcp: DhcpInfo = manager.dhcpInfo ?: return null
            val gateway = dhcp.gateway
            if (gateway == 0) null else
                "${gateway and 0xFF}.${gateway shr 8 and 0xFF}.${gateway shr 16 and 0xFF}.${gateway shr 24 and 0xFF}"
        } catch (_: Exception) {
            null
        }
    }
}
