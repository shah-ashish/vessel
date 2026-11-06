#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{Context, Result};
use axum::{
    extract::{
        connect_info::ConnectInfo,
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use futures_util::StreamExt;
use serde_json::json;
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

pub mod connection;
pub mod services;
pub mod ui;

use connection::discovery::run_discovery_responder;
use services::speaker::{capture_system_audio, AudioPacket, WavWriter};

const LISTEN_ADDR: &str = "0.0.0.0:8080";
const AUDIO_CHANNEL_CAPACITY: usize = 128;

#[derive(Clone)]
struct AppState {
    audio_tx: broadcast::Sender<AudioPacket>,
    connected_clients: Arc<AtomicUsize>,
    peak_levels: Arc<Mutex<(f32, f32)>>,
    is_streaming: Arc<AtomicBool>,
    receiver_active: Arc<AtomicBool>,
    speaker_active: Arc<AtomicBool>,
    receiver_info: Arc<Mutex<Option<ReceiverInfo>>>,
}

#[derive(Clone, Debug)]
struct ReceiverInfo {
    name: String,
    ip: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ClientStreamMode {
    Off,
    Laud,
    Raw,
}

#[derive(Debug, Clone)]
struct CliConfig {
    record_wav: Option<PathBuf>,
    headless: bool,
}

fn parse_cli_args() -> CliConfig {
    let mut args = std::env::args().skip(1);
    let mut record_wav = std::env::var("RECORD_WAV").ok().map(PathBuf::from);
    let mut headless = false;

    while let Some(arg) = args.next() {
        if arg == "--record" || arg == "-r" {
            if let Some(path) = args.next() {
                record_wav = Some(PathBuf::from(path));
            }
        } else if arg == "--headless" || arg == "-h" {
            headless = true;
        }
    }

    CliConfig { record_wav, headless }
}

fn get_local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let config = parse_cli_args();
    let (audio_tx, _) = broadcast::channel(AUDIO_CHANNEL_CAPACITY);
    let connected_clients = Arc::new(AtomicUsize::new(0));
    let peak_levels = Arc::new(Mutex::new((0.0f32, 0.0f32)));
    let is_streaming = Arc::new(AtomicBool::new(true));
    let receiver_active = Arc::new(AtomicBool::new(false));
    let speaker_active = Arc::new(AtomicBool::new(false));
    let receiver_info = Arc::new(Mutex::new(None));

    let state = AppState {
        audio_tx: audio_tx.clone(),
        connected_clients: connected_clients.clone(),
        peak_levels: peak_levels.clone(),
        is_streaming: is_streaming.clone(),
        receiver_active: receiver_active.clone(),
        speaker_active: speaker_active.clone(),
        receiver_info: receiver_info.clone(),
    };

    let wav_writer = match &config.record_wav {
        Some(path) => {
            info!("Enabling WAV recording to: {}", path.display());
            let writer = WavWriter::create(path, 48_000, 2)
                .with_context(|| format!("Failed to create WAV file at {}", path.display()))?;
            Some(Arc::new(Mutex::new(writer)))
        }
        None => None,
    };

    // Keep the existing, proven WASAPI capture pipeline unchanged.
    let tx_clone = audio_tx.clone();
    let peaks_clone = peak_levels.clone();
    std::thread::spawn(move || {
        if let Err(e) = capture_system_audio(tx_clone, wav_writer, peaks_clone) {
            error!("Audio capture stopped: {e:#}");
        }
    });

    tokio::spawn(async move {
        run_discovery_responder(8082, 8080).await;
    });

    // /ws remains a compatibility alias for the speaker stream, but now requires
    // an active receiver-control session and therefore cannot bypass single-device ownership.
    let app = Router::new()
        .route("/ws", get(speaker_websocket_handler))
        .route("/ws/control", get(control_websocket_handler))
        .route("/ws/speaker", get(speaker_websocket_handler))
        .route("/api/info", get(api_info_handler))
        .route("/api/status", get(api_status_handler))
        .with_state(state.clone());

    let local_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    let addr: SocketAddr = LISTEN_ADDR.parse()?;
    info!("Vessel server listening on http://127.0.0.1:8080");
    info!("LAN / Hotspot access: http://{}:8080", local_ip);

    if let Some(path) = &config.record_wav {
        info!("Audio recording active -> {}", path.display());
    }

    tokio::spawn(async move {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                if let Err(error) = axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .tcp_nodelay(true)
                .await
                {
                    error!("HTTP/WebSocket server stopped: {error}");
                }
            }
            Err(error) => error!("Could not bind Vessel server on {addr}: {error}"),
        }
    });

    if !config.headless {
        ui::run_native_gui(
            peak_levels,
            connected_clients,
            is_streaming,
            format!("http://{}:8080", local_ip),
        );
    } else {
        info!("Running in headless mode. Press Ctrl+C to stop.");
        tokio::signal::ctrl_c().await?;
    }

    Ok(())
}

async fn api_info_handler() -> impl IntoResponse {
    Json(json!({
        "app": "laptop-audio-stream",
        "product": "Vessel",
        "protocol": "LAUD_v1",
        "sample_rate": 48000,
        "channels": 2,
        "port": 8080,
        "routes": {
            "control": "/ws/control",
            "speaker": "/ws/speaker"
        }
    }))
}

async fn api_status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let info = state.receiver_info.lock().ok().and_then(|g| g.clone());
    let (peak_l, peak_r) = *state.peak_levels.lock().unwrap();

    Json(json!({
        "status": "running",
        "sample_rate": 48000,
        "channels": 2,
        "clients": state.connected_clients.load(Ordering::Relaxed),
        "receiver_connected": state.receiver_active.load(Ordering::Relaxed),
        "receiver_name": info.as_ref().map(|r| r.name.clone()),
        "receiver_ip": info.as_ref().map(|r| r.ip.clone()),
        "speaker_connected": state.speaker_active.load(Ordering::Relaxed),
        "speaker_streaming": state.is_streaming.load(Ordering::Relaxed),
        "ip": get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string()),
        "port": 8080,
        "peak_left": peak_l,
        "peak_right": peak_r
    }))
}

async fn control_websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
) -> Response {
    // Atomically reserve the one and only receiver slot before completing the upgrade.
    if state
        .receiver_active
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return (StatusCode::CONFLICT, "DEVICE_ALREADY_CONNECTED").into_response();
    }

    state.connected_clients.store(1, Ordering::SeqCst);
    if let Ok(mut current) = state.receiver_info.lock() {
        *current = Some(ReceiverInfo {
            name: "Android receiver".to_string(),
            ip: remote.ip().to_string(),
        });
    }

    let receiver_active = state.receiver_active.clone();
    let receiver_info = state.receiver_info.clone();
    let connected_clients = state.connected_clients.clone();

    ws.on_upgrade(move |socket| async move {
        handle_control_socket(
            socket,
            remote,
            receiver_active,
            receiver_info,
            connected_clients,
        )
        .await;
    })
}

async fn handle_control_socket(
    mut socket: WebSocket,
    remote: SocketAddr,
    receiver_active: Arc<AtomicBool>,
    receiver_info: Arc<Mutex<Option<ReceiverInfo>>>,
    connected_clients: Arc<AtomicUsize>,
) {
    let _ = socket.send(Message::Text("CONTROL_OK".to_string())).await;
    let mut ping_interval = tokio::time::interval(Duration::from_secs(10));
    ping_interval.tick().await;

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                if socket.send(Message::Ping(Vec::new())).await.is_err() {
                    break;
                }
            }
            incoming = socket.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        if let Some(name) = text.strip_prefix("DEVICE_NAME:") {
                            let safe_name: String = name
                                .chars()
                                .filter(|c| !c.is_control())
                                .take(48)
                                .collect();
                            if let Ok(mut current) = receiver_info.lock() {
                                *current = Some(ReceiverInfo {
                                    name: if safe_name.trim().is_empty() {
                                        "Android receiver".to_string()
                                    } else {
                                        safe_name
                                    },
                                    ip: remote.ip().to_string(),
                                });
                            }
                        } else if text == "DISCONNECT" {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(error)) => {
                        warn!("Receiver control socket failed: {error}");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    if let Ok(mut current) = receiver_info.lock() {
        *current = None;
    }
    connected_clients.store(0, Ordering::SeqCst);
    receiver_active.store(false, Ordering::SeqCst);
    info!("Receiver control disconnected: {}", remote);
}

async fn speaker_websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    if !state.receiver_active.load(Ordering::SeqCst) {
        return (StatusCode::PRECONDITION_FAILED, "NO_RECEIVER_CONTROL_SESSION").into_response();
    }

    if state
        .speaker_active
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return (StatusCode::CONFLICT, "SPEAKER_ALREADY_ACTIVE").into_response();
    }

    let tx = state.audio_tx.subscribe();
    let is_streaming = state.is_streaming.clone();
    let speaker_active = state.speaker_active.clone();

    ws.on_upgrade(move |socket| async move {
        handle_speaker_socket(socket, tx, is_streaming).await;
        speaker_active.store(false, Ordering::SeqCst);
    })
}

async fn handle_speaker_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<AudioPacket>,
    is_streaming: Arc<AtomicBool>,
) {
    let mut stream_mode = ClientStreamMode::Off;
    let mut ping_interval = tokio::time::interval(Duration::from_secs(5));
    ping_interval.tick().await;

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                if socket.send(Message::Ping(Vec::new())).await.is_err() {
                    break;
                }
            }
            incoming = socket.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match text.trim() {
                            "START:LAUD" | "START_LAUD" => {
                                stream_mode = ClientStreamMode::Laud;
                                info!("Speaker subscribed with LAUD v1 protocol");
                            }
                            "START" | "START:RAW" => stream_mode = ClientStreamMode::Raw,
                            "STOP" => stream_mode = ClientStreamMode::Off,
                            _ => {}
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(error)) => {
                        warn!("Speaker WebSocket failed: {error}");
                        break;
                    }
                    _ => {}
                }
            }
            packet = rx.recv(), if stream_mode != ClientStreamMode::Off => {
                match packet {
                    Ok(audio_packet) => {
                        if is_streaming.load(Ordering::Relaxed) {
                            let message = match stream_mode {
                                ClientStreamMode::Laud => Message::Binary(audio_packet.laud_frame.to_vec()),
                                ClientStreamMode::Raw => Message::Binary(audio_packet.raw_frame.to_vec()),
                                ClientStreamMode::Off => continue,
                            };
                            if socket.send(message).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!("Speaker client lagged, skipped {skipped} packets");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}
