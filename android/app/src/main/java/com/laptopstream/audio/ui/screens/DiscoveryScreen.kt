package com.laptopstream.audio.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ChevronRight
import androidx.compose.material.icons.filled.Laptop
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.laptopstream.audio.connection.DiscoveredDevice

private val Background = Color(0xFF0B1220)
private val Surface = Color(0xFF121C2E)
private val Border = Color(0xFF26364F)
private val TextPrimary = Color(0xFFE2E8F0)
private val TextSecondary = Color(0xFF94A3B8)
private val Accent = Color(0xFF38BDF8)

@Composable
fun DiscoveryScreen(
    devices: List<DiscoveredDevice>,
    isScanning: Boolean,
    statusMessage: String,
    onSelectDevice: (DiscoveredDevice) -> Unit,
    onRescan: () -> Unit,
    modifier: Modifier = Modifier
) {
    Column(
        modifier = modifier.fillMaxSize().background(Background).padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp)
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(modifier = Modifier.weight(1f)) {
                Text("Vessel", color = TextPrimary, fontSize = 22.sp, fontWeight = FontWeight.Bold)
                Text("Audio receiver", color = TextSecondary, fontSize = 12.sp)
            }
            IconButton(onClick = onRescan, enabled = !isScanning) {
                Icon(Icons.Default.Refresh, contentDescription = "Scan again", tint = Accent)
            }
        }

        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Available devices", color = TextPrimary, fontSize = 20.sp, fontWeight = FontWeight.SemiBold)
            Text("Select one Vessel server to connect. Speaker playback starts separately.", color = TextSecondary, fontSize = 13.sp)
        }

        when {
            devices.isNotEmpty() -> LazyColumn(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(10.dp)
            ) {
                items(devices, key = { "${it.ip}:${it.port}" }) { device ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(16.dp))
                            .background(Surface)
                            .border(1.dp, Border, RoundedCornerShape(16.dp))
                            .clickable { onSelectDevice(device) }
                            .padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Icon(Icons.Default.Laptop, contentDescription = null, tint = Accent, modifier = Modifier.size(28.dp))
                        Spacer(Modifier.width(14.dp))
                        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                            Text(device.name, color = TextPrimary, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                            Text("${device.ip}:${device.port}", color = TextSecondary, fontSize = 12.sp)
                            Text("Vessel server available", color = Color(0xFF22C55E), fontSize = 11.sp)
                        }
                        Icon(Icons.Default.ChevronRight, contentDescription = "Connect", tint = TextSecondary)
                    }
                }
            }
            else -> Column(
                modifier = Modifier.weight(1f).fillMaxWidth(),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center
            ) {
                if (isScanning) CircularProgressIndicator(color = Accent)
                else Icon(Icons.Default.Laptop, contentDescription = null, tint = TextSecondary, modifier = Modifier.size(44.dp))
                Spacer(Modifier.height(14.dp))
                Text(
                    if (isScanning) "Searching for Vessel servers..." else "No devices found",
                    color = TextPrimary,
                    fontWeight = FontWeight.Medium
                )
                Text("Make sure your laptop and phone are on the same network.", color = TextSecondary, fontSize = 12.sp)
            }
        }

        Box(
            modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp))
                .background(Surface).border(1.dp, Border, RoundedCornerShape(12.dp)).padding(14.dp)
        ) {
            Text(statusMessage, color = TextSecondary, fontSize = 12.sp)
        }
    }
}
