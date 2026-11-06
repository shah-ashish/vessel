package com.laptopstream.audio.features.speaker

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import android.os.Process
import android.util.Log

/**
 * Manages low-latency AudioTrack playback in streaming mode (48,000 Hz, 2 Channels, Signed 16-bit PCM).
 */
class AudioTrackPlayer(
    private val jitterBuffer: JitterBuffer
) {
    companion object {
        private const val TAG = "AudioTrackPlayer"
        private const val SAMPLE_RATE = 48000
        private const val CHANNELS = 2
        private const val QUANTUM_FRAMES = 512 // ~10.6 ms chunk for low-latency hardware feeding
    }

    private var audioTrack: AudioTrack? = null
    private var renderThread: Thread? = null
    @Volatile
    private var isPlaying = false

    fun start() {
        if (isPlaying) return

        val minBufferSize = AudioTrack.getMinBufferSize(
            SAMPLE_RATE,
            AudioFormat.CHANNEL_OUT_STEREO,
            AudioFormat.ENCODING_PCM_16BIT
        )

        val bufferSize = maxOf(minBufferSize, QUANTUM_FRAMES * CHANNELS * 2 * 4)

        val attributes = AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_MEDIA)
            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
            .setFlags(AudioAttributes.FLAG_LOW_LATENCY)
            .build()

        val format = AudioFormat.Builder()
            .setSampleRate(SAMPLE_RATE)
            .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
            .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
            .build()

        audioTrack = AudioTrack.Builder()
            .setAudioAttributes(attributes)
            .setAudioFormat(format)
            .setBufferSizeInBytes(bufferSize)
            .setTransferMode(AudioTrack.MODE_STREAM)
            .build()

        audioTrack?.play()
        isPlaying = true

        renderThread = Thread({
            Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
            val chunk = ShortArray(QUANTUM_FRAMES * CHANNELS)

            while (isPlaying) {
                jitterBuffer.read(chunk)
                audioTrack?.write(chunk, 0, chunk.size)
            }
        }, "AudioTrackRenderThread").apply {
            priority = Thread.MAX_PRIORITY
            start()
        }

        Log.i(TAG, "AudioTrack streaming started at $SAMPLE_RATE Hz")
    }

    fun stop() {
        isPlaying = false
        renderThread?.interrupt()
        renderThread = null

        try {
            audioTrack?.stop()
            audioTrack?.release()
        } catch (e: Exception) {
            Log.w(TAG, "Error stopping AudioTrack: ${e.message}")
        }
        audioTrack = null
        jitterBuffer.clear()
        Log.i(TAG, "AudioTrack stopped")
    }
}
