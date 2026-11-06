#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{Context, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::StreamExt;
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
pub mod gui;
pub mod services;

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

    CliConfig {
        record_wav,
        headless,
    }
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

    let state = AppState {
        audio_tx: audio_tx.clone(),
        connected_clients: connected_clients.clone(),
        peak_levels: peak_levels.clone(),
        is_streaming: is_streaming.clone(),
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

    // Capture worker thread
    let tx_clone = audio_tx.clone();
    let peaks_clone = peak_levels.clone();
    std::thread::spawn(move || {
        if let Err(e) = capture_system_audio(tx_clone, wav_writer, peaks_clone) {
            error!("Audio capture stopped: {e:#}");
        }
    });

    // UDP auto-discovery responder for mobile client (port 8082)
    tokio::spawn(async move {
        run_discovery_responder(8082, 8080).await;
    });

    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .route("/api/info", get(api_info_handler))
        .route("/api/status", get(api_status_handler))
        .with_state(state.clone());

    let local_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    let addr: SocketAddr = LISTEN_ADDR.parse()?;
    info!("Server listening on http://127.0.0.1:8080");
    info!("LAN / Hotspot access: http://{}:8080", local_ip);

    if let Some(path) = &config.record_wav {
        info!("Audio recording active -> {}", path.display());
    }

    // Spawn HTTP and WebSocket server in background
    tokio::spawn(async move {
        if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
            let _ = axum::serve(listener, app).tcp_nodelay(true).await;
        }
    });

    // Run proper native desktop GUI application on main thread (with real Taskbar window)
    if !config.headless {
        gui::run_native_gui(
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
    axum::response::Response::builder()
        .header("content-type", "application/json")
        .header("access-control-allow-origin", "*")
        .body(
            r#"{"app":"laptop-audio-stream","protocol":"LAUD_v1","sample_rate":48000,"port":8080}"#
                .to_string(),
        )
        .unwrap()
}

async fn api_status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let clients = state.connected_clients.load(Ordering::Relaxed);
    let (peak_l, peak_r) = *state.peak_levels.lock().unwrap();
    let local_ip = get_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());

    let json = format!(
        r#"{{"status":"running","sample_rate":48000,"channels":2,"clients":{},"ip":"{}","port":8080,"peak_left":{:.4},"peak_right":{:.4}}}"#,
        clients, local_ip, peak_l, peak_r
    );

    axum::response::Response::builder()
        .header("content-type", "application/json")
        .header("access-control-allow-origin", "*")
        .body(json)
        .unwrap()
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let clients = state.connected_clients.clone();
    let is_streaming = state.is_streaming.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, state.audio_tx.subscribe(), clients, is_streaming))
}

async fn handle_socket(
    mut socket: WebSocket,
    mut rx: broadcast::Receiver<AudioPacket>,
    clients: Arc<AtomicUsize>,
    is_streaming: Arc<AtomicBool>,
) {
    clients.fetch_add(1, Ordering::SeqCst);
    let mut stream_mode = ClientStreamMode::Off;
    let mut ping_interval = tokio::time::interval(Duration::from_secs(5));
    // First tick fires immediately, consume it
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
                        let text = text.trim();
                        if text == "START:LAUD" || text == "START_LAUD" {
                            stream_mode = ClientStreamMode::Laud;
                            info!("Client subscribed with LAUD v1 protocol");
                        } else if text == "START" || text == "START:RAW" {
                            stream_mode = ClientStreamMode::Raw;
                            info!("Client subscribed with legacy Raw PCM protocol");
                        } else if text == "STOP" {
                            stream_mode = ClientStreamMode::Off;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            packet = rx.recv(), if stream_mode != ClientStreamMode::Off => {
                match packet {
                    Ok(audio_packet) => {
                        // Only send audio data if streaming is active (controlled from Native Desktop GUI)
                        if is_streaming.load(Ordering::Relaxed) {
                            let msg = match stream_mode {
                                ClientStreamMode::Laud => Message::Binary(audio_packet.laud_frame.to_vec()),
                                ClientStreamMode::Raw => Message::Binary(audio_packet.raw_frame.to_vec()),
                                ClientStreamMode::Off => unreachable!(),
                            };
                            if socket.send(msg).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!("WebSocket client lagged, skipped {skipped} audio packets");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    clients.fetch_sub(1, Ordering::SeqCst);
}
