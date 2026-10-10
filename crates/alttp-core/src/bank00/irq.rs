//! $00:82D8 IRQ

use crate::cpu::{bank0_wram, flag};
use crate::wram::addr;
use crate::State;

/// $00:82D8 IRQ
/// Spec: ledger/units/0082d8.toml
///
/// Native-mode IRQ handler. Call with `s.cpu` as the CPU is at $00:82D8: the
/// hardware has already pushed PB, PC and P onto the stack, set I and PB=$00.
/// Returns after RTI with `s.cpu` holding the state RTI resumes into (the
/// interrupted code, or the other thread on the $012A path).
pub fn irq(s: &mut State) {
    let State { wram, hw, cpu } = s;

    // $82D8 SEI
    // QUIRK: redundant, interrupt entry already set I.
    cpu.p |= flag::I;
    // $82D9 REP #$30
    cpu.rep(flag::M | flag::X);
    // $82DB PHA / PHX / PHY / PHD (16-bit), PHB
    cpu.push16(wram, cpu.a);
    cpu.push16(wram, cpu.x);
    cpu.push16(wram, cpu.y);
    cpu.push16(wram, cpu.d);
    cpu.push8(wram, cpu.db);
    // $82E0 PHK / PLB: DB = PB = $00. Writes $00 to the stack byte below the frame.
    // QUIRK: D is saved but never set to 0 (unlike NMI); the body only uses absolute addressing.
    cpu.push8(wram, 0x00);
    cpu.db = cpu.pull8(wram);
    cpu.nz8(cpu.db);
    // $82E2 SEP #$30
    // QUIRK: zeroes X/Y high bytes; harmless, the 16-bit pulls restore them.
    cpu.sep(flag::M | flag::X);

    // $82E4 LDA $012A
    let v = wram.r8(bank0_wram(addr::NMI_ALT_THREAD));
    lda8(cpu, v);
    // $82E7 BNE L_00831F
    if v != 0 {
        // $831F LDA $4211
        // QUIRK: ack read on the thread path too; value unused, $0128 ignored.
        let v = hw.r8(0x4211);
        lda8(cpu, v);
        // $8322 REP #$30
        cpu.rep(flag::M | flag::X);
        // $8324 TSC / TAX
        cpu.a = cpu.s;
        cpu.nz16(cpu.a);
        cpu.x = cpu.a;
        cpu.nz16(cpu.x);
        // $8326 LDA $1F0A
        cpu.a = wram.r16(bank0_wram(addr::ALT_THREAD_SP));
        cpu.nz16(cpu.a);
        // $8329 TCS
        cpu.s = cpu.a;
        // $832A STX $1F0A
        wram.w16(bank0_wram(addr::ALT_THREAD_SP), cpu.x);
        // $832D PLB / PLD / PLY / PLX / PLA, $8332 RTI
        pull_frame_and_rti(s);
        return;
    }

    // $82E9 LDA $4211 (ack)
    let v = hw.r8(0x4211);
    lda8(cpu, v);
    // $82EC BPL L_008317
    // QUIRK: not a timer IRQ (bit 7 clear) skips the split even with $0128 != 0.
    if v & 0x80 != 0 {
        // $82EE LDA $0128 / $82F1 BEQ L_008317
        let v = wram.r8(bank0_wram(addr::IRQ_ENABLE_REQ));
        lda8(cpu, v);
        if v != 0 {
            // $82F3 BIT $4212 / BVC $82F3
            // QUIRK: unbounded spin until HBlank (bit 6); bit 7 alone does not exit.
            loop {
                let h = hw.r8(0x4212);
                cpu.p = (cpu.p & !(flag::N | flag::V | flag::Z))
                    | (h & (flag::N | flag::V))
                    | if (cpu.a as u8) & h == 0 { flag::Z } else { 0 };
                if h & 0x40 != 0 {
                    break;
                }
            }
            // $82F8 LDA $0630 / STA $2111 / LDA $0631 / STA $2111
            let v = wram.r8(bank0_wram(addr::IRQ_BG3HOFS));
            lda8(cpu, v);
            hw.w8(0x2111, v);
            let v = wram.r8(bank0_wram(addr::IRQ_BG3HOFS + 1));
            lda8(cpu, v);
            hw.w8(0x2111, v);
            // $8304 STZ $2112 x2
            // QUIRK: BG3VOFS always zeroed, not loaded from a mirror.
            hw.w8(0x2112, 0x00);
            hw.w8(0x2112, 0x00);
            // $830A LDA $0128 / BPL L_008317
            // QUIRK: $0128 is re-read here, not the value tested before the HBlank wait.
            let v = wram.r8(bank0_wram(addr::IRQ_ENABLE_REQ));
            lda8(cpu, v);
            if v & 0x80 != 0 {
                // $830F STZ $0128
                wram.w8(bank0_wram(addr::IRQ_ENABLE_REQ), 0x00);
                // $8312 LDA #$81 / STA $4200
                lda8(cpu, 0x81);
                hw.w8(0x4200, 0x81);
            }
        }
    }

    // $8317 REP #$30
    s.cpu.rep(flag::M | flag::X);
    // $8319 PLB / PLD / PLY / PLX / PLA, $831E RTI
    pull_frame_and_rti(s);
}

/// LDA at m8: replaces the low byte of A, sets N/Z.
fn lda8(cpu: &mut crate::Cpu, v: u8) {
    cpu.a = (cpu.a & 0xFF00) | v as u16;
    cpu.nz8(v);
}

/// PLB, then PLD/PLY/PLX/PLA at 16 bits (M/X already clear), then RTI.
fn pull_frame_and_rti(s: &mut State) {
    let State { wram, cpu, .. } = s;
    cpu.db = cpu.pull8(wram);
    cpu.nz8(cpu.db);
    cpu.d = cpu.pull16(wram);
    cpu.nz16(cpu.d);
    cpu.y = cpu.pull16(wram);
    cpu.nz16(cpu.y);
    cpu.x = cpu.pull16(wram);
    cpu.nz16(cpu.x);
    cpu.a = cpu.pull16(wram);
    cpu.nz16(cpu.a);
    cpu.rti(wram);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hw::{HwRead, HwWrite};
    use crate::Cpu;

    const ENTRY_S: u16 = 0x01F0;

    /// Interrupted CPU state at m16x16 with nonzero high bytes everywhere.
    fn interrupted_m16() -> Cpu {
        Cpu {
            a: 0x1234,
            x: 0x5678,
            y: 0x9ABC,
            s: ENTRY_S + 4,
            d: 0x0300,
            db: 0x7E,
            pb: 0x02,
            pc: 0xC0DE,
            p: flag::C | flag::N,
        }
    }

    /// Interrupted state at m8x8 (X/Y high bytes are zero on real hardware, B is kept).
    fn interrupted_m8() -> Cpu {
        Cpu {
            a: 0xAB12,
            x: 0x0034,
            y: 0x0056,
            s: ENTRY_S + 4,
            d: 0x1F00,
            db: 0x81,
            pb: 0x0D,
            pc: 0x8123,
            p: flag::M | flag::X | flag::Z,
        }
    }

    /// Native IRQ entry: push PB, PC, P; set I, clear D; jump to $00:82D8.
    fn enter(s: &mut State, c: Cpu) {
        let State { wram, cpu, .. } = s;
        *cpu = c;
        cpu.push8(wram, c.pb);
        cpu.push16(wram, c.pc);
        cpu.push8(wram, c.p);
        cpu.p = (cpu.p | flag::I) & !flag::D;
        cpu.pb = 0x00;
        cpu.pc = 0x82D8;
        assert_eq!(cpu.s, ENTRY_S);
    }

    fn state(c: Cpu, alt: u8, timeup: &[u8], req: u8) -> State {
        let mut s = State::new();
        s.wram.w8(addr::NMI_ALT_THREAD as u32, alt);
        s.wram.w8(addr::IRQ_ENABLE_REQ as u32, req);
        s.wram.w8(addr::IRQ_BG3HOFS as u32, 0x34);
        s.wram.w8(addr::IRQ_BG3HOFS as u32 + 1, 0x12);
        s.hw.inputs.set_reads(0x4211, timeup);
        enter(&mut s, c);
        s
    }

    fn split_writes() -> Vec<HwWrite> {
        [(0x2111, 0x34), (0x2111, 0x12), (0x2112, 0), (0x2112, 0)]
            .iter()
            .map(|&(addr, value)| HwWrite { addr, value })
            .collect()
    }

    #[test]
    fn thread_path_swaps_stacks() {
        for timeup in [0x80, 0x00] {
            let c = interrupted_m16();
            let mut s = state(c, 1, &[timeup], 0x80);
            // Other thread's 13-byte frame at [$1F0A]+1.
            let other_sp: u16 = 0x1DF2;
            s.wram.w16(addr::ALT_THREAD_SP as u32, other_sp);
            #[rustfmt::skip]
            let frame = [
                0x7F, // DB
                0x00, 0x04, // D
                0x22, 0x11, // Y
                0x44, 0x33, // X
                0x66, 0x55, // A
                flag::V, // P (m16x16)
                0x45, 0x23, // PC
                0x01, // PB
            ];
            for (i, b) in frame.iter().enumerate() {
                s.wram.w8(other_sp as u32 + 1 + i as u32, *b);
            }
            irq(&mut s);
            assert_eq!(
                s.hw.reads,
                vec![HwRead {
                    addr: 0x4211,
                    value: timeup
                }]
            );
            assert!(s.hw.writes.is_empty());
            assert_eq!(s.wram.r16(addr::ALT_THREAD_SP as u32), ENTRY_S - 9);
            assert_eq!(
                s.cpu,
                Cpu {
                    a: 0x5566,
                    x: 0x3344,
                    y: 0x1122,
                    s: other_sp + 13,
                    d: 0x0400,
                    db: 0x7F,
                    pb: 0x01,
                    pc: 0x2345,
                    p: flag::V,
                }
            );
            // $0128 ignored on this path.
            assert_eq!(s.wram.r8(addr::IRQ_ENABLE_REQ as u32), 0x80);
            // Suspended thread's frame is left on the old stack (top to bottom).
            let base = (ENTRY_S - 9) as u32;
            assert_eq!(s.wram.r8(base + 1), c.db);
            assert_eq!(s.wram.r16(base + 2), c.d);
            assert_eq!(s.wram.r16(base + 4), c.y);
            assert_eq!(s.wram.r16(base + 6), c.x);
            assert_eq!(s.wram.r16(base + 8), c.a);
            assert_eq!(s.wram.r8(base + 10), c.p);
            assert_eq!(s.wram.r16(base + 11), c.pc);
            assert_eq!(s.wram.r8(base + 13), c.pb);
        }
    }

    #[test]
    fn ack_only_not_timer_irq() {
        let c = interrupted_m16();
        let mut s = state(c, 0, &[0x00], 0x80);
        irq(&mut s);
        assert_eq!(
            s.hw.reads,
            vec![HwRead {
                addr: 0x4211,
                value: 0x00
            }]
        );
        assert!(s.hw.writes.is_empty());
        assert_eq!(s.wram.r8(addr::IRQ_ENABLE_REQ as u32), 0x80);
        assert_eq!(s.cpu, c);
    }

    #[test]
    fn ack_only_req_zero() {
        let c = interrupted_m16();
        let mut s = state(c, 0, &[0x80], 0x00);
        irq(&mut s);
        // $4212 never read (none supplied: reading it would panic).
        assert_eq!(
            s.hw.reads,
            vec![HwRead {
                addr: 0x4211,
                value: 0x80
            }]
        );
        assert!(s.hw.writes.is_empty());
        assert_eq!(s.cpu, c);
    }

    #[test]
    fn split_repeating() {
        let c = interrupted_m16();
        let mut s = state(c, 0, &[0x80], 0x01);
        s.hw.inputs.set_reads(0x4212, &[0x40]);
        irq(&mut s);
        assert_eq!(s.hw.writes, split_writes());
        assert_eq!(s.wram.r8(addr::IRQ_ENABLE_REQ as u32), 0x01);
        assert_eq!(s.cpu, c);
    }

    #[test]
    fn split_one_shot() {
        for req in [0x80, 0xFF] {
            let c = interrupted_m16();
            let mut s = state(c, 0, &[0x80], req);
            s.hw.inputs.set_reads(0x4212, &[0x40]);
            irq(&mut s);
            let mut want = split_writes();
            want.push(HwWrite {
                addr: 0x4200,
                value: 0x81,
            });
            assert_eq!(s.hw.writes, want);
            assert_eq!(s.wram.r8(addr::IRQ_ENABLE_REQ as u32), 0x00);
            assert_eq!(s.cpu, c);
        }
    }

    #[test]
    fn hblank_spin() {
        for seq in [&[0x00, 0x00, 0x40][..], &[0x80, 0x40][..]] {
            let c = interrupted_m16();
            let mut s = state(c, 0, &[0x80], 0x01);
            s.hw.inputs.set_reads(0x4212, seq);
            irq(&mut s);
            let polls: Vec<u8> =
                s.hw.reads
                    .iter()
                    .filter(|r| r.addr == 0x4212)
                    .map(|r| r.value)
                    .collect();
            assert_eq!(polls, seq);
            assert_eq!(s.hw.writes, split_writes());
            assert_eq!(s.cpu, c);
        }
    }

    #[test]
    fn restores_m8x8_and_m16x16_entries() {
        for c in [interrupted_m8(), interrupted_m16()] {
            for (alt_req, timeup) in [(0x00, 0x00), (0x01, 0x80), (0x80, 0x80)] {
                let mut s = state(c, 0, &[timeup], alt_req);
                s.hw.inputs.set_reads(0x4212, &[0x40]);
                irq(&mut s);
                assert_eq!(s.cpu, c);
                // PHK/PLB leaves $00 in the byte below the pushed frame.
                assert_eq!(s.wram.r8((ENTRY_S - 9) as u32), 0x00);
            }
        }
    }

    #[test]
    fn rti_with_x_set_keeps_index_high_bytes_zero() {
        let c = interrupted_m8();
        let mut s = state(c, 0, &[0x00], 0);
        irq(&mut s);
        assert_eq!((s.cpu.x, s.cpu.y, s.cpu.a), (0x0034, 0x0056, 0xAB12));
    }
}
