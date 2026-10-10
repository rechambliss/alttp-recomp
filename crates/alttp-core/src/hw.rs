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

/// One logged hardware read and the value the routine saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HwRead {
    pub addr: u16,
    pub value: u8,
}

#[derive(Default)]
pub struct Hw {
    pub writes: Vec<HwWrite>,
    pub inputs: HwInputs,
    /// Every [`Hw::r8`] in order, so tests can check acks and polls.
    pub reads: Vec<HwRead>,
}

#[derive(Default, Clone)]
pub struct HwInputs {
    /// $4218-$421F auto-joypad registers, as read by the game.
    pub joypad: [u16; 4],
    /// Read values for status registers read through [`Hw::r8`] ($4211, $4212, ...),
    /// keyed by address. Each read takes the front value; the last one stays and
    /// repeats, like a status register that holds its state. Reading an address
    /// with no values supplied panics: the harness must say what the hardware returns.
    pub read_seq: std::collections::HashMap<u16, std::collections::VecDeque<u8>>,
}

impl HwInputs {
    /// Sets the sequence of values that reads of `addr` return.
    pub fn set_reads(&mut self, addr: u16, values: &[u8]) {
        self.read_seq.insert(addr, values.iter().copied().collect());
    }
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

    /// Reads a hardware register from [`HwInputs::read_seq`] and logs it.
    pub fn r8(&mut self, addr: u16) -> u8 {
        let q = self
            .inputs
            .read_seq
            .get_mut(&addr)
            .filter(|q| !q.is_empty())
            .unwrap_or_else(|| panic!("no test value supplied for hw read ${addr:04X}"));
        let value = if q.len() > 1 {
            q.pop_front().unwrap()
        } else {
            q[0]
        };
        self.reads.push(HwRead { addr, value });
        value
    }

    pub fn clear_log(&mut self) {
        self.writes.clear();
        self.reads.clear();
    }
}
