package com.laptopstream.audio.ui.components

import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.VolumeMute
import androidx.compose.material.icons.filled.VolumeUp
import androidx.compose.material3.Icon
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.laptopstream.audio.connection.StreamClient.ConnectionState

@Composable
fun SpeakerButton(
    connectionState: ConnectionState,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    val isLive = connectionState == ConnectionState.CONNECTED
    val isConnecting = connectionState == ConnectionState.CONNECTING || connectionState == ConnectionState.RECONNECTING

    val backgroundColor = when {
        isLive -> Color(0x1A10B981) // emerald 10%
        isConnecting -> Color(0x1AF59E0B) // amber 10%
        else -> Color(0xFF0F172A) // slate-900
    }

    val borderColor = when {
        isLive -> Color(0xFF10B981) // emerald-500
        isConnecting -> Color(0xFFF59E0B) // amber-500
        else -> Color(0xFF334155) // slate-700
    }

    val iconTint = when {
        isLive -> Color(0xFF34D399) // emerald-400
        isConnecting -> Color(0xFFFBBF24) // amber-400
        else -> Color(0xFF64748B) // slate-500
    }

    // 48dp touch target for accessibility with 32dp visible circle (border-radius 100%)
    Box(
        modifier = modifier
            .size(48.dp)
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onClick
            ),
        contentAlignment = Alignment.Center
    ) {
        Box(
            modifier = Modifier
                .size(32.dp)
                .clip(CircleShape)
                .background(backgroundColor)
                .border(1.dp, borderColor, CircleShape),
            contentAlignment = Alignment.Center
        ) {
            Icon(
                imageVector = if (isLive) Icons.Default.VolumeUp else Icons.Default.VolumeMute,
                contentDescription = if (isLive) "Turn Off Speaker" else "Turn On Speaker",
                tint = iconTint,
                modifier = Modifier.size(16.dp)
            )
        }
    }
}
