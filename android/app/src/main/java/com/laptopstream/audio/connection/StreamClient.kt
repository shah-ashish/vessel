package com.laptopstream.audio.connection

import android.util.Log
import com.laptopstream.audio.features.speaker.JitterBuffer
import com.laptopstream.audio.features.speaker.LaudPacketParser
import com.laptopstream.audio.features.speaker.Protocol
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import okhttp3.*
import okio.ByteString
import java.util.concurrent.TimeUnit
import kotlin.math.min
import kotlin.math.pow

class StreamClient(
    private val jitterBuffer: JitterBuffer
) {
    companion object {
        private const val TAG = "StreamClient"
    }

    enum class ConnectionState {
        DISCONNECTED,
        CONNECTING,
        CONNECTED,
        RECONNECTING,
        ERROR
    }

    private val _connectionState = MutableStateFlow(ConnectionState.DISCONNECTED)
    val connectionState: StateFlow<ConnectionState> = _connectionState.asStateFlow()

    private val _statusMessage = MutableStateFlow("Tap to Listen")
    val statusMessage: StateFlow<String> = _statusMessage.asStateFlow()

    private val parser = LaudPacketParser()
    private val client = OkHttpClient.Builder()
        .pingInterval(5, TimeUnit.SECONDS)
        .retryOnConnectionFailure(true)
        .build()

    private var webSocket: WebSocket? = null
    private var currentHost: String = ""
    private var currentPort: Int = 8080

    private var shouldReconnect = true
    private var reconnectAttempt = 0
    private var reconnectJob: Job? = null
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    val totalPackets: Long get() = parser.totalPackets
    val lostPackets: Long get() = parser.lostPackets

    fun connect(host: String, port: Int = 8080) {
        currentHost = host
        currentPort = port
        shouldReconnect = true
        reconnectAttempt = 0
        reconnectJob?.cancel()
        parser.reset()
        jitterBuffer.clear()

        startSocket()
    }

    private fun startSocket() {
        _connectionState.value = if (reconnectAttempt == 0) ConnectionState.CONNECTING else ConnectionState.RECONNECTING
        _statusMessage.value = if (reconnectAttempt == 0) "Connecting..." else "Reconnecting (attempt $reconnectAttempt)..."

        val request = Request.Builder()
            .url("ws://$currentHost:$currentPort/ws")
            .build()

        webSocket = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                Log.i(TAG, "WebSocket connected. Sending START:LAUD")
                reconnectAttempt = 0
                _connectionState.value = ConnectionState.CONNECTED
                _statusMessage.value = "Streaming Live Audio"
                webSocket.send(Protocol.CMD_START_LAUD)
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                val parsed = parser.parse(bytes.toByteArray())
                if (parsed != null) {
                    val (_, samples) = parsed
                    jitterBuffer.push(samples)
                }
            }

            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                Log.i(TAG, "WebSocket closing: $code / $reason")
                webSocket.close(1000, null)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                Log.i(TAG, "WebSocket closed: $code")
                if (shouldReconnect) {
                    scheduleReconnect()
                } else {
                    _connectionState.value = ConnectionState.DISCONNECTED
                    _statusMessage.value = "Tap to Listen"
                }
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                Log.w(TAG, "WebSocket failure: ${t.message}")
                if (shouldReconnect) {
                    scheduleReconnect()
                } else {
                    _connectionState.value = ConnectionState.ERROR
                    _statusMessage.value = "Connection Error"
                }
            }
        })
    }

    private fun scheduleReconnect() {
        if (!shouldReconnect) return
        reconnectAttempt++
        // Exponential backoff: 1s, 1.5s, 2.25s ... capped at 8s
        val delayMs = min((1000.0 * 1.5.pow((reconnectAttempt - 1).toDouble())).toLong(), 8000L)

        _connectionState.value = ConnectionState.RECONNECTING
        _statusMessage.value = "Reconnecting in ${(delayMs / 1000f)}s..."

        reconnectJob?.cancel()
        reconnectJob = scope.launch {
            delay(delayMs)
            if (shouldReconnect) {
                startSocket()
            }
        }
    }

    fun disconnect() {
        shouldReconnect = false
        reconnectJob?.cancel()
        reconnectJob = null

        try {
            webSocket?.send(Protocol.CMD_STOP)
            webSocket?.close(1000, "User disconnected")
        } catch (_: Exception) {}

        webSocket = null
        jitterBuffer.clear()
        _connectionState.value = ConnectionState.DISCONNECTED
        _statusMessage.value = "Tap to Listen"
    }
}
