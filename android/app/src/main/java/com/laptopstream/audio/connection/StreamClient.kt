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

class StreamClient(private val jitterBuffer: JitterBuffer) {
    companion object {
        private const val TAG = "StreamClient"
    }

    enum class ConnectionState { DISCONNECTED, CONNECTING, CONNECTED, RECONNECTING, ERROR }

    private val _connectionState = MutableStateFlow(ConnectionState.DISCONNECTED)
    val connectionState: StateFlow<ConnectionState> = _connectionState.asStateFlow()
    private val _statusMessage = MutableStateFlow("Speaker is off")
    val statusMessage: StateFlow<String> = _statusMessage.asStateFlow()

    private val parser = LaudPacketParser()
    private val client = OkHttpClient.Builder().pingInterval(5, TimeUnit.SECONDS).retryOnConnectionFailure(true).build()
    private var webSocket: WebSocket? = null
    private var currentHost = ""
    private var currentPort = 8080
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
        _statusMessage.value = if (reconnectAttempt == 0) "Connecting to Speaker service..." else "Reconnecting to Speaker..."
        val request = Request.Builder().url("ws://$currentHost:$currentPort/ws/speaker").build()

        webSocket = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                reconnectAttempt = 0
                _connectionState.value = ConnectionState.CONNECTED
                _statusMessage.value = "Speaker streaming"
                webSocket.send(Protocol.CMD_START_LAUD)
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                val parsed = parser.parse(bytes.toByteArray())
                if (parsed != null) {
                    val (_, samples) = parsed
                    jitterBuffer.push(samples)
                }
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                if (shouldReconnect) scheduleReconnect()
                else {
                    _connectionState.value = ConnectionState.DISCONNECTED
                    _statusMessage.value = "Speaker is off"
                }
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                Log.w(TAG, "Speaker socket failed: ${t.message}")
                if (shouldReconnect) scheduleReconnect()
                else {
                    _connectionState.value = ConnectionState.ERROR
                    _statusMessage.value = "Speaker connection error"
                }
            }
        })
    }

    private fun scheduleReconnect() {
        if (!shouldReconnect) return
        reconnectAttempt++
        val delayMs = min((1000.0 * 1.5.pow((reconnectAttempt - 1).toDouble())).toLong(), 8000L)
        _connectionState.value = ConnectionState.RECONNECTING
        _statusMessage.value = "Reconnecting to Speaker in ${delayMs / 1000f}s..."
        reconnectJob?.cancel()
        reconnectJob = scope.launch {
            delay(delayMs)
            if (shouldReconnect) startSocket()
        }
    }

    fun disconnect() {
        shouldReconnect = false
        reconnectJob?.cancel()
        reconnectJob = null
        try {
            webSocket?.send(Protocol.CMD_STOP)
            webSocket?.close(1000, "Speaker service stopped")
        } catch (_: Exception) { }
        webSocket = null
        jitterBuffer.clear()
        _connectionState.value = ConnectionState.DISCONNECTED
        _statusMessage.value = "Speaker is off"
    }
}
