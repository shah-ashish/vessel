package com.laptopstream.audio.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.wifi.WifiManager
import android.os.Binder
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import com.laptopstream.audio.MainActivity
import com.laptopstream.audio.R
import com.laptopstream.audio.audio.AudioTrackPlayer
import com.laptopstream.audio.audio.JitterBuffer
import com.laptopstream.audio.network.StreamClient

class AudioPlaybackService : Service() {

    companion object {
        const val CHANNEL_ID = "laptop_audio_stream_channel"
        const val NOTIFICATION_ID = 1001

        const val ACTION_START = "com.laptopstream.audio.START"
        const val ACTION_STOP = "com.laptopstream.audio.STOP"
        const val EXTRA_HOST = "extra_host"
        const val EXTRA_PORT = "extra_port"
    }

    private val binder = LocalBinder()
    val jitterBuffer = JitterBuffer(sampleRate = 48000, channels = 2, targetMs = 135)
    private lateinit var audioPlayer: AudioTrackPlayer
    lateinit var streamClient: StreamClient

    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null

    inner class LocalBinder : Binder() {
        fun getService(): AudioPlaybackService = this@AudioPlaybackService
    }

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()

        audioPlayer = AudioTrackPlayer(jitterBuffer)
        streamClient = StreamClient(jitterBuffer)

        val powerManager = getSystemService(Context.POWER_SERVICE) as PowerManager
        wakeLock = powerManager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "LaptopAudioStream:WakeLock")

        val wifiManager = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        wifiLock = wifiManager.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "LaptopAudioStream:WifiLock")
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_START -> {
                val host = intent.getStringExtra(EXTRA_HOST) ?: "10.221.155.222"
                val port = intent.getIntExtra(EXTRA_PORT, 8080)
                startStreaming(host, port)
            }
            ACTION_STOP -> {
                stopStreaming()
            }
        }
        return START_NOT_STICKY
    }

    fun startStreaming(host: String, port: Int = 8080) {
        startForeground(NOTIFICATION_ID, buildNotification(host))

        wakeLock?.acquire(12 * 60 * 60 * 1000L) // 12 hours safe timeout
        wifiLock?.acquire()

        audioPlayer.start()
        streamClient.connect(host, port)
    }

    fun stopStreaming() {
        streamClient.disconnect()
        audioPlayer.stop()

        if (wakeLock?.isHeld == true) wakeLock?.release()
        if (wifiLock?.isHeld == true) wifiLock?.release()

        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    private fun buildNotification(host: String): Notification {
        val openIntent = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP
        }
        val pendingOpen = PendingIntent.getActivity(
            this, 0, openIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

        val stopIntent = Intent(this, AudioPlaybackService::class.java).apply {
            action = ACTION_STOP
        }
        val pendingStop = PendingIntent.getService(
            this, 1, stopIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(getString(R.string.notification_title))
            .setContentText("Connected to $host:8080")
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setContentIntent(pendingOpen)
            .addAction(android.R.drawable.ic_menu_close_clear_cancel, getString(R.string.action_stop), pendingStop)
            .setOngoing(true)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                getString(R.string.channel_name),
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = getString(R.string.channel_desc)
                setShowBadge(false)
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager?.createNotificationChannel(channel)
        }
    }

    override fun onBind(intent: Intent?): IBinder = binder

    override fun onDestroy() {
        stopStreaming()
        super.onDestroy()
    }
}
