use anyhow::{bail, Result};
use byteorder::{BigEndian, ByteOrder};

pub const LAUD_MAGIC: &[u8; 4] = b"LAUD";
pub const LAUD_VERSION: u8 = 1;
pub const LAUD_HEADER_SIZE: usize = 20;

// Flags
pub const FLAG_SILENCE: u8 = 0x01;
pub const FLAG_DISCONTINUITY: u8 = 0x02;

// Sample formats
pub const FORMAT_S16LE_48K: u8 = 0x01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaudHeader {
    pub version: u8,
    pub flags: u8,
    pub channels: u8,
    pub sample_format: u8,
    pub sequence: u32,
    pub timestamp_frames: u64,
}

impl LaudHeader {
    pub fn new(flags: u8, sequence: u32, timestamp_frames: u64) -> Self {
        Self {
            version: LAUD_VERSION,
            flags,
            channels: 2,
            sample_format: FORMAT_S16LE_48K,
            sequence,
            timestamp_frames,
        }
    }

    pub fn encode(&self, out: &mut [u8]) {
        assert!(out.len() >= LAUD_HEADER_SIZE);
        out[0..4].copy_from_slice(LAUD_MAGIC);
        out[4] = self.version;
        out[5] = self.flags;
        out[6] = self.channels;
        out[7] = self.sample_format;
        BigEndian::write_u32(&mut out[8..12], self.sequence);
        BigEndian::write_u64(&mut out[12..20], self.timestamp_frames);
    }

    pub fn decode(src: &[u8]) -> Result<Self> {
        if src.len() < LAUD_HEADER_SIZE {
            bail!("Payload too short for LAUD header: {} bytes", src.len());
        }
        if &src[0..4] != LAUD_MAGIC {
            bail!("Invalid LAUD magic bytes: {:?}", &src[0..4]);
        }
        let version = src[4];
        if version != LAUD_VERSION {
            bail!("Unsupported LAUD version: {version}");
        }
        let flags = src[5];
        let channels = src[6];
        let sample_format = src[7];
        let sequence = BigEndian::read_u32(&src[8..12]);
        let timestamp_frames = BigEndian::read_u64(&src[12..20]);

        Ok(Self {
            version,
            flags,
            channels,
            sample_format,
            sequence,
            timestamp_frames,
        })
    }
}

pub fn create_laud_packet(
    flags: u8,
    sequence: u32,
    timestamp_frames: u64,
    pcm_payload: &[u8],
) -> Vec<u8> {
    let mut packet = vec![0u8; LAUD_HEADER_SIZE + pcm_payload.len()];
    let header = LaudHeader::new(flags, sequence, timestamp_frames);
    header.encode(&mut packet[..LAUD_HEADER_SIZE]);
    packet[LAUD_HEADER_SIZE..].copy_from_slice(pcm_payload);
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_laud_header_roundtrip() {
        let header = LaudHeader::new(FLAG_SILENCE, 12345, 9876543210);
        let mut buf = [0u8; LAUD_HEADER_SIZE];
        header.encode(&mut buf);

        assert_eq!(&buf[0..4], b"LAUD");
        assert_eq!(buf[4], 1);
        assert_eq!(buf[5], FLAG_SILENCE);
        assert_eq!(buf[6], 2);
        assert_eq!(buf[7], FORMAT_S16LE_48K);

        let decoded = LaudHeader::decode(&buf).expect("Decode should succeed");
        assert_eq!(decoded, header);
        assert_eq!(decoded.sequence, 12345);
        assert_eq!(decoded.timestamp_frames, 9876543210);
    }

    #[test]
    fn test_invalid_magic_rejected() {
        let mut buf = [0u8; LAUD_HEADER_SIZE];
        buf[0..4].copy_from_slice(b"NOPE");
        assert!(LaudHeader::decode(&buf).is_err());
    }

    #[test]
    fn test_create_laud_packet() {
        let pcm = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let packet = create_laud_packet(0, 42, 1000, &pcm);
        assert_eq!(packet.len(), LAUD_HEADER_SIZE + pcm.len());

        let header = LaudHeader::decode(&packet).unwrap();
        assert_eq!(header.sequence, 42);
        assert_eq!(header.timestamp_frames, 1000);
        assert_eq!(&packet[LAUD_HEADER_SIZE..], &pcm[..]);
    }
}
