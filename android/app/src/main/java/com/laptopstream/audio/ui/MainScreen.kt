package com.laptopstream.audio.ui

import androidx.compose.runtime.Composable
import com.laptopstream.audio.discovery.DiscoveredDevice
import com.laptopstream.audio.network.StreamClient.ConnectionState
import com.laptopstream.audio.ui.screens.ConnectedScreen
import com.laptopstream.audio.ui.screens.DiscoveryScreen

@Composable
fun MainScreen(
    connectionState: ConnectionState,
    statusMessage: String,
    discoveredDevice: DiscoveredDevice?,
    isScanning: Boolean,
    onTogglePlayback: () -> Unit,
    onCustomIpChange: (String) -> Unit,
    onRescanClick: () -> Unit,
    bufferDepthMs: Float,
    lostPackets: Long,
    underruns: Int
) {
    val activeIp = discoveredDevice?.ip ?: "10.221.155.222"

    if (connectionState == ConnectionState.CONNECTED) {
        ConnectedScreen(
            connectionState = connectionState,
            discoveredDevice = discoveredDevice,
            targetIp = activeIp,
            onTogglePlayback = onTogglePlayback,
            onSettingsClick = onRescanClick,
            notificationMessage = statusMessage
        )
    } else {
        DiscoveryScreen(
            discoveredDevice = discoveredDevice,
            isScanning = isScanning,
            onConnectClick = onTogglePlayback,
            onSettingsClick = onRescanClick,
            notificationMessage = statusMessage
        )
    }
}
