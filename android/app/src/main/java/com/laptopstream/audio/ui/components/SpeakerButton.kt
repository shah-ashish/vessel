package com.laptopstream.audio.ui.components

import androidx.compose.animation.core.*
import androidx.compose.foundation.background
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
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.laptopstream.audio.network.StreamClient.ConnectionState

@Composable
fun SpeakerButton(
    connectionState: ConnectionState,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    val isLive = connectionState == ConnectionState.CONNECTED
    val isConnecting = connectionState == ConnectionState.CONNECTING || connectionState == ConnectionState.RECONNECTING

    // Pulsing animation when streaming
    val infiniteTransition = rememberInfiniteTransition(label = "pulse")
    val pulseScale by infiniteTransition.animateFloat(
        initialValue = 1.0f,
        targetValue = if (isLive) 1.08f else if (isConnecting) 1.04f else 1.0f,
        animationSpec = infiniteRepeatable(
            animation = tween(if (isLive) 900 else 600, easing = FastOutSlowInEasing),
            repeatMode = RepeatMode.Reverse
        ),
        label = "scale"
    )

    val outerGlowColor = when {
        isLive -> Color(0xFF10B981).copy(alpha = 0.25f)
        isConnecting -> Color(0xFFF59E0B).copy(alpha = 0.20f)
        else -> Color.Transparent
    }

    val buttonBrush = when {
        isLive -> Brush.radialGradient(
            colors = listOf(Color(0xFF059669), Color(0xFF047857), Color(0xFF064E3B))
        )
        isConnecting -> Brush.radialGradient(
            colors = listOf(Color(0xFFD97706), Color(0xFFB45309), Color(0xFF78350F))
        )
        else -> Brush.radialGradient(
            colors = listOf(Color(0xFF334155), Color(0xFF1E293B), Color(0xFF0F172A))
        )
    }

    val iconTint = when {
        isLive -> Color(0xFFE6FFFA)
        isConnecting -> Color(0xFFFEF3C7)
        else -> Color(0xFF94A3B8)
    }

    Box(
        modifier = modifier
            .size(240.dp)
            .scale(pulseScale),
        contentAlignment = Alignment.Center
    ) {
        // Outer pulsing ring
        Box(
            modifier = Modifier
                .fillMaxSize()
                .clip(CircleShape)
                .background(outerGlowColor)
        )

        // Main circular button
        Box(
            modifier = Modifier
                .size(190.dp)
                .clip(CircleShape)
                .background(buttonBrush)
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onClick = onClick
                ),
            contentAlignment = Alignment.Center
        ) {
            Icon(
                imageVector = if (isLive) Icons.Default.VolumeUp else Icons.Default.VolumeMute,
                contentDescription = if (isLive) "Speaker Active" else "Speaker Muted",
                tint = iconTint,
                modifier = Modifier.size(92.dp)
            )
        }
    }
}
