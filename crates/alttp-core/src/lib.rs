//! Game logic translated from the original routines.
//!
//! All state lives in [`State`]. WRAM keeps the original byte layout so it can be
//! diffed byte-for-byte against the original running in an emulator.
//! Routines are added under `bankXX` modules, one unit per function.
//! See harness `docs/01-methodology.md` and `docs/02-conventions.md`.

pub mod hw;
pub mod wram;

pub use hw::Hw;
pub use wram::Wram;

/// Everything a translated routine may touch.
pub struct State {
    pub wram: Wram,
    pub hw: Hw,
}

impl State {
    pub fn new() -> Self {
        State { wram: Wram::new(), hw: Hw::default() }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
