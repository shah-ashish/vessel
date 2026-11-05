package com.laptopstream.audio.network

/**
 * Protocol constants for the LAUD v1 20-byte framed binary audio format.
 */
object Protocol {
    const val LAUD_MAGIC = 0x4c415544 // "LAUD" in ASCII
    const val LAUD_VERSION = 1
    const val LAUD_HEADER_SIZE = 20

    const val FLAG_SILENCE = 0x01
    const val FLAG_DISCONTINUITY = 0x02

    const val SAMPLE_RATE = 48000
    const val CHANNELS = 2
    const val BYTES_PER_SAMPLE = 2 // Signed 16-bit PCM

    const val CMD_START_LAUD = "START:LAUD"
    const val CMD_START_RAW = "START:RAW"
    const val CMD_STOP = "STOP"
}

data class LaudHeader(
    val magic: Int,
    val version: Int,
    val flags: Int,
    val channels: Int,
    val sampleFormat: Int,
    val sequence: Long,
    val timestampFrames: Long,
    val isSilent: Boolean,
    val isDiscontinuity: Boolean
)
