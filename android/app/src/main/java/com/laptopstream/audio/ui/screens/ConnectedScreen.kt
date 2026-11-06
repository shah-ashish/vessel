package com.laptopstream.audio.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Laptop
import androidx.compose.material.icons.filled.VolumeOff
import androidx.compose.material.icons.filled.VolumeUp
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
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
import com.laptopstream.audio.connection.StreamClient.ConnectionState

private val Background = Color(0xFF0B1220)
private val Surface = Color(0xFF121C2E)
private val Border = Color(0xFF26364F)
private val TextPrimary = Color(0xFFE2E8F0)
private val TextSecondary = Color(0xFF94A3B8)
private val Accent = Color(0xFF38BDF8)
private val Green = Color(0xFF22C55E)
private val Amber = Color(0xFFF59E0B)
private val Red = Color(0xFFF87171)

@Composable
fun ConnectedScreen(
    device: DiscoveredDevice,
    receiverState: ConnectionState,
    speakerState: ConnectionState,
    statusMessage: String,
    onDisconnect: () -> Unit,
    onToggleSpeaker: () -> Unit,
    modifier: Modifier = Modifier
) {
    val speakerRunning = speakerState == ConnectionState.CONNECTED
    val speakerConnecting = speakerState == ConnectionState.CONNECTING ||
        speakerState == ConnectionState.RECONNECTING

    Column(
        modifier = modifier.fillMaxSize().background(Background).padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp)
    ) {
        Column {
            Text("Vessel", color = TextPrimary, fontSize = 22.sp, fontWeight = FontWeight.Bold)
            Text("Device control", color = TextSecondary, fontSize = 12.sp)
        }

        Text("Connected device", color = TextPrimary, fontSize = 20.sp, fontWeight = FontWeight.SemiBold)

        Column(
            modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp))
                .background(Surface).border(1.dp, Border, RoundedCornerShape(16.dp)).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Default.Laptop, contentDescription = null, tint = Accent, modifier = Modifier.size(30.dp))
                Spacer(Modifier.width(12.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(device.name, color = TextPrimary, fontWeight = FontWeight.SemiBold, fontSize = 16.sp)
                    Text("${device.ip}:${device.port}", color = TextSecondary, fontSize = 12.sp)
                }
                Text("CONNECTED", color = Green, fontSize = 10.sp, fontWeight = FontWeight.Bold)
            }
            OutlinedButton(
                onClick = onDisconnect,
                modifier = Modifier.fillMaxWidth(),
                colors = ButtonDefaults.outlinedButtonColors(contentColor = Red)
            ) {
                Text("Disconnect device")
            }
        }

        Text("Services", color = TextPrimary, fontSize = 20.sp, fontWeight = FontWeight.SemiBold)

        Column(
            modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp))
                .background(Surface).border(1.dp, Border, RoundedCornerShape(16.dp)).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    if (speakerRunning) Icons.Default.VolumeUp else Icons.Default.VolumeOff,
                    contentDescription = null,
                    tint = if (speakerRunning) Green else TextSecondary,
                    modifier = Modifier.size(30.dp)
                )
                Spacer(Modifier.width(12.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text("Speaker", color = TextPrimary, fontWeight = FontWeight.SemiBold)
                    Text("Windows system audio", color = TextSecondary, fontSize = 12.sp)
                }
                Text(
                    when {
                        speakerRunning -> "ON"
                        speakerConnecting -> "CONNECTING"
                        else -> "OFF"
                    },
                    color = when {
                        speakerRunning -> Green
                        speakerConnecting -> Amber
                        else -> TextSecondary
                    },
                    fontSize = 10.sp,
                    fontWeight = FontWeight.Bold
                )
            }

            Button(
                onClick = onToggleSpeaker,
                modifier = Modifier.fillMaxWidth(),
                colors = ButtonDefaults.buttonColors(
                    containerColor = if (speakerRunning || speakerConnecting) Red.copy(alpha = 0.14f) else Accent.copy(alpha = 0.16f),
                    contentColor = if (speakerRunning || speakerConnecting) Red else Accent
                )
            ) {
                Text(if (speakerRunning || speakerConnecting) "Turn speaker off" else "Turn speaker on")
            }
        }

        Spacer(Modifier.weight(1f))
        Box(
            modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp))
                .background(Surface).border(1.dp, Border, RoundedCornerShape(12.dp)).padding(14.dp)
        ) {
            Text(
                if (speakerRunning) "Device connection is active. Speaker is streaming."
                else if (speakerConnecting) "Device connection is active. Speaker is connecting."
                else if (receiverState == ConnectionState.CONNECTED) "Device remains connected. Speaker is off."
                else statusMessage,
                color = TextSecondary,
                fontSize = 12.sp
            )
        }
    }
}
