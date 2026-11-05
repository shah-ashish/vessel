package com.laptopstream.audio.discovery

import android.content.Context
import android.net.DhcpInfo
import android.net.wifi.WifiManager
import android.util.Log
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
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

        fun getLastSavedIp(context: Context): String? {
            val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            return prefs.getString(KEY_LAST_IP, null)
        }

        fun saveLastIp(context: Context, ip: String) {
            val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            prefs.edit().putString(KEY_LAST_IP, ip).apply()
        }
    }

    private val _discoveredDevice = MutableStateFlow<DiscoveredDevice?>(null)
    val discoveredDevice: StateFlow<DiscoveredDevice?> = _discoveredDevice.asStateFlow()

    private val _isScanning = MutableStateFlow(false)
    val isScanning: StateFlow<Boolean> = _isScanning.asStateFlow()

    private val httpClient = OkHttpClient.Builder()
        .connectTimeout(800, TimeUnit.MILLISECONDS)
        .readTimeout(800, TimeUnit.MILLISECONDS)
        .build()

    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    /**
     * Scans for the Windows laptop via Hotspot Gateway check and UDP Broadcast.
     */
    fun startDiscovery(onResult: (DiscoveredDevice?) -> Unit = {}) {
        if (_isScanning.value) return
        _isScanning.value = true

        scope.launch {
            var foundDevice: DiscoveredDevice? = null

            // 1. Immediately check last known IP as candidate
            val lastIp = getLastSavedIp(context)
            if (lastIp != null && verifyServerAt(lastIp)) {
                foundDevice = DiscoveredDevice("Windows Laptop (Saved)", lastIp, DEFAULT_PORT)
                _discoveredDevice.value = foundDevice
                _isScanning.value = false
                withContext(Dispatchers.Main) { onResult(foundDevice) }
                return@launch
            }

            // 2. Check Wi-Fi / Hotspot Gateway IP
            val gatewayIp = getWifiGatewayIp()
            if (gatewayIp != null && gatewayIp != "0.0.0.0") {
                Log.i(TAG, "Probing Wi-Fi Hotspot Gateway: $gatewayIp")
                if (verifyServerAt(gatewayIp)) {
                    Log.i(TAG, "Found laptop at hotspot gateway: $gatewayIp")
                    saveLastIp(context, gatewayIp)
                    foundDevice = DiscoveredDevice("Windows Laptop (Hotspot)", gatewayIp, DEFAULT_PORT)
                    _discoveredDevice.value = foundDevice
                    _isScanning.value = false
                    withContext(Dispatchers.Main) { onResult(foundDevice) }
                    return@launch
                }
            }

            // 3. UDP Broadcast Discovery Probe
            val udpDevice = probeUdpBroadcast()
            if (udpDevice != null) {
                foundDevice = udpDevice
                _discoveredDevice.value = foundDevice
                saveLastIp(context, udpDevice.ip)
            }

            _isScanning.value = false
            withContext(Dispatchers.Main) { onResult(foundDevice) }
        }
    }

    private fun verifyServerAt(ip: String, port: Int = DEFAULT_PORT): Boolean {
        return try {
            val request = Request.Builder()
                .url("http://$ip:$port/api/info")
                .build()
            val response = httpClient.newCall(request).execute()
            val body = response.body?.string() ?: ""
            response.isSuccessful && body.contains("laptop-audio-stream")
        } catch (_: Exception) {
            false
        }
    }

    private fun probeUdpBroadcast(): DiscoveredDevice? {
        return try {
            val socket = DatagramSocket()
            socket.broadcast = true
            socket.soTimeout = 1200

            val message = "LAUD_DISCOVER".toByteArray()
            val broadcastAddr = InetAddress.getByName("255.255.255.255")
            val packet = DatagramPacket(message, message.size, broadcastAddr, UDP_DISCOVERY_PORT)
            socket.send(packet)

            val receiveBuf = ByteArray(256)
            val receivePacket = DatagramPacket(receiveBuf, receiveBuf.size)
            socket.receive(receivePacket)

            val reply = String(receivePacket.data, 0, receivePacket.length)
            socket.close()

            if (reply.startsWith("LAUD_HERE")) {
                val senderIp = receivePacket.address.hostAddress ?: ""
                Log.i(TAG, "UDP Discovery received from: $senderIp")
                if (senderIp.isNotEmpty()) {
                    return DiscoveredDevice("Windows Laptop (LAN)", senderIp, DEFAULT_PORT)
                }
            }
            null
        } catch (e: Exception) {
            Log.d(TAG, "UDP probe finished: ${e.message}")
            null
        }
    }

    private fun getWifiGatewayIp(): String? {
        return try {
            val wifiManager = context.applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            val dhcpInfo: DhcpInfo = wifiManager?.dhcpInfo ?: return null
            val gateway = dhcpInfo.gateway
            if (gateway == 0) return null

            // Convert little-endian int IP to dotted-decimal string
            "${gateway and 0xFF}.${gateway shr 8 and 0xFF}.${gateway shr 16 and 0xFF}.${gateway shr 24 and 0xFF}"
        } catch (_: Exception) {
            null
        }
    }
}
