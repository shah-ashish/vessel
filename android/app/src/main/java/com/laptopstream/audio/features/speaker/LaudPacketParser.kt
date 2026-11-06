package com.laptopstream.audio.features.speaker

import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Validates and parses 20-byte LAUD v1 packets into typed headers and PCM16 samples.
 */
class LaudPacketParser {

    private var expectedSeq: Long? = null
    var lostPackets: Long = 0
        private set
    var totalPackets: Long = 0
        private set

    fun reset() {
        expectedSeq = null
        lostPackets = 0
        totalPackets = 0
    }

    /**
     * Parses a binary packet. Returns a Pair of [LaudHeader] and [ShortArray] PCM samples,
     * or null if the packet is malformed or invalid.
     */
    fun parse(bytes: ByteArray): Pair<LaudHeader, ShortArray>? {
        if (bytes.size < Protocol.LAUD_HEADER_SIZE) return null

        val buffer = ByteBuffer.wrap(bytes).order(ByteOrder.BIG_ENDIAN)
        val magic = buffer.int
        val version = buffer.get().toInt() and 0xFF
        val flags = buffer.get().toInt() and 0xFF
        val channels = buffer.get().toInt() and 0xFF
        val sampleFormat = buffer.get().toInt() and 0xFF
        val sequence = buffer.int.toLong() and 0xFFFFFFFFL
        val timestampFrames = buffer.long

        // Validation
        if (magic != Protocol.LAUD_MAGIC || version != Protocol.LAUD_VERSION ||
            channels != Protocol.CHANNELS || sampleFormat != 1) {
            return null
        }

        totalPackets++

        // Sequence loss tracking
        expectedSeq?.let { expected ->
            if (sequence > expected) {
                lostPackets += (sequence - expected)
            }
        }
        expectedSeq = (sequence + 1L) and 0xFFFFFFFFL

        val pcmByteCount = bytes.size - Protocol.LAUD_HEADER_SIZE
        val sampleCount = pcmByteCount / Protocol.BYTES_PER_SAMPLE
        val pcmBuffer = ByteBuffer.wrap(bytes, Protocol.LAUD_HEADER_SIZE, pcmByteCount)
            .order(ByteOrder.LITTLE_ENDIAN)

        val samples = ShortArray(sampleCount)
        val isSilent = (flags and Protocol.FLAG_SILENCE) != 0

        if (!isSilent) {
            pcmBuffer.asShortBuffer().get(samples)
        }

        val header = LaudHeader(
            magic = magic,
            version = version,
            flags = flags,
            channels = channels,
            sampleFormat = sampleFormat,
            sequence = sequence,
            timestampFrames = timestampFrames,
            isSilent = isSilent,
            isDiscontinuity = (flags and Protocol.FLAG_DISCONTINUITY) != 0
        )

        return Pair(header, samples)
    }
}
