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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.core.content.ContextCompat
import androidx.lifecycle.lifecycleScope
import com.laptopstream.audio.connection.DeviceControlClient
import com.laptopstream.audio.connection.DeviceDiscoveryManager
import com.laptopstream.audio.connection.DiscoveredDevice
import com.laptopstream.audio.connection.StreamClient.ConnectionState
import com.laptopstream.audio.features.speaker.AudioPlaybackService
import com.laptopstream.audio.ui.MainScreen
import com.laptopstream.audio.update.UpdateChecker
import com.laptopstream.audio.update.VesselUpdate
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {

    private lateinit var discoveryManager: DeviceDiscoveryManager
    private lateinit var controlClient: DeviceControlClient
    private var playbackService: AudioPlaybackService? = null
    private var isBound = false

    private val receiverState = mutableStateOf(ConnectionState.DISCONNECTED)
    private val speakerState = mutableStateOf(ConnectionState.DISCONNECTED)
    private val statusMessage = mutableStateOf("Searching for Vessel servers...")
    private val discoveredDevices = mutableStateOf<List<DiscoveredDevice>>(emptyList())
    private val selectedDevice = mutableStateOf<DiscoveredDevice?>(null)
    private val isScanning = mutableStateOf(false)
    private val pendingUpdate = mutableStateOf<VesselUpdate?>(null)
    private val showUpdateDialog = mutableStateOf(false)

    private val requestNotificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

    private val serviceConnection = object : ServiceConnection {
        override fun onServiceConnected(name: ComponentName?, service: IBinder?) {
            playbackService = (service as? AudioPlaybackService.LocalBinder)?.getService()
            isBound = true
            monitorSpeakerState()
        }

        override fun onServiceDisconnected(name: ComponentName?) {
            playbackService = null
            isBound = false
            speakerState.value = ConnectionState.DISCONNECTED
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        discoveryManager = DeviceDiscoveryManager(this)

        controlClient = DeviceControlClient { state, message ->
            runOnUiThread {
                receiverState.value = state
                statusMessage.value = message
                if (state == ConnectionState.ERROR || state == ConnectionState.DISCONNECTED) {
                    selectedDevice.value = null
                    stopSpeakerPlayback()
                }
            }
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            requestNotificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }

        setContent {
            val receiver by receiverState
            val speaker by speakerState
            val status by statusMessage
            val devices by discoveredDevices
            val selected by selectedDevice
            val scanning by isScanning
            val update by pendingUpdate
            val showUpdate by showUpdateDialog

            MainScreen(
                receiverState = receiver,
                speakerState = speaker,
                statusMessage = status,
                devices = devices,
                selectedDevice = selected,
                isScanning = scanning,
                onSelectDevice = { connectToDevice(it) },
                onDisconnect = { disconnectDevice() },
                onToggleSpeaker = { toggleSpeaker() },
                onRescan = { startDiscovery() }
            )

            if (showUpdate && update != null) {
                AlertDialog(
                    onDismissRequest = { showUpdateDialog.value = false },
                    title = { Text("Vessel update available") },
                    text = { Text("Version ${update.version}\n\n${update.notes}") },
                    confirmButton = {
                        Button(onClick = {
                            showUpdateDialog.value = false
                            UpdateChecker(this@MainActivity).downloadAndInstall(update)
                        }) { Text("Download and install") }
                    },
                    dismissButton = {
                        TextButton(onClick = { showUpdateDialog.value = false }) { Text("Later") }
                    }
                )
            }
        }

        startDiscovery()
        checkForUpdatesAtStartup()
    }

    override fun onStart() {
        super.onStart()
        bindService(Intent(this, AudioPlaybackService::class.java), serviceConnection, Context.BIND_AUTO_CREATE)
    }

    override fun onStop() {
        if (isBound) {
            unbindService(serviceConnection)
            isBound = false
        }
        super.onStop()
    }

    override fun onDestroy() {
        controlClient.disconnect()
        super.onDestroy()
    }

    private fun checkForUpdatesAtStartup() {
        // Deliberately launch-only: no periodic worker or background polling is registered.
        lifecycleScope.launch(kotlinx.coroutines.Dispatchers.IO) {
            val update = UpdateChecker(this@MainActivity).checkAtStartup()
            if (update != null && isActive) {
                runOnUiThread {
                    pendingUpdate.value = update
                    showUpdateDialog.value = true
                }
            }
        }
    }

    private fun startDiscovery() {
        if (isScanning.value) return
        isScanning.value = true
        statusMessage.value = "Searching for Vessel servers..."
        discoveryManager.startDiscovery { devices ->
            isScanning.value = false
            discoveredDevices.value = devices
            statusMessage.value = if (devices.isEmpty()) {
                "No Vessel servers found. Check the network and scan again."
            } else {
                "${devices.size} Vessel server(s) available."
            }
        }
    }

    private fun connectToDevice(device: DiscoveredDevice) {
        if (receiverState.value != ConnectionState.DISCONNECTED &&
            receiverState.value != ConnectionState.ERROR
        ) return

        selectedDevice.value = device
        receiverState.value = ConnectionState.CONNECTING
        statusMessage.value = "Connecting to ${device.name}..."
        controlClient.connect(device.ip, device.port, "Vessel Android")
    }

    private fun disconnectDevice() {
        stopSpeakerPlayback()
        controlClient.disconnect()
        selectedDevice.value = null
        receiverState.value = ConnectionState.DISCONNECTED
        statusMessage.value = "Disconnected from device."
        speakerState.value = ConnectionState.DISCONNECTED
    }

    private fun toggleSpeaker() {
        val device = selectedDevice.value ?: return
        when (speakerState.value) {
            ConnectionState.CONNECTED,
            ConnectionState.CONNECTING,
            ConnectionState.RECONNECTING -> stopSpeakerPlayback()
            else -> {
                val startIntent = Intent(this, AudioPlaybackService::class.java).apply {
                    action = AudioPlaybackService.ACTION_START
                    putExtra(AudioPlaybackService.EXTRA_HOST, device.ip)
                    putExtra(AudioPlaybackService.EXTRA_PORT, device.port)
                }
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) startForegroundService(startIntent)
                else startService(startIntent)
                speakerState.value = ConnectionState.CONNECTING
                statusMessage.value = "Starting Speaker service..."
            }
        }
    }

    private fun stopSpeakerPlayback() {
        val stopIntent = Intent(this, AudioPlaybackService::class.java).apply {
            action = AudioPlaybackService.ACTION_STOP
        }
        try { startService(stopIntent) } catch (_: Exception) { }
        speakerState.value = ConnectionState.DISCONNECTED
    }

    private fun monitorSpeakerState() {
        lifecycleScope.launch {
            while (isActive && isBound) {
                playbackService?.let { service ->
                    speakerState.value = service.streamClient.connectionState.value
                    if (speakerState.value == ConnectionState.ERROR) {
                        statusMessage.value = service.streamClient.statusMessage.value
                    }
                }
                delay(250)
            }
        }
    }
}
