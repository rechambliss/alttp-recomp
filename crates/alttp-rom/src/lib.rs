//! Loads and verifies the player's own ROM, and maps SNES addresses to file offsets.
//! This crate never embeds ROM data. It only reads a file the user supplies.

use sha1::{Digest, Sha1};
use std::fmt;
use std::path::Path;

/// ROM dumps the project accepts. See harness `docs/03-legal-hygiene.md`.
pub const ACCEPTED: &[(&str, &str)] = &[("EU", "7c073a222569b9b8e8ca5fcb5dfec3b5e31da895")];

pub const ROM_SIZE: usize = 0x10_0000; // 1 MiB
const COPIER_HEADER: usize = 0x200;
const HEADER_OFFSET: usize = 0x7FC0; // LoROM internal header

#[derive(Debug)]
pub enum RomError {
    Io(std::io::Error),
    BadSize(usize),
    UnknownHash(String),
    NotLoRom(u16),
}

impl fmt::Display for RomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RomError::Io(e) => write!(f, "could not read ROM: {e}"),
            RomError::BadSize(n) => write!(f, "unexpected ROM size {n} bytes (want {ROM_SIZE})"),
            RomError::UnknownHash(h) => write!(f, "ROM SHA-1 {h} is not an accepted dump"),
            RomError::NotLoRom(a) => write!(f, "address ${a:04X} is not in LoROM ROM space"),
        }
    }
}

impl std::error::Error for RomError {}

impl From<std::io::Error> for RomError {
    fn from(e: std::io::Error) -> Self {
        RomError::Io(e)
    }
}

pub struct Rom {
    data: Vec<u8>,
    pub region: &'static str,
    pub sha1: String,
}

impl Rom {
    /// Load from disk, strip a 512-byte copier header if present, and check the hash.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, RomError> {
        Self::from_bytes(std::fs::read(path)?)
    }

    pub fn from_bytes(mut data: Vec<u8>) -> Result<Self, RomError> {
        if data.len() == ROM_SIZE + COPIER_HEADER {
            data.drain(..COPIER_HEADER);
        }
        if data.len() != ROM_SIZE {
            return Err(RomError::BadSize(data.len()));
        }
        let sha1 = hex(&Sha1::digest(&data));
        let region = ACCEPTED
            .iter()
            .find(|(_, h)| *h == sha1)
            .map(|(r, _)| *r)
            .ok_or_else(|| RomError::UnknownHash(sha1.clone()))?;
        Ok(Rom { data, region, sha1 })
    }

    pub fn read_u8(&self, bank: u8, addr: u16) -> Result<u8, RomError> {
        Ok(self.data[lorom_offset(bank, addr)?])
    }

    pub fn read_u16(&self, bank: u8, addr: u16) -> Result<u16, RomError> {
        let lo = self.read_u8(bank, addr)? as u16;
        let hi = self.read_u8(bank, addr.wrapping_add(1))? as u16;
        Ok(lo | hi << 8)
    }

    pub fn header(&self) -> Header {
        let h = &self.data[HEADER_OFFSET..HEADER_OFFSET + 0x40];
        let w = |o: usize| u16::from_le_bytes([h[o], h[o + 1]]);
        Header {
            title: String::from_utf8_lossy(&h[..21]).trim_end().to_string(),
            map_mode: h[0x15],
            cart_type: h[0x16],
            rom_size: h[0x17],
            sram_size: h[0x18],
            region: h[0x19],
            version: h[0x1B],
            checksum: w(0x1E),
            checksum_complement: w(0x1C),
            vec_nmi: w(0x2A),
            vec_irq: w(0x2E),
            vec_reset: w(0x3C),
        }
    }

    /// Sum of all bytes, as the cartridge header checksum defines it.
    pub fn computed_checksum(&self) -> u16 {
        self.data.iter().fold(0u16, |a, &b| a.wrapping_add(b as u16))
    }
}

#[derive(Debug, Clone)]
pub struct Header {
    pub title: String,
    pub map_mode: u8,
    pub cart_type: u8,
    pub rom_size: u8,
    pub sram_size: u8,
    pub region: u8,
    pub version: u8,
    pub checksum: u16,
    pub checksum_complement: u16,
    /// Native-mode NMI vector ($00:FFEA)
    pub vec_nmi: u16,
    /// Native-mode IRQ vector ($00:FFEE)
    pub vec_irq: u16,
    /// Emulation-mode RESET vector ($00:FFFC)
    pub vec_reset: u16,
}

/// LoROM: `$BB:AAAA` with `AAAA >= $8000` maps to `(BB & $7F) * $8000 + (AAAA - $8000)`.
/// Banks $80-$FF mirror $00-$7F.
pub fn lorom_offset(bank: u8, addr: u16) -> Result<usize, RomError> {
    if addr < 0x8000 {
        return Err(RomError::NotLoRom(addr));
    }
    let off = (bank as usize & 0x7F) * 0x8000 + (addr as usize - 0x8000);
    Ok(off % ROM_SIZE)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lorom_mapping() {
        assert_eq!(lorom_offset(0x00, 0x8000).unwrap(), 0x0000);
        assert_eq!(lorom_offset(0x00, 0xFFFF).unwrap(), 0x7FFF);
        assert_eq!(lorom_offset(0x01, 0x8000).unwrap(), 0x8000);
        assert_eq!(lorom_offset(0x1F, 0xFFFF).unwrap(), 0xF_FFFF);
        // FastROM-style mirror bank
        assert_eq!(lorom_offset(0x80, 0x8000).unwrap(), 0x0000);
        assert!(lorom_offset(0x00, 0x7FFF).is_err());
    }

    #[test]
    fn rejects_wrong_size() {
        assert!(matches!(Rom::from_bytes(vec![0; 1234]), Err(RomError::BadSize(1234))));
    }

    #[test]
    fn rejects_unknown_hash() {
        assert!(matches!(Rom::from_bytes(vec![0; ROM_SIZE]), Err(RomError::UnknownHash(_))));
    }

    /// Runs only when ALTTP_ROM points at the user's dump. Never runs in CI.
    #[test]
    fn real_rom_if_present() {
        let Ok(path) = std::env::var("ALTTP_ROM") else { return };
        let rom = Rom::load(path).expect("load");
        let h = rom.header();
        assert_eq!(h.map_mode, 0x20, "LoROM");
        assert_eq!(rom.computed_checksum(), h.checksum);
        assert_eq!(h.checksum ^ h.checksum_complement, 0xFFFF);
        assert_eq!(h.vec_reset, 0x8000);
    }
}
