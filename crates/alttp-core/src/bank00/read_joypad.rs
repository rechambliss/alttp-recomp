//! $00:83D1 ReadJoypad

use crate::cpu::bank0_wram;
use crate::wram::addr;
use crate::{Cpu, State};

/// $00:83D1 ReadJoypad
/// Spec: ledger/units/0083d1.toml
///
/// Reads controller 1 from the auto-joypad registers into the held ($F0/$F2),
/// new-press ($F4/$F6) and previous ($F8/$FA) bytes, and leaves the raw bytes
/// in dp $00/$01. Entered and left at m8x8 with D=$0000 (JSR/RTS; the return
/// address is not modelled). Leaves A low = $F4 value (B kept), Y = raw $4219,
/// X unchanged, N/Z from the final AND, other flags unchanged.
pub fn read_joypad(s: &mut State) {
    let State { wram, hw, cpu } = s;
    let dp = |cpu: &Cpu, off: u16| bank0_wram(cpu.d.wrapping_add(off));

    // $83D1 STZ $4016
    // QUIRK: clears the manual-read latch although only auto-read results are used.
    hw.w8(0x4016, 0x00);
    // $83D4 LDA $4218 / STA $00
    // QUIRK: no wait on $4212 auto-read busy; takes whatever the registers hold.
    let v = hw.r8(0x4218);
    lda8(cpu, v);
    wram.w8(dp(cpu, 0x00), v);
    // $83D9 LDA $4219 / STA $01
    let v = hw.r8(0x4219);
    lda8(cpu, v);
    wram.w8(dp(cpu, 0x01), v);

    // $83DE LDA $00 / STA $F2 / TAY
    let v = wram.r8(dp(cpu, 0x00));
    lda8(cpu, v);
    wram.w8(dp(cpu, addr::JOY1_HELD_AXLR), v);
    tay8(cpu);
    // $83E3 EOR $FA / AND $F2 / STA $F6
    let v = (cpu.a as u8) ^ wram.r8(dp(cpu, addr::JOY1_PREV_AXLR));
    lda8(cpu, v);
    let v = (cpu.a as u8) & wram.r8(dp(cpu, addr::JOY1_HELD_AXLR));
    lda8(cpu, v);
    wram.w8(dp(cpu, addr::JOY1_NEW_AXLR), v);
    // $83E9 STY $FA
    wram.w8(dp(cpu, addr::JOY1_PREV_AXLR), cpu.y as u8);

    // $83EB LDA $01 / STA $F0 / TAY
    let v = wram.r8(dp(cpu, 0x01));
    lda8(cpu, v);
    wram.w8(dp(cpu, addr::JOY1_HELD_BYSTUDLR), v);
    tay8(cpu);
    // $83F0 EOR $F8 / AND $F0 / STA $F4
    let v = (cpu.a as u8) ^ wram.r8(dp(cpu, addr::JOY1_PREV_BYSTUDLR));
    lda8(cpu, v);
    let v = (cpu.a as u8) & wram.r8(dp(cpu, addr::JOY1_HELD_BYSTUDLR));
    lda8(cpu, v);
    wram.w8(dp(cpu, addr::JOY1_NEW_BYSTUDLR), v);
    // $83F6 STY $F8
    wram.w8(dp(cpu, addr::JOY1_PREV_BYSTUDLR), cpu.y as u8);
    // $83F8 RTS
}

/// 8-bit A result (LDA/EOR/AND at m8): replaces the low byte of A, sets N/Z.
fn lda8(cpu: &mut Cpu, v: u8) {
    cpu.a = (cpu.a & 0xFF00) | v as u16;
    cpu.nz8(v);
}

/// TAY at x8: Y = low byte of A (high byte zero), sets N/Z.
fn tay8(cpu: &mut Cpu) {
    cpu.y = cpu.a & 0x00FF;
    cpu.nz8(cpu.y as u8);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::flag;
    use crate::hw::{HwRead, HwWrite};

    struct Out {
        raw: [u8; 2],
        held: [u8; 2],
        new: [u8; 2],
        prev: [u8; 2],
    }

    /// Runs with JOY1L/JOY1H = `lo`/`hi` and old prev $FA/$F8 = `prev_lo`/`prev_hi`.
    fn run(cpu: Cpu, lo: u8, hi: u8, prev_lo: u8, prev_hi: u8) -> (State, Out) {
        let mut s = State::new();
        s.cpu = cpu;
        s.wram.w8(addr::JOY1_PREV_AXLR as u32, prev_lo);
        s.wram.w8(addr::JOY1_PREV_BYSTUDLR as u32, prev_hi);
        s.hw.inputs.set_reads(0x4218, &[lo]);
        s.hw.inputs.set_reads(0x4219, &[hi]);
        read_joypad(&mut s);
        let r = |a: u16| s.wram.r8(a as u32);
        let out = Out {
            raw: [r(0x00), r(0x01)],
            held: [r(addr::JOY1_HELD_AXLR), r(addr::JOY1_HELD_BYSTUDLR)],
            new: [r(addr::JOY1_NEW_AXLR), r(addr::JOY1_NEW_BYSTUDLR)],
            prev: [r(addr::JOY1_PREV_AXLR), r(addr::JOY1_PREV_BYSTUDLR)],
        };
        assert_eq!(
            s.hw.writes,
            vec![HwWrite {
                addr: 0x4016,
                value: 0
            }]
        );
        assert_eq!(
            s.hw.reads,
            vec![
                HwRead {
                    addr: 0x4218,
                    value: lo
                },
                HwRead {
                    addr: 0x4219,
                    value: hi
                },
            ]
        );
        (s, out)
    }

    fn m8x8() -> Cpu {
        Cpu {
            p: flag::M | flag::X,
            s: 0x01FF,
            ..Default::default()
        }
    }

    #[test]
    fn nothing_pressed() {
        let (s, o) = run(m8x8(), 0, 0, 0, 0);
        assert_eq!(
            (o.raw, o.held, o.new, o.prev),
            ([0; 2], [0; 2], [0; 2], [0; 2])
        );
        assert_eq!(s.cpu.a, 0);
        assert_eq!(s.cpu.p & (flag::N | flag::Z), flag::Z);
    }

    #[test]
    fn new_press() {
        let (s, o) = run(m8x8(), 0xC0, 0x80, 0x40, 0x00);
        assert_eq!(o.raw, [0xC0, 0x80]);
        assert_eq!(o.held, [0xC0, 0x80]);
        assert_eq!(o.new, [0x80, 0x80]);
        assert_eq!(o.prev, [0xC0, 0x80]);
        assert_eq!((s.cpu.a, s.cpu.y), (0x0080, 0x0080));
        assert_eq!(s.cpu.p & (flag::N | flag::Z), flag::N);
    }

    #[test]
    fn held_not_new() {
        let (s, o) = run(m8x8(), 0xF0, 0xFF, 0xF0, 0xFF);
        assert_eq!(o.new, [0, 0]);
        assert_eq!(o.held, [0xF0, 0xFF]);
        assert_eq!(o.prev, [0xF0, 0xFF]);
        assert_eq!((s.cpu.a, s.cpu.y), (0x0000, 0x00FF));
        assert_eq!(s.cpu.p & (flag::N | flag::Z), flag::Z);
    }

    #[test]
    fn release() {
        let (s, o) = run(m8x8(), 0x00, 0x00, 0xFF, 0xFF);
        assert_eq!(o.new, [0, 0]);
        assert_eq!(o.prev, [0, 0]);
        assert_eq!(o.held, [0, 0]);
        assert_eq!(s.cpu.p & flag::Z, flag::Z);
    }

    #[test]
    fn random_and_pass_through() {
        // Small xorshift so the test is deterministic without extra crates.
        let mut seed: u32 = 0x1234_5678;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..500 {
            let r = next();
            let [lo, hi, plo, phi] = r.to_le_bytes();
            let r2 = next();
            let carried = (r2 as u8) & (flag::C | flag::V | flag::I | flag::D);
            let cpu = Cpu {
                a: (r2 >> 8) as u16 & 0xFF00 | 0x5A,
                x: (r2 >> 24) as u16,
                y: 0x0033,
                p: flag::M | flag::X | carried,
                ..m8x8()
            };
            let (s, o) = run(cpu, lo, hi, plo, phi);
            let new_lo = (lo ^ plo) & lo;
            let new_hi = (hi ^ phi) & hi;
            assert_eq!(o.raw, [lo, hi]);
            assert_eq!(o.held, [lo, hi]);
            assert_eq!(o.new, [new_lo, new_hi]);
            assert_eq!(o.prev, [lo, hi]);
            assert_eq!(s.cpu.a, (cpu.a & 0xFF00) | new_hi as u16);
            assert_eq!(s.cpu.y, hi as u16);
            assert_eq!(s.cpu.x, cpu.x);
            let nz = (new_hi & flag::N) | if new_hi == 0 { flag::Z } else { 0 };
            assert_eq!(s.cpu.p, flag::M | flag::X | carried | nz);
            assert_eq!((s.cpu.s, s.cpu.d, s.cpu.db), (cpu.s, cpu.d, cpu.db));
        }
    }
}
