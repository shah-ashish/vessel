//! Stateful audio resampler with FIFO buffer support.
//!
//! Handles variable-sized input packets from WASAPI without phase resets or clicks
//! by queueing samples in a FIFO buffer and driving a persistent `rubato::Fft` resampler.

use anyhow::{Context, Result};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use std::collections::VecDeque;

pub const TARGET_SAMPLE_RATE: u32 = 48_000;
pub const RESAMPLER_CHANNELS: usize = 2;
const DEFAULT_CHUNK_SIZE: usize = 1024;

pub struct AudioResampler {
    input_sample_rate: u32,
    output_sample_rate: u32,
    resampler: Option<Fft<f32>>,
    // Input FIFO containing interleaved stereo samples (L, R, L, R...)
    input_fifo: VecDeque<f32>,
    // Pre-allocated scratch buffers for rubato
    in_chunk: Vec<f32>,
    out_chunk: Vec<f32>,
}

impl AudioResampler {
    pub fn new(input_sample_rate: u32) -> Result<Self> {
        let output_sample_rate = TARGET_SAMPLE_RATE;

        let (resampler, in_chunk, out_chunk) = if input_sample_rate == output_sample_rate {
            (None, Vec::new(), Vec::new())
        } else {
            let resampler = Fft::<f32>::new(
                input_sample_rate as usize,
                output_sample_rate as usize,
                DEFAULT_CHUNK_SIZE,
                RESAMPLER_CHANNELS,
                FixedSync::Input,
            )
            .with_context(|| {
                format!(
                    "Failed to construct rubato FFT resampler from {input_sample_rate} Hz to {output_sample_rate} Hz"
                )
            })?;

            let in_frames = resampler.input_frames_next();
            let out_frames_max = resampler.output_frames_max();

            let in_chunk = vec![0.0f32; in_frames * RESAMPLER_CHANNELS];
            let out_chunk = vec![0.0f32; out_frames_max * RESAMPLER_CHANNELS];

            (Some(resampler), in_chunk, out_chunk)
        };

        Ok(Self {
            input_sample_rate,
            output_sample_rate,
            resampler,
            input_fifo: VecDeque::with_capacity(8192),
            in_chunk,
            out_chunk,
        })
    }

    /// Feeds interleaved stereo samples into the resampler.
    /// Returns resampled interleaved stereo samples at 48 kHz.
    pub fn process(&mut self, input_stereo: &[f32], output_stereo: &mut Vec<f32>) -> Result<()> {
        output_stereo.clear();

        // 1. Direct passthrough path if native rate is already 48 kHz
        if self.input_sample_rate == self.output_sample_rate || self.resampler.is_none() {
            output_stereo.extend_from_slice(input_stereo);
            return Ok(());
        }

        // 2. Queue newly arrived samples into the input FIFO
        self.input_fifo.extend(input_stereo.iter().copied());

        let resampler = self.resampler.as_mut().unwrap();

        // 3. Process complete chunks from the FIFO
        while self.input_fifo.len() >= resampler.input_frames_next() * RESAMPLER_CHANNELS {
            let in_frames = resampler.input_frames_next();
            let in_samples = in_frames * RESAMPLER_CHANNELS;

            // Copy from FIFO to in_chunk
            for sample in self.in_chunk.iter_mut().take(in_samples) {
                *sample = self.input_fifo.pop_front().unwrap();
            }

            let input_adapter =
                InterleavedSlice::new(&self.in_chunk[..in_samples], RESAMPLER_CHANNELS, in_frames)
                    .map_err(|e| anyhow::anyhow!("Input adapter error: {e:?}"))?;

            let out_capacity_frames = self.out_chunk.len() / RESAMPLER_CHANNELS;
            let mut output_adapter = InterleavedSlice::new_mut(
                &mut self.out_chunk,
                RESAMPLER_CHANNELS,
                out_capacity_frames,
            )
            .map_err(|e| anyhow::anyhow!("Output adapter error: {e:?}"))?;

            let (_read, written_frames) = resampler
                .process_into_buffer(&input_adapter, &mut output_adapter, None)
                .map_err(|e| anyhow::anyhow!("Resampler processing error: {e:?}"))?;

            let written_samples = written_frames * RESAMPLER_CHANNELS;
            output_stereo.extend_from_slice(&self.out_chunk[..written_samples]);
        }

        Ok(())
    }

    pub fn input_sample_rate(&self) -> u32 {
        self.input_sample_rate
    }

    pub fn output_sample_rate(&self) -> u32 {
        self.output_sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_passthrough_48k() {
        let mut resampler = AudioResampler::new(48_000).expect("Constructor should succeed");
        let input = vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3];
        let mut output = Vec::new();

        resampler.process(&input, &mut output).unwrap();
        assert_eq!(output, input);
    }

    #[test]
    fn test_resampler_fifo_continuity_with_variable_chunks() {
        // Resample from 44.1 kHz to 48 kHz
        let mut resampler = AudioResampler::new(44_100).expect("Constructor should succeed");

        // Generate 44.1 kHz 440 Hz test sine wave (stereo)
        let sample_rate = 44_100.0;
        let freq = 440.0;
        let total_frames = 44100; // 1 second
        let mut test_signal = Vec::with_capacity(total_frames * 2);

        for i in 0..total_frames {
            let t = i as f32 / sample_rate;
            let val = (2.0 * std::f32::consts::PI * freq * t).sin();
            test_signal.push(val); // Left
            test_signal.push(val); // Right
        }

        // Feed test signal in irregular, arbitrary chunk sizes (simulating variable WASAPI packets)
        let chunk_sizes = [100, 333, 448, 512, 17, 892, 1024, 73, 256];
        let mut current_idx = 0;
        let mut resampled_output = Vec::new();
        let mut temp_out = Vec::new();

        let mut chunk_idx = 0;
        while current_idx < test_signal.len() {
            let chunk_frames = chunk_sizes[chunk_idx % chunk_sizes.len()];
            chunk_idx += 1;
            let chunk_samples = (chunk_frames * 2).min(test_signal.len() - current_idx);

            let slice = &test_signal[current_idx..current_idx + chunk_samples];
            current_idx += chunk_samples;

            resampler.process(slice, &mut temp_out).unwrap();
            resampled_output.extend_from_slice(&temp_out);
        }

        // Verify that resampled output has generated audio
        let out_frames = resampled_output.len() / 2;
        // Expected ~48000 frames minus resampler filter latency
        assert!(out_frames > 46000 && out_frames <= 48000);

        // Verify signal continuity: check that there are no giant spikes/discontinuities (> 0.2 sample-to-sample difference)
        for i in 1..out_frames {
            let prev_l = resampled_output[(i - 1) * 2];
            let curr_l = resampled_output[i * 2];
            let diff = (curr_l - prev_l).abs();
            // A 440 Hz sine wave at 48 kHz has max consecutive delta: 2 * pi * 440 / 48000 approx 0.057
            assert!(
                diff < 0.15,
                "Discontinuity detected at frame {i}: diff={diff}, prev={prev_l}, curr={curr_l}"
            );
        }
    }

    #[test]
    fn test_frequency_accuracy() {
        // Resample from 44.1 kHz to 48 kHz
        let mut resampler = AudioResampler::new(44_100).expect("Constructor should succeed");

        let input_rate = 44_100.0f32;
        let freq = 1000.0f32; // 1 kHz sine
        let frames = 44100;
        let mut input = Vec::with_capacity(frames * 2);

        for i in 0..frames {
            let t = i as f32 / input_rate;
            let val = (2.0 * std::f32::consts::PI * freq * t).sin();
            input.push(val);
            input.push(val);
        }

        let mut output = Vec::new();
        let mut chunk_out = Vec::new();

        // Feed in 448-frame packets (typical 10ms at 44.1 kHz)
        for chunk in input.chunks(448 * 2) {
            resampler.process(chunk, &mut chunk_out).unwrap();
            output.extend_from_slice(&chunk_out);
        }

        let out_frames = output.len() / 2;
        // Count zero crossings on Left channel (ignoring initial settling frames)
        let settle_frames = 2000;
        let mut zero_crossings = 0;
        for i in settle_frames..out_frames - 1 {
            let s1 = output[i * 2];
            let s2 = output[(i + 1) * 2];
            if (s1 <= 0.0 && s2 > 0.0) || (s1 >= 0.0 && s2 < 0.0) {
                zero_crossings += 1;
            }
        }

        let analyzed_duration = (out_frames - 1 - settle_frames) as f32 / 48000.0;
        let measured_freq = (zero_crossings as f32 / 2.0) / analyzed_duration;

        // Measured frequency must be within 0.5% of 1000 Hz
        assert!(
            (measured_freq - 1000.0).abs() < 5.0,
            "Frequency mismatch: expected ~1000 Hz, got {measured_freq} Hz"
        );
    }
}
