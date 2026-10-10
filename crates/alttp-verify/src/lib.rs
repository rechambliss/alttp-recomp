//! Differential testing: run a routine in the reference emulator and in Rust from the
//! same starting state, then compare WRAM and hardware writes.
//!
//! The reference emulator is not chosen yet (harness decision O-001).
//! It plugs in through [`Reference`].

use alttp_core::hw::HwWrite;
use alttp_core::wram::SIZE;

/// CPU registers that cross a routine boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Regs {
    pub a: u16,
    pub x: u16,
    pub y: u16,
    pub d: u16,
    pub db: u8,
    pub p: u8,
}

pub struct Snapshot {
    pub wram: Box<[u8; SIZE]>,
    pub regs: Regs,
    pub hw_writes: Vec<HwWrite>,
}

/// A reference implementation that can run one original routine to completion.
pub trait Reference {
    /// Load `start`, set PC to `entry` (24-bit), run until the matching return,
    /// and capture the result.
    fn run_routine(&mut self, entry: u32, start: &Snapshot) -> Snapshot;
}

#[derive(Debug, PartialEq, Eq)]
pub enum Diff {
    Wram { addr: u32, expected: u8, actual: u8 },
    HwWrite { index: usize, expected: Option<HwWrite>, actual: Option<HwWrite> },
}

/// Compare WRAM, skipping `scratch` offsets from the unit spec. Returns every differing byte.
pub fn diff_wram(expected: &[u8; SIZE], actual: &[u8; SIZE], scratch: &[u32]) -> Vec<Diff> {
    expected
        .iter()
        .zip(actual.iter())
        .enumerate()
        .filter(|(i, (e, a))| e != a && !scratch.contains(&(*i as u32)))
        .map(|(i, (&e, &a))| Diff::Wram { addr: i as u32, expected: e, actual: a })
        .collect()
}

/// Compare hardware write logs in order.
pub fn diff_hw(expected: &[HwWrite], actual: &[HwWrite]) -> Vec<Diff> {
    (0..expected.len().max(actual.len()))
        .filter_map(|i| {
            let (e, a) = (expected.get(i).copied(), actual.get(i).copied());
            (e != a).then_some(Diff::HwWrite { index: i, expected: e, actual: a })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank() -> Box<[u8; SIZE]> {
        Box::new([0; SIZE])
    }

    #[test]
    fn wram_diff_reports_and_respects_scratch() {
        let e = blank();
        let mut a = blank();
        a[0x10] = 1;
        a[0x20] = 2;
        let d = diff_wram(&e, &a, &[0x20]);
        assert_eq!(d, vec![Diff::Wram { addr: 0x10, expected: 0, actual: 1 }]);
    }

    #[test]
    fn hw_diff_catches_missing_write() {
        let w = HwWrite { addr: 0x2100, value: 0x80 };
        let d = diff_hw(&[w], &[]);
        assert_eq!(d.len(), 1);
    }
}
