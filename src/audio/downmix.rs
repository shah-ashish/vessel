//! Channel downmixing module for Windows audio stream.
//!
//! Maps Windows audio channels to standard speaker positions according to
//! `WAVEFORMATEXTENSIBLE` `dwChannelMask`, and downmixes multi-channel audio
//! (Mono, Stereo, 5.1, 7.1) to stereo (Left, Right) using standard ITU-R BS.775 coefficients.

// Windows speaker mask bit definitions from ksmedia.h
pub const SPEAKER_FRONT_LEFT: u32 = 0x00000001;
pub const SPEAKER_FRONT_RIGHT: u32 = 0x00000002;
pub const SPEAKER_FRONT_CENTER: u32 = 0x00000004;
pub const SPEAKER_LOW_FREQUENCY: u32 = 0x00000008;
pub const SPEAKER_BACK_LEFT: u32 = 0x00000010;
pub const SPEAKER_BACK_RIGHT: u32 = 0x00000020;
pub const SPEAKER_FRONT_LEFT_OF_CENTER: u32 = 0x00000040;
pub const SPEAKER_FRONT_RIGHT_OF_CENTER: u32 = 0x00000080;
pub const SPEAKER_BACK_CENTER: u32 = 0x00000100;
pub const SPEAKER_SIDE_LEFT: u32 = 0x00000200;
pub const SPEAKER_SIDE_RIGHT: u32 = 0x00000400;

const INV_SQRT_2: f32 = 0.70710678; // -3 dB attenuation

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeakerPosition {
    FrontLeft,
    FrontRight,
    FrontCenter,
    LowFrequency,
    BackLeft,
    BackRight,
    FrontLeftOfCenter,
    FrontRightOfCenter,
    BackCenter,
    SideLeft,
    SideRight,
    Other,
}

#[derive(Debug, Clone)]
pub struct ChannelDownmixer {
    channel_count: usize,
    // Contribution [left_weight, right_weight] per input channel index
    weights: Vec<[f32; 2]>,
}

impl ChannelDownmixer {
    /// Constructs a downmixer given the number of channels and the Windows `dwChannelMask`.
    pub fn new(channel_count: usize, channel_mask: u32) -> Self {
        let positions = Self::resolve_speaker_positions(channel_count, channel_mask);
        let mut weights = Vec::with_capacity(channel_count);

        if channel_count == 1 {
            weights.push([1.0, 1.0]);
        } else {
            for pos in &positions {
                let (l, r) = match pos {
                    SpeakerPosition::FrontLeft => (1.0, 0.0),
                    SpeakerPosition::FrontRight => (0.0, 1.0),
                    SpeakerPosition::FrontCenter => (INV_SQRT_2, INV_SQRT_2),
                    SpeakerPosition::LowFrequency => (0.0, 0.0), // LFE omitted from stereo downmix per ITU-R BS.775
                    SpeakerPosition::BackLeft => (INV_SQRT_2, 0.0),
                    SpeakerPosition::BackRight => (0.0, INV_SQRT_2),
                    SpeakerPosition::BackCenter => (0.5, 0.5),
                    SpeakerPosition::SideLeft => (INV_SQRT_2, 0.0),
                    SpeakerPosition::SideRight => (0.0, INV_SQRT_2),
                    SpeakerPosition::FrontLeftOfCenter => (1.0, 0.0),
                    SpeakerPosition::FrontRightOfCenter => (0.0, 1.0),
                    SpeakerPosition::Other => (0.5, 0.5),
                };
                weights.push([l, r]);
            }
        }

        // Compute normalization factor to avoid clipping if channels > 2
        let (max_l, max_r) = weights
            .iter()
            .fold((0.0f32, 0.0f32), |(acc_l, acc_r), [l, r]| {
                (acc_l + l, acc_r + r)
            });

        let max_sum = max_l.max(max_r);
        let norm = if max_sum > 1.0 { 1.0 / max_sum } else { 1.0 };

        for w in &mut weights {
            w[0] *= norm;
            w[1] *= norm;
        }

        Self {
            channel_count,
            weights,
        }
    }

    /// Resolves speaker positions for each interleaved channel index.
    pub fn resolve_speaker_positions(channels: usize, mask: u32) -> Vec<SpeakerPosition> {
        let mut positions = Vec::with_capacity(channels);

        if mask != 0 {
            // Channel order in Windows follows the lowest to highest set bit
            let bit_mappings = [
                (SPEAKER_FRONT_LEFT, SpeakerPosition::FrontLeft),
                (SPEAKER_FRONT_RIGHT, SpeakerPosition::FrontRight),
                (SPEAKER_FRONT_CENTER, SpeakerPosition::FrontCenter),
                (SPEAKER_LOW_FREQUENCY, SpeakerPosition::LowFrequency),
                (SPEAKER_BACK_LEFT, SpeakerPosition::BackLeft),
                (SPEAKER_BACK_RIGHT, SpeakerPosition::BackRight),
                (
                    SPEAKER_FRONT_LEFT_OF_CENTER,
                    SpeakerPosition::FrontLeftOfCenter,
                ),
                (
                    SPEAKER_FRONT_RIGHT_OF_CENTER,
                    SpeakerPosition::FrontRightOfCenter,
                ),
                (SPEAKER_BACK_CENTER, SpeakerPosition::BackCenter),
                (SPEAKER_SIDE_LEFT, SpeakerPosition::SideLeft),
                (SPEAKER_SIDE_RIGHT, SpeakerPosition::SideRight),
            ];

            for (bit, pos) in bit_mappings {
                if (mask & bit) != 0 {
                    positions.push(pos);
                    if positions.len() == channels {
                        break;
                    }
                }
            }
        }

        // If mask was 0 or incomplete, populate remaining with standard defaults
        while positions.len() < channels {
            let idx = positions.len();
            let fallback = match (channels, idx) {
                (1, 0) => SpeakerPosition::FrontCenter,
                (2, 0) => SpeakerPosition::FrontLeft,
                (2, 1) => SpeakerPosition::FrontRight,
                // 5.1 default order
                (6, 0) => SpeakerPosition::FrontLeft,
                (6, 1) => SpeakerPosition::FrontRight,
                (6, 2) => SpeakerPosition::FrontCenter,
                (6, 3) => SpeakerPosition::LowFrequency,
                (6, 4) => SpeakerPosition::BackLeft,
                (6, 5) => SpeakerPosition::BackRight,
                // 7.1 default order
                (8, 0) => SpeakerPosition::FrontLeft,
                (8, 1) => SpeakerPosition::FrontRight,
                (8, 2) => SpeakerPosition::FrontCenter,
                (8, 3) => SpeakerPosition::LowFrequency,
                (8, 4) => SpeakerPosition::BackLeft,
                (8, 5) => SpeakerPosition::BackRight,
                (8, 6) => SpeakerPosition::SideLeft,
                (8, 7) => SpeakerPosition::SideRight,
                _ => SpeakerPosition::Other,
            };
            positions.push(fallback);
        }

        positions
    }

    /// Downmixes multi-channel interleaved float samples to stereo (L, R, L, R...).
    pub fn downmix_interleaved(&self, input: &[f32], output_stereo: &mut Vec<f32>) {
        if self.channel_count == 0 {
            return;
        }
        let frames = input.len() / self.channel_count;
        output_stereo.clear();
        output_stereo.reserve(frames * 2);

        // Fast path for standard stereo
        if self.channel_count == 2 && self.weights[0] == [1.0, 0.0] && self.weights[1] == [0.0, 1.0]
        {
            output_stereo.extend_from_slice(input);
            return;
        }

        // Fast path for mono
        if self.channel_count == 1 {
            let norm = self.weights[0][0];
            for &sample in input {
                let v = sample * norm;
                output_stereo.push(v);
                output_stereo.push(v);
            }
            return;
        }

        // General downmixing loop using precomputed channel weights
        for frame in 0..frames {
            let frame_start = frame * self.channel_count;
            let mut left = 0.0f32;
            let mut right = 0.0f32;

            for (ch, &[w_l, w_r]) in self.weights.iter().enumerate() {
                let s = input[frame_start + ch];
                left += s * w_l;
                right += s * w_r;
            }

            output_stereo.push(left.clamp(-1.0, 1.0));
            output_stereo.push(right.clamp(-1.0, 1.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mono_downmix() {
        let downmixer = ChannelDownmixer::new(1, 0);
        let input = vec![0.5, -0.5, 1.0];
        let mut output = Vec::new();
        downmixer.downmix_interleaved(&input, &mut output);

        assert_eq!(output.len(), 6);
        assert_eq!(output, vec![0.5, 0.5, -0.5, -0.5, 1.0, 1.0]);
    }

    #[test]
    fn test_stereo_passthrough() {
        let downmixer = ChannelDownmixer::new(2, SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT);
        let input = vec![0.1, 0.2, -0.3, -0.4];
        let mut output = Vec::new();
        downmixer.downmix_interleaved(&input, &mut output);

        assert_eq!(output, input);
    }

    #[test]
    fn test_5_1_downmix_preserves_center() {
        // 5.1 layout: FL, FR, FC, LFE, BL, BR
        let mask = SPEAKER_FRONT_LEFT
            | SPEAKER_FRONT_RIGHT
            | SPEAKER_FRONT_CENTER
            | SPEAKER_LOW_FREQUENCY
            | SPEAKER_BACK_LEFT
            | SPEAKER_BACK_RIGHT;

        let downmixer = ChannelDownmixer::new(6, mask);
        // Only Front Center has 1.0 signal, all other channels 0.0
        let input = vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
        let mut output = Vec::new();
        downmixer.downmix_interleaved(&input, &mut output);

        assert_eq!(output.len(), 2);
        // Both Left and Right should receive equal center energy > 0
        assert!(output[0] > 0.0);
        assert!((output[0] - output[1]).abs() < 1e-6);
    }

    #[test]
    fn test_7_1_downmix_mapping() {
        let mask = SPEAKER_FRONT_LEFT
            | SPEAKER_FRONT_RIGHT
            | SPEAKER_FRONT_CENTER
            | SPEAKER_LOW_FREQUENCY
            | SPEAKER_BACK_LEFT
            | SPEAKER_BACK_RIGHT
            | SPEAKER_SIDE_LEFT
            | SPEAKER_SIDE_RIGHT;

        let downmixer = ChannelDownmixer::new(8, mask);
        let positions = ChannelDownmixer::resolve_speaker_positions(8, mask);

        assert_eq!(positions[0], SpeakerPosition::FrontLeft);
        assert_eq!(positions[1], SpeakerPosition::FrontRight);
        assert_eq!(positions[2], SpeakerPosition::FrontCenter);
        assert_eq!(positions[3], SpeakerPosition::LowFrequency);
        assert_eq!(positions[4], SpeakerPosition::BackLeft);
        assert_eq!(positions[5], SpeakerPosition::BackRight);
        assert_eq!(positions[6], SpeakerPosition::SideLeft);
        assert_eq!(positions[7], SpeakerPosition::SideRight);

        // Side Left signal should only appear on Left output
        let input = vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let mut output = Vec::new();
        downmixer.downmix_interleaved(&input, &mut output);

        assert_eq!(output.len(), 2);
        assert!(output[0] > 0.0);
        assert_eq!(output[1], 0.0);
    }
}
