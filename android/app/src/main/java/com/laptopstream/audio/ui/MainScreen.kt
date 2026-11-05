package com.laptopstream.audio.ui

import androidx.compose.animation.*
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.laptopstream.audio.discovery.DiscoveredDevice
import com.laptopstream.audio.network.StreamClient.ConnectionState
import com.laptopstream.audio.ui.components.SpeakerButton

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
    var showIpDialog by remember { mutableStateOf(false) }
    var showAdvancedStats by remember { mutableStateOf(false) }

    val activeIp = discoveredDevice?.ip ?: "10.221.155.222"

    Scaffold(
        containerColor = Color(0xFF090D16)
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.SpaceBetween
        ) {
            // TOP BAR: Clean Auto-Discovered Device Pill
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 8.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                // Auto-Discovered Device Pill
                Surface(
                    shape = RoundedCornerShape(24.dp),
                    color = Color(0xFF1E293B),
                    modifier = Modifier.clickable { showIpDialog = true }
                ) {
                    Row(
                        modifier = Modifier.padding(horizontal = 14.dp, vertical = 8.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        val dotColor = when (connectionState) {
                            ConnectionState.CONNECTED -> Color(0xFF10B981)
                            ConnectionState.CONNECTING, ConnectionState.RECONNECTING -> Color(0xFFF59E0B)
                            else -> if (discoveredDevice != null) Color(0xFF38BDF8) else Color(0xFF64748B)
                        }

                        Box(
                            modifier = Modifier
                                .size(9.dp)
                                .clip(CircleShape)
                                .background(dotColor)
                        )
                        Spacer(modifier = Modifier.width(8.dp))

                        val label = when {
                            discoveredDevice != null -> "${discoveredDevice.name} · $activeIp"
                            isScanning -> "Scanning Wi-Fi hotspot..."
                            else -> "Target: $activeIp"
                        }
                        Text(
                            text = label,
                            color = Color(0xFFE2E8F0),
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Medium
                        )
                        Spacer(modifier = Modifier.width(6.dp))
                        Icon(
                            imageVector = Icons.Default.Edit,
                            contentDescription = "Edit IP",
                            tint = Color(0xFF94A3B8),
                            modifier = Modifier.size(14.dp)
                        )
                    }
                }

                // Rescan Button
                IconButton(
                    onClick = onRescanClick,
                    enabled = !isScanning
                ) {
                    Icon(
                        imageVector = Icons.Default.Refresh,
                        contentDescription = "Scan Network",
                        tint = if (isScanning) Color(0xFF64748B) else Color(0xFF94A3B8)
                    )
                }
            }

            // CENTER: Big Speaker Button & Status
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center
            ) {
                SpeakerButton(
                    connectionState = connectionState,
                    onClick = onTogglePlayback
                )

                Spacer(modifier = Modifier.height(36.dp))

                Text(
                    text = statusMessage,
                    color = when (connectionState) {
                        ConnectionState.CONNECTED -> Color(0xFF10B981)
                        ConnectionState.CONNECTING, ConnectionState.RECONNECTING -> Color(0xFFF59E0B)
                        else -> Color(0xFF94A3B8)
                    },
                    fontSize = 20.sp,
                    fontWeight = FontWeight.SemiBold
                )

                Spacer(modifier = Modifier.height(8.dp))

                val subText = when (connectionState) {
                    ConnectionState.CONNECTED -> "Tap speaker to stop audio"
                    ConnectionState.CONNECTING -> "Connecting to Windows audio loopback..."
                    ConnectionState.RECONNECTING -> "Network hiccup, restoring stream..."
                    else -> "Tap speaker to listen"
                }
                Text(
                    text = subText,
                    color = Color(0xFF64748B),
                    fontSize = 14.sp
                )
            }

            // BOTTOM: Hidden for Later / Collapsible Advanced Stats
            Column(
                modifier = Modifier.fillMaxWidth(),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                // Subtle toggle button to open stats when needed later
                TextButton(
                    onClick = { showAdvancedStats = !showAdvancedStats }
                ) {
                    Text(
                        text = if (showAdvancedStats) "Hide Advanced Stats ▲" else "Advanced Stats ▼",
                        color = Color(0xFF475569),
                        fontSize = 12.sp
                    )
                }

                AnimatedVisibility(
                    visible = showAdvancedStats,
                    enter = expandVertically() + fadeIn(),
                    exit = shrinkVertically() + fadeOut()
                ) {
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = Color(0xFF131C2E),
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(top = 4.dp)
                    ) {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(16.dp),
                            horizontalArrangement = Arrangement.SpaceAround
                        ) {
                            StatItem("Buffer", "${bufferDepthMs.toInt()} ms")
                            StatItem("Lost", lostPackets.toString())
                            StatItem("Underruns", underruns.toString())
                            StatItem("Format", "48k PCM16")
                        }
                    }
                }
            }
        }
    }

    // IP Configuration Dialog
    if (showIpDialog) {
        var ipInput by remember { mutableStateOf(activeIp) }

        AlertDialog(
            onDismissRequest = { showIpDialog = false },
            title = { Text("Server IP Address") },
            text = {
                Column {
                    Text(
                        "Enter the IP address of your Windows laptop if not auto-detected:",
                        fontSize = 13.sp,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                    OutlinedTextField(
                        value = ipInput,
                        onValueChange = { ipInput = it },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth()
                    )
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    onCustomIpChange(ipInput.trim())
                    showIpDialog = false
                }) {
                    Text("Save & Connect")
                }
            },
            dismissButton = {
                TextButton(onClick = { showIpDialog = false }) {
                    Text("Cancel")
                }
            }
        )
    }
}

@Composable
private fun StatItem(label: String, value: String) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(text = label, color = Color(0xFF64748B), fontSize = 11.sp)
        Spacer(modifier = Modifier.height(2.dp))
        Text(text = value, color = Color(0xFFE2E8F0), fontSize = 13.sp, fontWeight = FontWeight.Bold)
    }
}
