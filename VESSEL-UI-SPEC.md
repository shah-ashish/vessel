# Vessel UI, Connection, and Release Specification

**Repository:** https://github.com/shah-ashish/vessel  
**Target branch:** `feat/vessel-dashboard-and-updates`  
**Current release manifest:** `version.json`

## Product goal

Vessel has two user-facing applications:
- Android is a receiver client that discovers and connects to the Vessel server, then independently starts or stops individual services.
- Windows is the host/server control panel. It controls the server lifecycle, shows connected receivers, and exposes service controls and audio diagnostics.

This work is a focused product and architecture improvement, not a rewrite of the proven audio pipeline.

## Non-negotiable preservation rules

1. Preserve the WASAPI capture implementation, audio framing/protocol, resampling, Android jitter buffer, and AudioTrack playback path unless a change is required to separate the control and speaker lifecycles.
2. Do not regress measured audio latency, throughput, packet-loss handling, or reconnection behavior.
3. Do not stop the server, close a receiver-control connection, or clear selected device state when the speaker service is turned off.
4. Do not present fabricated IP addresses, latency values, device names, service states, or packet statistics.
5. Keep existing routes compatible during migration where practical.

## Independent state model

The UI and backend must represent three independent concepts:

| State | Meaning | Owner |
|---|---|---|
| Server | Vessel host process and network listener are running | Windows host |
| Receiver | Exactly one Android receiver owns the active device-control session | Server control API |
| Service | An individual feature such as Speaker is enabled/connected | Feature endpoint |

Turning Speaker OFF closes only the speaker stream. It must not disconnect the selected receiver or shut down the server.

## Single receiver policy

- The server accepts at most one active receiver-control session at a time.
- The server, not only the Android UI, enforces this limit atomically.
- A second receiver receives a stable, machine-readable rejection reason such as `DEVICE_ALREADY_CONNECTED`.
- A rejected receiver must not disturb the active receiver.
- Disconnecting the active receiver releases the single-device slot.
- Speaker ON/OFF does not acquire or release the receiver slot.
- The server dashboard shows the active receiver's reported name, remote address when available, and connected/disconnected status. When empty, show an honest empty state.

## Android screen flow

### Screen 1 — Device discovery

- App name: Vessel.
- Settings entry.
- Available devices section with a selectable device list.
- Each device item shows discovered name, host/IP, server port, and availability state.
- A scan/rescan state and clear empty/error state.
- Selecting a device starts the control-session handshake. It does not start Speaker.
- If another receiver already owns the server, show the server's rejection reason.

### Screen 2 — Connected device

- Vessel header and settings.
- Connected device information and real connection state.
- Explicit Disconnect button.
- Services section, beginning with Speaker.
- Speaker icon/button has independent ON/OFF/CONNECTING/ERROR states.
- Bottom notification/status area for transient messages and real errors.
- The selected device remains on this screen while Speaker is OFF.

### Android navigation/state rules

- Screen selection is driven by the receiver-control connection, never by Speaker WebSocket state.
- Speaker OFF returns Speaker to OFF while the receiver remains connected.
- Only explicit Disconnect or a lost control connection returns to discovery.
- Discovery result, selected receiver, control socket, and speaker socket have separate state holders.

## Windows desktop dashboard

The desktop is the server control panel, not a mirror of the Android UI.

Always show:
1. Vessel server status and server address/port.
2. Server Start/Stop controls.
3. Connected receiver section, including a real empty state when there are no receivers.
4. Services section, with independent Speaker service status and control.
5. Live stereo VU meters and real audio diagnostics.
6. Copy server address.
7. Installed version and update availability.

Server status, receiver count, service state, and VU data must come from actual backend state. Closing/minimizing the dashboard must not accidentally stop the server unless the user explicitly chooses Exit/Stop.

## Network route direction

The intended architecture uses one server port (8080) and separate routes/sockets:

- `GET /api/info` — discovery and static server capabilities.
- `GET /api/status` — live server and receiver status.
- `/ws/control` — receiver registration, heartbeat, and connection ownership.
- `/ws/speaker` — speaker-only audio stream.
- Future independent endpoints may include `/ws/mic`, `/ws/camera`, and `/ws/input`.

The current implementation uses `/ws`. Preserve a compatibility path until the new clients and server have migrated. Do not split features across extra TCP ports merely to get independent lifecycles.

## Visual design — Midnight Control

Use a cohesive dark technical dashboard across platforms.

| Token | Value | Usage |
|---|---|---|
| Background | `#0B1220` | App/window background |
| Surface | `#121C2E` | Main cards |
| Elevated surface | `#1C2940` | Nested cards/hover |
| Primary accent | `#38BDF8` | Selection and primary actions |
| Running | `#22C55E` | Confirmed running/connected |
| Connecting | `#F59E0B` | Transitional states |
| Error | `#F87171` | Errors and destructive actions |
| Secondary text | `#94A3B8` | Supporting information |
| Primary text | `#E2E8F0` | Main text |

Use status colours only when supported by real state. Keep typography readable, spacing consistent, touch targets accessible, and motion restrained. Android should be compact and touch-first; Windows should be information-dense and show server, receivers, services, and audio activity together.

## Application identity

- Android `applicationId`: `io.github.shahashish.vessel`.
- Android source `namespace`: `com.laptopstream.audio` may remain unchanged; do not rename every Kotlin package solely to change install identity.
- Display name: `Vessel`.
- Treat the new application ID as a distinct Android app from the previous `com.laptopstream.audio` build. The first install of this new ID does not update or remove the old app.
- Keep the new application ID permanent once the first release is distributed.
- Sign every release with the same protected release keystore. Never commit keystore files/passwords.
- Keep Android `versionCode` increasing for each release.

## Application icon

Use the exact user-supplied artwork for:
- Android adaptive launcher icon (foreground/background assets and round icon where needed).
- Windows executable icon resource and installer/shortcut icon.

Do not invent a replacement icon. The shared Gemini URL did not expose a downloadable image in the connected environment; source artwork must be supplied as an image file before the branded assets can be generated.

## Release and update policy

- Root `version.json` is the release-version source of truth.
- Publish installers through GitHub Releases.
- Release tags must match `version.json.releaseTag` and `v` + `version.json.version`.
- Release assets: signed Android APK, Windows NSIS installer, and SHA-256 checksum file.
- Android and Windows check GitHub Releases when the app starts/restarts only. No periodic polling and no push notification infrastructure in this release.
- If a newer release exists, show a clear in-app/system notification with version and release notes; let the user choose Download/Install.
- Download from the official GitHub Release asset, verify SHA-256 before install, and report checksum/download failures.
- Android uses the OS package installer; user approval may be required. Do not claim silent installation.
- Windows updater must close/restart the app safely, replace the existing installation, and preserve user data/configuration.
- Do not install a release whose version is not newer than the installed version.

## Windows installation

- The NSIS installer must explicitly ask whether to create a desktop shortcut.
- The desktop shortcut is optional and should be controlled by a checkbox on the Components page.
- Start Menu shortcuts remain part of the normal installation.
- The uninstaller removes the optional shortcut if present.

## Acceptance tests

1. A device can be discovered and selected without starting Speaker.
2. Exactly one receiver-control session can be active on a server.
3. A second receiver is rejected without disconnecting the first.
4. Speaker ON opens only the speaker route.
5. Speaker OFF closes only the speaker route; the device remains connected.
6. Explicit Disconnect releases the receiver slot and returns Android to discovery.
7. Windows shows server state even with zero receivers.
8. Windows lists the connected receiver when present and a clear empty state when absent.
9. Server Stop, Speaker Stop, and receiver Disconnect are distinct operations.
10. Android checks for updates only at app launch/restart.
11. Android rejects an APK with an invalid checksum or incompatible package/signing identity.
12. Windows checks for updates only at app launch/restart and verifies its installer before running it.
13. Windows installer presents an optional desktop-shortcut checkbox.
14. Android and Windows display the supplied Vessel icon artwork.
15. Build/test workflows pass for both platforms; audio pipeline regression tests remain green.
