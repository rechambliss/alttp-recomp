//! 128 KiB WRAM with the original layout: $7E:0000-$7F:FFFF.
//!
//! Addresses passed here are 17-bit offsets from $7E:0000 (0x00000-0x1FFFF).
//! Use [`Wram::long`] to convert a `$BB:AAAA` WRAM address.

pub const SIZE: usize = 0x2_0000;

/// Named WRAM addresses. Add entries as units establish them.
/// Every entry should also appear in harness `knowledge/ram-map.md`.
pub mod addr {
    pub const IRQ_ENABLE_REQ: u16 = 0x0128;
    pub const NMI_ALT_THREAD: u16 = 0x012A;
    pub const JOY1_HELD_BYSTUDLR: u16 = 0x00F0;
    pub const JOY1_HELD_AXLR: u16 = 0x00F2;
    pub const JOY1_NEW_BYSTUDLR: u16 = 0x00F4;
    pub const JOY1_NEW_AXLR: u16 = 0x00F6;
    pub const JOY1_PREV_BYSTUDLR: u16 = 0x00F8;
    pub const JOY1_PREV_AXLR: u16 = 0x00FA;
    pub const IRQ_BG3HOFS: u16 = 0x0630;
    pub const OAM_BUFFER_LOW: u16 = 0x0800;
    pub const OAM_BUFFER_HIGH: u16 = 0x0A00;
    pub const OAM_EXT_BITS: u16 = 0x0A20;
    pub const FRAME_UPLOAD_PTRS: u16 = 0x0AC0;
    /// $7E:C00D (bank $7E offset).
    pub const ANIM_TILE_TIMER_A: u16 = 0xC00D;
    pub const ANIM_TILE_OFS_A: u16 = 0xC00F;
    pub const ANIM_TILE_TIMER_B: u16 = 0xC013;
    pub const ANIM_TILE_IDX_B: u16 = 0xC015;
    pub const ALT_THREAD_SP: u16 = 0x1F0A;
}

pub struct Wram {
    bytes: Box<[u8; SIZE]>,
}

impl Wram {
    pub fn new() -> Self {
        Wram {
            bytes: Box::new([0; SIZE]),
        }
    }

    /// Converts `$7E:AAAA` / `$7F:AAAA` into an offset. Panics on other banks:
    /// a routine that reaches here with a non-WRAM bank is a translation bug.
    pub fn long(bank: u8, addr: u16) -> u32 {
        match bank {
            0x7E => addr as u32,
            0x7F => 0x1_0000 + addr as u32,
            _ => panic!("${bank:02X}:{addr:04X} is not a WRAM bank"),
        }
    }

    #[inline]
    pub fn r8(&self, a: u32) -> u8 {
        self.bytes[a as usize % SIZE]
    }

    #[inline]
    pub fn w8(&mut self, a: u32, v: u8) {
        self.bytes[a as usize % SIZE] = v;
    }

    /// Little-endian 16-bit read. The high byte comes from `a + 1`, wrapping at the end of WRAM.
    #[inline]
    pub fn r16(&self, a: u32) -> u16 {
        u16::from_le_bytes([self.r8(a), self.r8(a.wrapping_add(1))])
    }

    #[inline]
    pub fn w16(&mut self, a: u32, v: u16) {
        let [lo, hi] = v.to_le_bytes();
        self.w8(a, lo);
        self.w8(a.wrapping_add(1), hi);
    }

    pub fn as_bytes(&self) -> &[u8; SIZE] {
        &self.bytes
    }

    pub fn load(&mut self, src: &[u8; SIZE]) {
        self.bytes.copy_from_slice(src);
    }
}

impl Default for Wram {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn little_endian_16() {
        let mut w = Wram::new();
        w.w16(0x0022, 0xBEEF);
        assert_eq!(w.r8(0x0022), 0xEF);
        assert_eq!(w.r8(0x0023), 0xBE);
        assert_eq!(w.r16(0x0022), 0xBEEF);
    }

    #[test]
    fn long_addressing() {
        assert_eq!(Wram::long(0x7E, 0x0000), 0x0_0000);
        assert_eq!(Wram::long(0x7F, 0x0000), 0x1_0000);
        assert_eq!(Wram::long(0x7F, 0xFFFF), 0x1_FFFF);
    }

    #[test]
    #[should_panic]
    fn long_rejects_rom_bank() {
        Wram::long(0x00, 0x8000);
    }
}
