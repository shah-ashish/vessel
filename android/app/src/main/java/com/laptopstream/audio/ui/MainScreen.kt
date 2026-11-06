package com.laptopstream.audio.ui

import androidx.compose.runtime.Composable
import com.laptopstream.audio.connection.DiscoveredDevice
import com.laptopstream.audio.connection.StreamClient.ConnectionState
import com.laptopstream.audio.ui.screens.ConnectedScreen
import com.laptopstream.audio.ui.screens.DiscoveryScreen

@Composable
fun MainScreen(
    receiverState: ConnectionState,
    speakerState: ConnectionState,
    statusMessage: String,
    devices: List<DiscoveredDevice>,
    selectedDevice: DiscoveredDevice?,
    isScanning: Boolean,
    onSelectDevice: (DiscoveredDevice) -> Unit,
    onDisconnect: () -> Unit,
    onToggleSpeaker: () -> Unit,
    onRescan: () -> Unit
) {
    if (receiverState == ConnectionState.CONNECTED && selectedDevice != null) {
        ConnectedScreen(
            device = selectedDevice,
            receiverState = receiverState,
            speakerState = speakerState,
            statusMessage = statusMessage,
            onDisconnect = onDisconnect,
            onToggleSpeaker = onToggleSpeaker
        )
    } else {
        DiscoveryScreen(
            devices = devices,
            isScanning = isScanning,
            statusMessage = statusMessage,
            onSelectDevice = onSelectDevice,
            onRescan = onRescan
        )
    }
}
