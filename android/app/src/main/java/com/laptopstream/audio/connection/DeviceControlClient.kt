package com.laptopstream.audio.connection

import android.util.Log
import com.laptopstream.audio.connection.StreamClient.ConnectionState
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.util.concurrent.TimeUnit

/**
 * Long-lived receiver-control socket. It is deliberately separate from the speaker stream.
 * Closing or stopping Speaker must never close this connection.
 */
class DeviceControlClient(
    private val onStateChanged: (ConnectionState, String) -> Unit
) {
    private val client = OkHttpClient.Builder()
        .pingInterval(5, TimeUnit.SECONDS)
        .retryOnConnectionFailure(true)
        .build()

    private var socket: WebSocket? = null
    private var manualDisconnect = false

    fun connect(host: String, port: Int, deviceName: String) {
        // Close a previous socket without publishing DISCONNECTED; the selected
        // receiver must remain selected while a new control handshake starts.
        manualDisconnect = true
        try { socket?.close(1000, "Replacing control connection") } catch (_: Exception) { }
        socket = null
        manualDisconnect = false
        onStateChanged(ConnectionState.CONNECTING, "Connecting to $host...")
        val request = Request.Builder()
            .url("ws://$host:$port/ws/control")
            .build()

        socket = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                val safeName = deviceName.replace(Regex("[^A-Za-z0-9 _.-]"), "").take(48)
                webSocket.send("DEVICE_NAME:$safeName")
                onStateChanged(ConnectionState.CONNECTED, "Connected to $host")
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                if (text.startsWith("ERROR:DEVICE_ALREADY_CONNECTED")) {
                    onStateChanged(ConnectionState.ERROR, "Another receiver is already connected")
                    webSocket.close(1008, "Receiver limit reached")
                }
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                if (!manualDisconnect) {
                    onStateChanged(ConnectionState.DISCONNECTED, "Device connection closed")
                }
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                val message = when (response?.code) {
                    409 -> "Another receiver is already connected"
                    else -> "Device connection failed: ${t.message ?: "Unknown error"}"
                }
                onStateChanged(ConnectionState.ERROR, message)
                Log.w("DeviceControlClient", message, t)
            }
        })
    }

    fun disconnect() {
        manualDisconnect = true
        try {
            socket?.send("DISCONNECT")
            socket?.close(1000, "User disconnected")
        } catch (_: Exception) {
        }
        socket = null
        onStateChanged(ConnectionState.DISCONNECTED, "Disconnected")
    }
}
