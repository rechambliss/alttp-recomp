//! 65816 register state that crosses routine boundaries, plus the stack.
//!
//! The stack lives in bank $00, which mirrors WRAM $7E:0000-$1FFF, so pushes and
//! pulls go straight into [`Wram`] at the original addresses.

use crate::wram::Wram;

/// P register bits.
pub mod flag {
    pub const C: u8 = 0x01;
    pub const Z: u8 = 0x02;
    pub const I: u8 = 0x04;
    pub const D: u8 = 0x08;
    pub const X: u8 = 0x10;
    pub const M: u8 = 0x20;
    pub const V: u8 = 0x40;
    pub const N: u8 = 0x80;
}

/// Native-mode CPU registers. `a`, `x` and `y` always hold all 16 bits;
/// the M/X flags in `p` say how much of them an instruction uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cpu {
    pub a: u16,
    pub x: u16,
    pub y: u16,
    pub s: u16,
    pub d: u16,
    pub db: u8,
    pub pb: u8,
    pub pc: u16,
    pub p: u8,
}

/// Maps a bank-$00 data address to a WRAM offset. Only $0000-$1FFF is WRAM there;
/// anything else reaching here is a translation bug.
#[inline]
pub fn bank0_wram(addr: u16) -> u32 {
    assert!(addr < 0x2000, "$00:{addr:04X} is not WRAM");
    addr as u32
}

impl Cpu {
    /// REP #imm.
    pub fn rep(&mut self, mask: u8) {
        self.p &= !mask;
    }

    /// SEP #imm. Setting X zeroes the high bytes of X and Y, as on hardware.
    pub fn sep(&mut self, mask: u8) {
        self.p |= mask;
        if self.p & flag::X != 0 {
            self.x &= 0x00FF;
            self.y &= 0x00FF;
        }
    }

    /// Sets N and Z from an 8-bit result.
    pub fn nz8(&mut self, v: u8) {
        self.p = (self.p & !(flag::N | flag::Z)) | (v & 0x80) | if v == 0 { flag::Z } else { 0 };
    }

    /// Sets N and Z from a 16-bit result.
    pub fn nz16(&mut self, v: u16) {
        self.p = (self.p & !(flag::N | flag::Z))
            | ((v >> 8) as u8 & 0x80)
            | if v == 0 { flag::Z } else { 0 };
    }

    pub fn push8(&mut self, w: &mut Wram, v: u8) {
        w.w8(bank0_wram(self.s), v);
        self.s = self.s.wrapping_sub(1);
    }

    pub fn pull8(&mut self, w: &Wram) -> u8 {
        self.s = self.s.wrapping_add(1);
        w.r8(bank0_wram(self.s))
    }

    /// High byte first, like PHA/PHX/PHY/PHD at 16 bits.
    pub fn push16(&mut self, w: &mut Wram, v: u16) {
        let [lo, hi] = v.to_le_bytes();
        self.push8(w, hi);
        self.push8(w, lo);
    }

    pub fn pull16(&mut self, w: &Wram) -> u16 {
        let lo = self.pull8(w);
        let hi = self.pull8(w);
        u16::from_le_bytes([lo, hi])
    }

    /// Native-mode RTI: pulls P, PC, PB. Pulling P with X set zeroes the
    /// high bytes of X and Y.
    pub fn rti(&mut self, w: &Wram) {
        self.p = self.pull8(w);
        if self.p & flag::X != 0 {
            self.x &= 0x00FF;
            self.y &= 0x00FF;
        }
        self.pc = self.pull16(w);
        self.pb = self.pull8(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pull_16_layout() {
        let mut w = Wram::new();
        let mut c = Cpu {
            s: 0x01FF,
            ..Default::default()
        };
        c.push16(&mut w, 0xBEEF);
        assert_eq!(c.s, 0x01FD);
        assert_eq!(w.r8(0x01FF), 0xBE);
        assert_eq!(w.r8(0x01FE), 0xEF);
        assert_eq!(c.pull16(&w), 0xBEEF);
        assert_eq!(c.s, 0x01FF);
    }

    #[test]
    fn sep_x_zeroes_index_high_bytes() {
        let mut c = Cpu {
            x: 0x1234,
            y: 0x5678,
            a: 0x9ABC,
            ..Default::default()
        };
        c.sep(flag::M | flag::X);
        assert_eq!((c.a, c.x, c.y), (0x9ABC, 0x0034, 0x0078));
    }
}
