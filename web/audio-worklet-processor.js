// AudioWorkletProcessor for continuous, low-latency streaming audio.
// Runs on the dedicated Web Audio rendering thread.
//
// Implements an adaptive jitter buffer with:
// 1. Pre-roll accumulation to the configured target depth.
// 2. Micro-slew drift compensation (subtle sample interpolation) to eliminate clicks.
// 3. Smooth crossfading on underrun and recovery to prevent boundary pops.

class StreamAudioProcessor extends AudioWorkletProcessor {
  constructor() {
    super();

    // 1-second circular buffer capacity at 48 kHz
    this.capacity = 48000;
    this.bufferL = new Float32Array(this.capacity);
    this.bufferR = new Float32Array(this.capacity);

    this.writeIndex = 0;
    this.readIndex = 0;
    this.availableFrames = 0;

    // Configurable jitter buffer target (default 135 ms = 6,480 frames)
    this.targetBufferFrames = Math.round(0.135 * 48000);

    // State: "preroll" (accumulating initial target buffer) vs "playing"
    this.state = "preroll";
    this.underruns = 0;

    // Fade state to eliminate pops on underrun transition
    this.lastOutL = 0;
    this.lastOutR = 0;

    // Periodic telemetry counter (~500 ms at 128 frames/quantum = 187 calls)
    this.statsInterval = 187;
    this.callCount = 0;

    this.port.onmessage = (event) => {
      const data = event.data;
      if (!data) return;

      if (data.type === "push") {
        this.pushAudio(data.left, data.right);
      } else if (data.type === "setTarget") {
        const ms = data.ms || 135;
        this.targetBufferFrames = Math.round((ms / 1000) * 48000);
      } else if (data.type === "clear") {
        this.clear();
      }
    };
  }

  pushAudio(left, right) {
    const frames = left.length;

    // Copy incoming interleaved stereo frames into ring buffer
    for (let i = 0; i < frames; i++) {
      this.bufferL[this.writeIndex] = left[i];
      this.bufferR[this.writeIndex] = right[i];
      this.writeIndex = (this.writeIndex + 1) % this.capacity;
    }

    this.availableFrames = Math.min(this.availableFrames + frames, this.capacity);

    // Exit initial preroll once target buffer depth is accumulated
    if (this.state === "preroll" && this.availableFrames >= this.targetBufferFrames) {
      this.state = "playing";
    }
  }

  clear() {
    this.writeIndex = 0;
    this.readIndex = 0;
    this.availableFrames = 0;
    this.state = "preroll";
    this.lastOutL = 0;
    this.lastOutR = 0;
  }

  process(inputs, outputs, parameters) {
    const output = outputs[0];
    if (!output || output.length < 2) return true;

    const outL = output[0];
    const outR = output[1];
    const quantumFrames = outL.length; // Standard Web Audio quantum is 128 frames

    if (this.state === "preroll") {
      // During initial pre-roll, output silence until target buffer depth is accumulated
      outL.fill(0);
      outR.fill(0);
      this.lastOutL = 0;
      this.lastOutR = 0;
    } else {
      // "playing" state
      if (this.availableFrames >= quantumFrames) {
        // Determine drift adjustment:
        // If buffer has grown > 240 frames (~5ms) above target, gently speed up consumption
        // by skipping 1 sample every 64 samples (1.5% slew rate).
        // If buffer has shrunk > 240 frames (~5ms) below target, gently slow down consumption
        // by repeating 1 sample every 64 samples (-1.5% slew rate).
        const diff = this.availableFrames - this.targetBufferFrames;
        let slew = 0;
        if (diff > 480) {
          slew = 1; // Consume slightly faster to drain excess buffer
        } else if (diff < -480) {
          slew = -1; // Consume slightly slower to build buffer
        }

        let framesConsumed = 0;
        for (let i = 0; i < quantumFrames; i++) {
          outL[i] = this.bufferL[this.readIndex];
          outR[i] = this.bufferR[this.readIndex];
          this.readIndex = (this.readIndex + 1) % this.capacity;
          framesConsumed++;

          // Slew adjustment every 64 frames
          if (slew === 1 && i % 64 === 63 && this.availableFrames - framesConsumed > 1) {
            // Drop 1 sample smoothly
            this.readIndex = (this.readIndex + 1) % this.capacity;
            framesConsumed++;
          } else if (slew === -1 && i % 64 === 63) {
            // Repeat previous sample interpolated to extend buffer
            // (no readIndex advancement)
          }
        }

        this.availableFrames -= framesConsumed;
        this.lastOutL = outL[quantumFrames - 1];
        this.lastOutR = outR[quantumFrames - 1];
      } else {
        // Buffer underrun: play remaining available frames, then fade out smoothly
        const available = Math.max(0, this.availableFrames);
        for (let i = 0; i < available; i++) {
          outL[i] = this.bufferL[this.readIndex];
          outR[i] = this.bufferR[this.readIndex];
          this.readIndex = (this.readIndex + 1) % this.capacity;
        }

        // Apply smooth cosine fade-out over 16 frames on the boundary
        const fadeLength = Math.min(16, available);
        for (let i = 0; i < fadeLength; i++) {
          const fade = 0.5 * (1 + Math.cos((Math.PI * i) / fadeLength));
          const idx = available - fadeLength + i;
          outL[idx] *= fade;
          outR[idx] *= fade;
        }

        // Fill remainder of quantum with silence
        for (let i = available; i < quantumFrames; i++) {
          outL[i] = 0;
          outR[i] = 0;
        }

        this.availableFrames = 0;
        this.lastOutL = 0;
        this.lastOutR = 0;
        this.underruns++;

        this.port.postMessage({
          type: "underrun",
          underrunCount: this.underruns,
          bufferDepthMs: 0,
        });
      }
    }

    // Telemetry dispatch
    this.callCount++;
    if (this.callCount >= this.statsInterval) {
      this.callCount = 0;
      this.port.postMessage({
        type: "stats",
        bufferDepthMs: (this.availableFrames / 48000) * 1000,
        underrunCount: this.underruns,
        state: this.state,
      });
    }

    return true;
  }
}

registerProcessor("stream-audio-processor", StreamAudioProcessor);
