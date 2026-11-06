use anyhow::Result;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tracing::info;

use super::{
    downmix::ChannelDownmixer,
    protocol::{create_laud_packet, FLAG_DISCONTINUITY, FLAG_SILENCE},
    resampler::AudioResampler,
    wav_writer::WavWriter,
};

#[derive(Clone, Debug)]
pub struct AudioPacket {
    pub laud_frame: Arc<[u8]>,
    pub raw_frame: Arc<[u8]>,
}

pub fn capture_system_audio(
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
