//! Standard RIFF WAV file writer for local audio recording verification.
//!
//! Writes stereo 16-bit PCM WAV files at 48 kHz, updating header chunk sizes
//! upon completion or drop.

use anyhow::Result;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

pub struct WavWriter {
    file: File,
    data_bytes: u32,
    unflushed_bytes: u32,
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
}

impl WavWriter {
    pub fn create<P: AsRef<Path>>(path: P, sample_rate: u32, channels: u16) -> Result<Self> {
        let mut file = File::create(path)?;
        let bits_per_sample = 16u16;

        // Write placeholder 44-byte header
        let header = [0u8; 44];
        file.write_all(&header)?;

        let mut writer = Self {
            file,
            data_bytes: 0,
            unflushed_bytes: 0,
            sample_rate,
            channels,
            bits_per_sample,
        };
        writer.update_header()?;
        Ok(writer)
    }

    /// Writes raw interleaved 16-bit PCM bytes (L, R, L, R...).
    pub fn write_pcm_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.file.write_all(bytes)?;
        self.data_bytes += bytes.len() as u32;
        self.unflushed_bytes += bytes.len() as u32;

        // Periodically update the header (every ~0.5s of 48k stereo audio)
        // so that the WAV file is valid even if the process is terminated abruptly.
        if self.unflushed_bytes >= 96_000 {
            self.update_header()?;
            self.unflushed_bytes = 0;
        }
        Ok(())
    }

    /// Updates RIFF and data chunk headers on disk.
    pub fn update_header(&mut self) -> Result<()> {
        let block_align = self.channels * (self.bits_per_sample / 8);
        let byte_rate = self.sample_rate * block_align as u32;
        let riff_chunk_size = 36 + self.data_bytes;

        let mut header = [0u8; 44];
        // "RIFF"
        header[0..4].copy_from_slice(b"RIFF");
        header[4..8].copy_from_slice(&riff_chunk_size.to_le_bytes());
        // "WAVE"
        header[8..12].copy_from_slice(b"WAVE");
        // "fmt " chunk
        header[12..16].copy_from_slice(b"fmt ");
        header[16..20].copy_from_slice(&16u32.to_le_bytes()); // subchunk1 size (16 for PCM)
        header[20..22].copy_from_slice(&1u16.to_le_bytes()); // AudioFormat (1 = PCM)
        header[22..24].copy_from_slice(&self.channels.to_le_bytes());
        header[24..28].copy_from_slice(&self.sample_rate.to_le_bytes());
        header[28..32].copy_from_slice(&byte_rate.to_le_bytes());
        header[32..34].copy_from_slice(&block_align.to_le_bytes());
        header[34..36].copy_from_slice(&self.bits_per_sample.to_le_bytes());
        // "data" chunk
        header[36..40].copy_from_slice(b"data");
        header[40..44].copy_from_slice(&self.data_bytes.to_le_bytes());

        let current_pos = self.file.stream_position()?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header)?;
        self.file.seek(SeekFrom::Start(current_pos))?;
        self.file.flush()?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        self.update_header()
    }
}

impl Drop for WavWriter {
    fn drop(&mut self) {
        let _ = self.update_header();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn test_wav_file_structure() {
        let temp_dir = std::env::temp_dir();
        let test_wav = temp_dir.join("test_output.wav");

        {
            let mut writer = WavWriter::create(&test_wav, 48000, 2).unwrap();
            // Write 100 samples of 16-bit stereo (400 bytes)
            let data = vec![0x12u8; 400];
            writer.write_pcm_bytes(&data).unwrap();
            writer.finish().unwrap();
        }

        let mut file = File::open(&test_wav).unwrap();
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).unwrap();

        assert_eq!(buffer.len(), 44 + 400);
        assert_eq!(&buffer[0..4], b"RIFF");
        assert_eq!(&buffer[8..12], b"WAVE");
        assert_eq!(&buffer[12..16], b"fmt ");
        assert_eq!(&buffer[36..40], b"data");

        // Check RIFF size = 36 + 400 = 436
        let riff_size = u32::from_le_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]);
        assert_eq!(riff_size, 436);

        // Check data size = 400
        let data_size = u32::from_le_bytes([buffer[40], buffer[41], buffer[42], buffer[43]]);
        assert_eq!(data_size, 400);

        let _ = std::fs::remove_file(test_wav);
    }
}
