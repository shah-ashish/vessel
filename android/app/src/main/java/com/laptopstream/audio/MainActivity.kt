package com.laptopstream.audio

import android.Manifest
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.*
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import com.laptopstream.audio.discovery.DeviceDiscoveryManager
import com.laptopstream.audio.discovery.DiscoveredDevice
import com.laptopstream.audio.network.StreamClient.ConnectionState
import com.laptopstream.audio.service.AudioPlaybackService
import com.laptopstream.audio.ui.MainScreen
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {

    private lateinit var discoveryManager: DeviceDiscoveryManager
    private var playbackService: AudioPlaybackService? = null
    private var isBound = false

    private val connectionState = mutableStateOf(ConnectionState.DISCONNECTED)
    private val statusMessage = mutableStateOf("Ready to listen")
    private val discoveredDevice = mutableStateOf<DiscoveredDevice?>(null)
    private val isScanning = mutableStateOf(false)
    private val bufferDepthMs = mutableStateOf(0f)
    private val lostPackets = mutableStateOf(0L)
    private val underruns = mutableStateOf(0)

    private var customTargetIp: String? = null

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, service: IBinder?) {
            val binder = service as? AudioPlaybackService.LocalBinder
            playbackService = binder?.getService()
            isBound = true
            monitorServiceState()
        }

        override fun onServiceDisconnected(name: ComponentName?) {
            playbackService = null
            isBound = false
            connectionState.value = ConnectionState.DISCONNECTED
            statusMessage.value = "Disconnected"
        }
    }

    private val requestNotificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            // Notification permission granted or denied
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        discoveryManager = DeviceDiscoveryManager(this)

        // Check notification permission for Android 13+
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            if (ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS)
                != PackageManager.PERMISSION_GRANTED) {
                requestNotificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
        }

        // Restore last known IP
        val lastIp = DeviceDiscoveryManager.getLastSavedIp(this)
        if (lastIp != null) {
            discoveredDevice.value = DiscoveredDevice("Windows Laptop (Saved)", lastIp, 8080)
        }

        // Auto-discover laptop on hotspot / Wi-Fi network immediately
        startDiscovery()

        setContent {
            val connState by connectionState
            val status by statusMessage
            val device by discoveredDevice
            val scanning by isScanning
            val bufferDepth by bufferDepthMs
            val lost by lostPackets
            val under by underruns

            MainScreen(
                connectionState = connState,
                statusMessage = status,
                discoveredDevice = device,
                isScanning = scanning,
                onTogglePlayback = { toggleStreaming() },
                onCustomIpChange = { newIp ->
                    customTargetIp = newIp
                    DeviceDiscoveryManager.saveLastIp(this@MainActivity, newIp)
                    discoveredDevice.value = DiscoveredDevice("Custom Target", newIp, 8080)
                },
                onRescanClick = { startDiscovery() },
                bufferDepthMs = bufferDepth,
                lostPackets = lost,
                underruns = under
            )
        }
    }

    override fun onStart() {
        super.onStart()
        val intent = Intent(this, AudioPlaybackService::class.java)
        bindService(intent, serviceConnection, Context.BIND_AUTO_CREATE)
    }

    override fun onStop() {
        super.onStop()
        if (isBound) {
            unbindService(serviceConnection)
            isBound = false
        }
    }

    private fun startDiscovery() {
        if (isScanning.value) return
        isScanning.value = true

        discoveryManager.startDiscovery { device ->
            isScanning.value = false
            if (device != null) {
                discoveredDevice.value = device
                DeviceDiscoveryManager.saveLastIp(this@MainActivity, device.ip)
            }
        }
    }

    private fun toggleStreaming() {
        val targetIp = customTargetIp ?: discoveredDevice.value?.ip ?: "10.221.155.222"

        if (connectionState.value == ConnectionState.CONNECTED ||
            connectionState.value == ConnectionState.CONNECTING ||
            connectionState.value == ConnectionState.RECONNECTING) {
            // STOP
            val stopIntent = Intent(this, AudioPlaybackService::class.java).apply {
                action = AudioPlaybackService.ACTION_STOP
            }
            startService(stopIntent)
            connectionState.value = ConnectionState.DISCONNECTED
            statusMessage.value = "Audio stopped"
        } else {
            // START
            val startIntent = Intent(this, AudioPlaybackService::class.java).apply {
                action = AudioPlaybackService.ACTION_START
                putExtra(AudioPlaybackService.EXTRA_HOST, targetIp)
                putExtra(AudioPlaybackService.EXTRA_PORT, 8080)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                startForegroundService(startIntent)
            } else {
                startService(startIntent)
            }
            connectionState.value = ConnectionState.CONNECTING
            statusMessage.value = "Connecting to $targetIp..."
        }
    }

    private fun monitorServiceState() {
        lifecycleScope.launch {
            while (isActive && isBound) {
                playbackService?.let { service ->
                    connectionState.value = service.streamClient.connectionState.value
                    statusMessage.value = service.streamClient.statusMessage.value
                    bufferDepthMs.value = service.jitterBuffer.bufferDepthMs
                    lostPackets.value = service.streamClient.lostPackets
                    underruns.value = service.jitterBuffer.underruns
                }
                delay(100)
            }
        }
    }
}
