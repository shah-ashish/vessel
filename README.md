# vessel

# Laptop Audio Stream (Windows 11)

A small Rust + browser application that captures Windows system playback audio through WASAPI loopback and streams it over WebSocket to browsers on the same LAN.

## Important status

This is a starter implementation. WASAPI crate APIs can vary by release, so compile it first and use the troubleshooting notes below if your installed crate version reports API mismatches. It supports common Windows shared-mode float32 and PCM16 output formats. It uses simple linear resampling; replace this with a high-quality resampler such as `rubato` before treating it as production audio software.

## Requirements

- Windows 11
- Rust stable (install from https://rustup.rs)
- Windows audio output device enabled
- Laptop and listener device on the same trusted Wi-Fi/LAN

## Run

From this folder:

```powershell
cargo run
```

Open on the laptop:

```text
http://127.0.0.1:8080
```

To listen from another device on the same network:

1. Find the laptop's LAN IPv4 address:

   ```powershell
   ipconfig
   ```

2. Open `http://LAPTOP-IP:8080` on the other device, replacing `LAPTOP-IP`.
3. If Windows Firewall asks, allow access on **Private networks only**.
4. Click **Listen**.

## Audio path

- Captures the default Windows render endpoint with WASAPI loopback.
- Converts common float32 or PCM16 mix formats to stereo signed 16-bit PCM at 48 kHz.
- Broadcasts audio packets to WebSocket clients.
- Each browser independently starts/stops its own playback.

## Security

This sample binds to `0.0.0.0:8080` so devices on your LAN can connect. Use it only on a trusted private network. It has no authentication or encryption. Do not port-forward it to the public internet. For shared or untrusted networks, add authentication, TLS, and access controls.

## Notes on sound and latency

- PCM 48 kHz stereo 16-bit is about 1.536 Mbps before WebSocket/TCP overhead.
- WebSocket over TCP can briefly stall when packets are lost; WebRTC/Opus is a better next step for internet streaming or tighter latency.
- The browser may apply its own output-device processing and latency.
- Windows output devices using exclusive mode may prevent shared-mode loopback capture.
- The app captures system playback, not microphone input.
- The sample currently does not implement a sophisticated jitter buffer, packet timestamps, automatic device reconnection, or high-quality resampling.

## Troubleshooting

- **No audio:** Confirm sound is playing through the Windows default output device.
- **Unsupported mix format:** Set Windows output format to 16-bit, 48 kHz or 32-bit float in Sound settings > More sound settings > Playback device > Properties > Advanced.
- **Browser cannot connect from another device:** Check both devices are on the same private network and allow `laptop-audio-stream.exe` / port 8080 through Windows Firewall for Private networks.
- **Compile errors in WASAPI calls:** Check the installed `wasapi` crate version/API docs and adjust the capture setup to that exact release.
