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
use tower_http::services::ServeDir;
use tracing::{error, info, warn};

pub mod audio;
pub mod gui;
use audio::{
    downmix::ChannelDownmixer,
    protocol::{create_laud_packet, FLAG_DISCONTINUITY, FLAG_SILENCE},
    resampler::AudioResampler,
    wav_writer::WavWriter,
};

const LISTEN_ADDR: &str = "0.0.0.0:8080";
const AUDIO_CHANNEL_CAPACITY: usize = 128;

#[derive(Clone)]
pub struct AudioPacket {
    pub laud_frame: Arc<[u8]>, // 20-byte LAUD v1 header + PCM16 payload
    pub raw_frame: Arc<[u8]>,  // Legacy raw PCM16 payload
}

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
        if let Ok(socket) = tokio::net::UdpSocket::bind("0.0.0.0:8082").await {
            info!("Discovery UDP responder active on port 8082");
            let mut buf = [0u8; 128];
            while let Ok((len, addr)) = socket.recv_from(&mut buf).await {
                if &buf[..len] == b"LAUD_DISCOVER" {
                    let _ = socket.send_to(b"LAUD_HERE:8080", addr).await;
                }
            }
        }
    });

    let app = Router::new()
        .route("/ws", get(websocket_handler))
        .route("/api/info", get(api_info_handler))
        .route("/api/status", get(api_status_handler))
        .nest_service("/", ServeDir::new("web"))
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

fn capture_system_audio(
    tx: broadcast::Sender<AudioPacket>,
    wav_recorder: Option<Arc<Mutex<WavWriter>>>,
    peak_levels: Arc<Mutex<(f32, f32)>>,
) -> Result<()> {
    use wasapi::{initialize_mta, Direction, ShareMode};

    let _ = initialize_mta().ok();

    let device = wasapi::get_default_device(&Direction::Render)
        .map_err(|e| anyhow::anyhow!("Could not find the default Windows playback device: {e}"))?;

    let mut audio_client = device
        .get_iaudioclient()
        .map_err(|e| anyhow::anyhow!("Failed to get IAudioClient: {e}"))?;

    let mix_format = audio_client
        .get_mixformat()
        .map_err(|e| anyhow::anyhow!("Failed to get MixFormat: {e}"))?;

    let subformat = mix_format
        .get_subformat()
        .map_err(|e| anyhow::anyhow!("Failed to get subformat: {e}"))?;

    let sample_rate = mix_format.get_samplespersec();
    let channels = mix_format.get_nchannels() as usize;
    let channel_mask = mix_format.get_dwchannelmask();
    let bits_per_sample = mix_format.get_bitspersample();
    let blockalign = mix_format.get_blockalign() as usize;

    info!(
        "WASAPI Endpoint: {} Hz, {} channels, mask: {:#010x}, subformat: {:?}, {} bits",
        sample_rate, channels, channel_mask, subformat, bits_per_sample
    );

    let (def_time, min_time) = audio_client
        .get_periods()
        .map_err(|e| anyhow::anyhow!("Failed to get device periods: {e}"))?;

    // In shared loopback mode, allocate at least twice the engine default period (def_time)
    // or 20 ms (200,000 hns) to prevent audio driver buffer overflow and data discontinuities.
    let buffer_duration = (def_time * 2).max(200_000);
    info!(
        "WASAPI Buffer: def_time={} hns ({:.1}ms), min_time={} hns ({:.1}ms), requested={} hns ({:.1}ms)",
        def_time,
        def_time as f64 / 10_000.0,
        min_time,
        min_time as f64 / 10_000.0,
        buffer_duration,
        buffer_duration as f64 / 10_000.0
    );

    audio_client
        .initialize_client(
            &mix_format,
            buffer_duration,
            &Direction::Capture,
            &ShareMode::Shared,
            true,
        )
        .map_err(|e| anyhow::anyhow!("Failed to initialize audio client: {e}"))?;

    let h_event = audio_client
        .set_get_eventhandle()
        .map_err(|e| anyhow::anyhow!("Failed to set event handle: {e}"))?;

    let capture_client = audio_client
        .get_audiocaptureclient()
        .map_err(|e| anyhow::anyhow!("Failed to get audio capture client: {e}"))?;

    audio_client
        .start_stream()
        .map_err(|e| anyhow::anyhow!("Failed to start audio stream: {e}"))?;

    let mut packet_buffer = vec![0u8; 64 * 1024];
    let downmixer = ChannelDownmixer::new(channels, channel_mask);
    let mut resampler = AudioResampler::new(sample_rate)?;

    let mut sequence: u32 = 0;
    let mut timestamp_frames: u64 = 0;

    let mut raw_float_samples: Vec<f32> = Vec::with_capacity(4096);
    let mut stereo_float_samples: Vec<f32> = Vec::with_capacity(4096);
    let mut resampled_float_samples: Vec<f32> = Vec::with_capacity(4096);
    let mut pcm16_bytes: Vec<u8> = Vec::with_capacity(4096 * 4);

    loop {
        // When no audio is playing on the system, WASAPI loopback events time out.
        // Continue waiting instead of failing the capture thread.
        if h_event.wait_for_event(1000).is_err() {
            if let Ok(mut peaks) = peak_levels.lock() {
                peaks.0 = 0.0;
                peaks.1 = 0.0;
            }
            continue;
        }

        while let Some(nbr_frames) = capture_client
            .get_next_nbr_frames()
            .map_err(|e| anyhow::anyhow!("Failed to get next packet size: {e}"))?
        {
            if nbr_frames == 0 {
                break;
            }

            let required_size = (nbr_frames as usize) * blockalign;
            if packet_buffer.len() < required_size {
                packet_buffer.resize(required_size, 0);
            }

            let (frames, flags) = capture_client
                .read_from_device(&mut packet_buffer)
                .map_err(|e| anyhow::anyhow!("Failed to read audio from device: {e}"))?;

            let frame_count = frames as usize;
            let byte_count = frame_count * blockalign;
            if byte_count == 0 {
                continue;
            }

            let input_bytes = &packet_buffer[..byte_count];
            raw_float_samples.clear();

            let is_silent = flags.silent;
            let mut laud_flags = 0u8;
            if is_silent {
                laud_flags |= FLAG_SILENCE;
            }
            if flags.data_discontinuity {
                laud_flags |= FLAG_DISCONTINUITY;
            }

            if is_silent {
                raw_float_samples.resize(frame_count * channels, 0.0);
            } else {
                match (subformat, bits_per_sample) {
                    (wasapi::SampleType::Float, 32) => {
                        for chunk in input_bytes.chunks_exact(4) {
                            raw_float_samples
                                .push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                        }
                    }
                    (wasapi::SampleType::Int, 16) => {
                        for chunk in input_bytes.chunks_exact(2) {
                            let s = i16::from_le_bytes([chunk[0], chunk[1]]);
                            raw_float_samples.push(s as f32 / 32768.0);
                        }
                    }
                    (wasapi::SampleType::Int, 24) => {
                        for chunk in input_bytes.chunks_exact(3) {
                            let val = (chunk[0] as i32)
                                | ((chunk[1] as i32) << 8)
                                | ((chunk[2] as i8 as i32) << 16);
                            raw_float_samples.push(val as f32 / 8388608.0);
                        }
                    }
                    (wasapi::SampleType::Int, 32) => {
                        for chunk in input_bytes.chunks_exact(4) {
                            let s = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                            raw_float_samples.push(s as f32 / 2147483648.0);
                        }
                    }
                    _ => {
                        anyhow::bail!(
                            "Unsupported Windows mix format: {:?}, {} bits",
                            subformat,
                            bits_per_sample
                        );
                    }
                }
            }

            // Downmix to stereo
            downmixer.downmix_interleaved(&raw_float_samples, &mut stereo_float_samples);

            // Calculate peak amplitude for live VU meters
            let mut peak_l = 0.0f32;
            let mut peak_r = 0.0f32;
            for chunk in stereo_float_samples.chunks_exact(2) {
                let abs_l = chunk[0].abs();
                let abs_r = chunk[1].abs();
                if abs_l > peak_l {
                    peak_l = abs_l;
                }
                if abs_r > peak_r {
                    peak_r = abs_r;
                }
            }
            if let Ok(mut peaks) = peak_levels.lock() {
                peaks.0 = peak_l;
                peaks.1 = peak_r;
            }

            // Resample to 48 kHz
            resampler.process(&stereo_float_samples, &mut resampled_float_samples)?;

            let out_frames = resampled_float_samples.len() / 2;
            if out_frames == 0 {
                continue;
            }

            // Quantize to signed 16-bit PCM LE
            pcm16_bytes.clear();
            pcm16_bytes.reserve(out_frames * 4);
            for &s in &resampled_float_samples {
                let clamped = s.clamp(-1.0, 1.0);
                let pcm16 = (clamped * 32767.0) as i16;
                pcm16_bytes.extend_from_slice(&pcm16.to_le_bytes());
            }

            if let Some(recorder) = &wav_recorder {
                if let Ok(mut rec) = recorder.lock() {
                    let _ = rec.write_pcm_bytes(&pcm16_bytes);
                }
            }

            let laud_packet = create_laud_packet(
                laud_flags,
                sequence,
                timestamp_frames,
                &pcm16_bytes,
            );

            sequence = sequence.wrapping_add(1);
            timestamp_frames += out_frames as u64;

            let _ = tx.send(AudioPacket {
                laud_frame: Arc::from(laud_packet.into_boxed_slice()),
                raw_frame: Arc::from(pcm16_bytes.clone().into_boxed_slice()),
            });
        }
    }
}
