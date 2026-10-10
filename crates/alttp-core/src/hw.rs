//! Hardware register traffic (PPU, DMA, APU ports, etc.).
//!
//! Translated code records every write in order instead of emulating hardware here,
//! so the verifier can diff register traffic against the original.
//! Reads of hardware registers that affect logic (joypad, APU ports, status flags)
//! come from `inputs`, which the test harness or platform layer fills in.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HwWrite {
    pub addr: u16,
    pub value: u8,
}

#[derive(Default)]
pub struct Hw {
    pub writes: Vec<HwWrite>,
    pub inputs: HwInputs,
}

#[derive(Default, Clone)]
pub struct HwInputs {
    /// $4218-$421F auto-joypad registers, as read by the game.
    pub joypad: [u16; 4],
}

impl Hw {
    #[inline]
    pub fn w8(&mut self, addr: u16, value: u8) {
        self.writes.push(HwWrite { addr, value });
    }

    #[inline]
    pub fn w16(&mut self, addr: u16, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        self.w8(addr, lo);
        self.w8(addr.wrapping_add(1), hi);
    }

    pub fn clear_log(&mut self) {
        self.writes.clear();
    }
}
