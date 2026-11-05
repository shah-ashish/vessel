// Laptop Audio Stream · Reference Receiver Client
// Continuous low-latency streaming with dual AudioWorklet & ScriptProcessor fallback

const LAUD_MAGIC = 0x4c415544; // "LAUD"
const LAUD_VERSION = 1;
const LAUD_HEADER_SIZE = 20;
const FLAG_SILENCE = 0x01;
const FLAG_DISCONTINUITY = 0x02;

// UI Elements
const listenBtn = document.getElementById("listen");
const stopBtn = document.getElementById("stop");
const statusText = document.getElementById("status");
const detailText = document.getElementById("detail");
const dot = document.getElementById("dot");
const visualizer = document.querySelector(".visualizer");
const protoSelect = document.getElementById("proto-select");
const bufferTargetSelect = document.getElementById("buffer-target");
const resumeBanner = document.getElementById("resume-banner");
const resumeBtn = document.getElementById("resume-btn");

// Telemetry DOM elements
const statProto = document.getElementById("stat-proto");
const statBuffer = document.getElementById("stat-buffer");
const statPackets = document.getElementById("stat-packets");
const statSeq = document.getElementById("stat-seq");
const statLost = document.getElementById("stat-lost");
const statUnderruns = document.getElementById("stat-underruns");
const statReconnects = document.getElementById("stat-reconnects");
const statCtx = document.getElementById("stat-ctx");

// Audio & Networking State
let audioContext = null;
let audioSink = null;
let ws = null;
let active = false;
let userRequestedStop = false;
let reconnectTimer = null;
let reconnectAttempts = 0;
let reconnectCount = 0;

// Metrics & Packet Tracking
let packetsReceived = 0;
let lastSeq = 0;
let expectedSeq = null;
let lostPackets = 0;
let underrunCount = 0;
let malformedPackets = 0;
let bufferDepthMs = 0;

function updateTelemetryUI() {
  const engineLabel = audioSink ? (audioSink.type === "worklet" ? "Worklet" : "Fallback") : "";
  const protoLabel = protoSelect.value === "LAUD" ? "LAUD v1" : "Raw PCM";
  statProto.textContent = engineLabel ? `${protoLabel} (${engineLabel})` : protoLabel;

  statPackets.textContent = packetsReceived.toLocaleString();
  statSeq.textContent = packetsReceived > 0 ? `#${lastSeq}` : "#0";

  statLost.textContent = lostPackets.toString();
  statLost.className = `telemetry-value ${lostPackets === 0 ? "ok" : "warn"}`;

  statUnderruns.textContent = underrunCount.toString();
  statUnderruns.className = `telemetry-value ${underrunCount === 0 ? "ok" : "warn"}`;

  statBuffer.textContent = `${Math.round(bufferDepthMs)} ms`;
  statReconnects.textContent = reconnectCount.toString();
  statCtx.textContent = audioContext ? audioContext.state : "idle";
}

function setUIState(isActive, status, detail) {
  active = isActive;
  statusText.textContent = status;
  detailText.textContent = detail;
  dot.classList.toggle("live", isActive);
  visualizer.classList.toggle("active", isActive);
  listenBtn.disabled = isActive;
  stopBtn.disabled = !isActive;
  protoSelect.disabled = isActive;
  updateTelemetryUI();
}

async function createAudioSink(ctx, targetMs) {
  // 1. Check if AudioWorklet is supported (Secure context: localhost or https)
  if (ctx.audioWorklet && typeof ctx.audioWorklet.addModule === "function") {
    try {
      await ctx.audioWorklet.addModule("/audio-worklet-processor.js");
      const workletNode = new AudioWorkletNode(ctx, "stream-audio-processor", {
        numberOfInputs: 0,
        numberOfOutputs: 1,
        outputChannelCount: [2],
      });

      workletNode.port.onmessage = (event) => {
        const data = event.data;
        if (!data) return;
        if (data.type === "stats") {
          bufferDepthMs = data.bufferDepthMs || 0;
          underrunCount = data.underrunCount || 0;
          updateTelemetryUI();
        } else if (data.type === "underrun") {
          underrunCount = data.underrunCount || 0;
          bufferDepthMs = data.bufferDepthMs || 0;
          updateTelemetryUI();
        }
      };

      workletNode.port.postMessage({ type: "setTarget", ms: targetMs });
      workletNode.connect(ctx.destination);

      return {
        type: "worklet",
        push(left, right) {
          workletNode.port.postMessage({ type: "push", left, right }, [
            left.buffer,
            right.buffer,
          ]);
        },
        setTarget(ms) {
          workletNode.port.postMessage({ type: "setTarget", ms });
        },
        clear() {
          workletNode.port.postMessage({ type: "clear" });
        },
        disconnect() {
          try {
            workletNode.disconnect();
          } catch (_) {}
        },
      };
    } catch (err) {
      console.warn("AudioWorklet unavailable or failed, switching to fallback:", err);
    }
  }

  // 2. High-performance ScriptProcessor fallback (Runs on HTTP LAN IP, mobile, insecure contexts)
  console.log("Using universal ScriptProcessor jitter buffer sink.");
  const bufferSize = 1024;
  const scriptNode = ctx.createScriptProcessor(bufferSize, 0, 2);

  const capacity = 48000;
  const bufferL = new Float32Array(capacity);
  const bufferR = new Float32Array(capacity);
  let writeIndex = 0;
  let readIndex = 0;
  let availableFrames = 0;
  let targetBufferFrames = Math.round((targetMs / 1000) * 48000);
  let state = "preroll";
  let callCount = 0;

  scriptNode.onaudioprocess = (event) => {
    const outL = event.outputBuffer.getChannelData(0);
    const outR = event.outputBuffer.getChannelData(1);
    const quantumFrames = outL.length;

    if (state === "preroll") {
      outL.fill(0);
      outR.fill(0);
    } else {
      if (availableFrames >= quantumFrames) {
        // Micro-slew drift compensation
        const diff = availableFrames - targetBufferFrames;
        let slew = 0;
        if (diff > 480) slew = 1;
        else if (diff < -480) slew = -1;

        let framesConsumed = 0;
        for (let i = 0; i < quantumFrames; i++) {
          outL[i] = bufferL[readIndex];
          outR[i] = bufferR[readIndex];
          readIndex = (readIndex + 1) % capacity;
          framesConsumed++;

          if (slew === 1 && i % 64 === 63 && availableFrames - framesConsumed > 1) {
            readIndex = (readIndex + 1) % capacity;
            framesConsumed++;
          }
        }
        availableFrames -= framesConsumed;
      } else {
        // Underrun: play remaining available frames, then cosine fade-out
        const available = Math.max(0, availableFrames);
        for (let i = 0; i < available; i++) {
          outL[i] = bufferL[readIndex];
          outR[i] = bufferR[readIndex];
          readIndex = (readIndex + 1) % capacity;
        }

        const fadeLength = Math.min(16, available);
        for (let i = 0; i < fadeLength; i++) {
          const fade = 0.5 * (1 + Math.cos((Math.PI * i) / fadeLength));
          const idx = available - fadeLength + i;
          outL[idx] *= fade;
          outR[idx] *= fade;
        }

        for (let i = available; i < quantumFrames; i++) {
          outL[i] = 0;
          outR[i] = 0;
        }

        availableFrames = 0;
        underrunCount++;
        updateTelemetryUI();
      }
    }

    callCount++;
    if (callCount >= 24) {
      callCount = 0;
      bufferDepthMs = (availableFrames / 48000) * 1000;
      updateTelemetryUI();
    }
  };

  scriptNode.connect(ctx.destination);

  return {
    type: "script",
    push(left, right) {
      const frames = left.length;
      for (let i = 0; i < frames; i++) {
        bufferL[writeIndex] = left[i];
        bufferR[writeIndex] = right[i];
        writeIndex = (writeIndex + 1) % capacity;
      }
      availableFrames = Math.min(availableFrames + frames, capacity);
      if (state === "preroll" && availableFrames >= targetBufferFrames) {
        state = "playing";
      }
    },
    setTarget(ms) {
      targetBufferFrames = Math.round((ms / 1000) * 48000);
    },
    clear() {
      writeIndex = 0;
      readIndex = 0;
      availableFrames = 0;
      state = "preroll";
    },
    disconnect() {
      try {
        scriptNode.disconnect();
      } catch (_) {}
    },
  };
}

async function ensureAudioContext() {
  if (!audioContext || audioContext.state === "closed") {
    audioContext = new (window.AudioContext || window.webkitAudioContext)({
      sampleRate: 48000,
      latencyHint: "interactive",
    });

    audioContext.onstatechange = () => {
      statCtx.textContent = audioContext.state;
      if (audioContext.state === "suspended" && active) {
        resumeBanner.classList.remove("hidden");
      } else {
        resumeBanner.classList.add("hidden");
      }
    };
  }

  if (audioContext.state === "suspended") {
    await audioContext.resume();
  }

  if (!audioSink) {
    const targetMs = parseInt(bufferTargetSelect.value, 10) || 135;
    audioSink = await createAudioSink(audioContext, targetMs);
    window.audioSink = audioSink;
    window.workletNode = audioSink;
  }
}

function connectWebSocket() {
  if (userRequestedStop) return;

  const scheme = location.protocol === "https:" ? "wss" : "ws";
  const wsUrl = `${scheme}://${location.host}/ws`;

  ws = new WebSocket(wsUrl);
  ws.binaryType = "arraybuffer";

  ws.onopen = () => {
    reconnectAttempts = 0;
    const protocolMode = protoSelect.value;
    const subscribeMsg = protocolMode === "LAUD" ? "START:LAUD" : "START:RAW";
    ws.send(subscribeMsg);

    setUIState(
      true,
      `Connected · ${protocolMode === "LAUD" ? "LAUD v1" : "Raw PCM"}`,
      `Streaming live system audio (${bufferTargetSelect.value} ms target buffer).`
    );
  };

  ws.onmessage = (event) => {
    if (!active || !(event.data instanceof ArrayBuffer)) return;
    const arrayBuffer = event.data;

    if (protoSelect.value === "LAUD") {
      handleLaudPacket(arrayBuffer);
    } else {
      handleRawPacket(arrayBuffer);
    }
  };

  ws.onerror = () => {
    if (!userRequestedStop) {
      statusText.textContent = "Connection error";
      detailText.textContent = "Network error. Reconnecting...";
    }
  };

  ws.onclose = () => {
    if (userRequestedStop) {
      setUIState(false, "Stopped", "Listening has stopped.");
      return;
    }

    setUIState(false, "Reconnecting...", "Connection lost. Retrying with backoff...");
    scheduleReconnect();
  };
}

function scheduleReconnect() {
  if (reconnectTimer || userRequestedStop) return;

  reconnectAttempts++;
  reconnectCount++;
  // Exponential backoff: 1s, 1.5s, 2.25s ... capped at 8s
  const delay = Math.min(Math.round(1000 * Math.pow(1.5, reconnectAttempts - 1)), 8000);
  detailText.textContent = `Reconnecting in ${(delay / 1000).toFixed(1)}s (attempt ${reconnectAttempts})...`;
  updateTelemetryUI();

  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    if (!userRequestedStop) {
      connectWebSocket();
    }
  }, delay);
}

function handleLaudPacket(buffer) {
  if (buffer.byteLength < LAUD_HEADER_SIZE) {
    malformedPackets++;
    return;
  }

  const view = new DataView(buffer);
  const magic = view.getUint32(0, false);
  const version = view.getUint8(4);
  const flags = view.getUint8(5);
  const channels = view.getUint8(6);
  const sampleFormat = view.getUint8(7);
  const seq = view.getUint32(8, false);

  // Validation
  if (magic !== LAUD_MAGIC || version !== LAUD_VERSION || channels !== 2 || sampleFormat !== 1) {
    malformedPackets++;
    return;
  }

  packetsReceived++;
  lastSeq = seq;

  // Sequence tracking
  if (expectedSeq !== null) {
    if (seq > expectedSeq) {
      lostPackets += seq - expectedSeq;
    }
  }
  expectedSeq = (seq + 1) >>> 0;

  // Audio payload
  const pcmBytes = buffer.byteLength - LAUD_HEADER_SIZE;
  const frames = pcmBytes / 4; // 2 channels * 2 bytes/sample
  if (frames <= 0) return;

  const pcmView = new DataView(buffer, LAUD_HEADER_SIZE);
  const left = new Float32Array(frames);
  const right = new Float32Array(frames);

  const isSilent = (flags & FLAG_SILENCE) !== 0;
  if (!isSilent) {
    for (let i = 0; i < frames; i++) {
      left[i] = pcmView.getInt16(i * 4, true) / 32768;
      right[i] = pcmView.getInt16(i * 4 + 2, true) / 32768;
    }
  }

  if (audioSink) {
    audioSink.push(left, right);
  }

  if (packetsReceived % 10 === 0) {
    updateTelemetryUI();
  }
}

function handleRawPacket(buffer) {
  packetsReceived++;
  const frames = buffer.byteLength / 4;
  if (frames <= 0) return;

  const view = new DataView(buffer);
  const left = new Float32Array(frames);
  const right = new Float32Array(frames);

  for (let i = 0; i < frames; i++) {
    left[i] = view.getInt16(i * 4, true) / 32768;
    right[i] = view.getInt16(i * 4 + 2, true) / 32768;
  }

  if (audioSink) {
    audioSink.push(left, right);
  }

  if (packetsReceived % 10 === 0) {
    updateTelemetryUI();
  }
}

async function start() {
  if (active) return;
  userRequestedStop = false;
  reconnectAttempts = 0;
  expectedSeq = null;

  try {
    await ensureAudioContext();
    connectWebSocket();
  } catch (err) {
    setUIState(false, "Audio Error", err.message || "Failed to initialize AudioContext.");
  }
}

function stopListening() {
  userRequestedStop = true;
  if (reconnectTimer) {
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
  }

  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
    try {
      ws.send("STOP");
      ws.close();
    } catch (_) {}
  }
  ws = null;

  if (audioSink) {
    audioSink.clear();
  }

  setUIState(false, "Stopped", "Listening has stopped.");
}

bufferTargetSelect.addEventListener("change", () => {
  const targetMs = parseInt(bufferTargetSelect.value, 10) || 135;
  if (audioSink) {
    audioSink.setTarget(targetMs);
  }
});

resumeBtn.addEventListener("click", async () => {
  if (audioContext && audioContext.state === "suspended") {
    await audioContext.resume();
  }
});

listenBtn.addEventListener("click", start);
stopBtn.addEventListener("click", stopListening);
window.addEventListener("beforeunload", stopListening);
