//! $00:841E ClearOamBuffer

use crate::cpu::{bank0_wram, flag};
use crate::wram::addr;
use crate::{Cpu, State};

/// Direct-page offsets stored to by each half-pass ($8429-$8447 and $8451-$846F).
const DP_OFFSETS: [u8; 16] = [
    0x01, 0x05, 0x09, 0x0D, 0x11, 0x15, 0x19, 0x1D, 0x81, 0x85, 0x89, 0x8D, 0x91, 0x95, 0x99, 0x9D,
];

/// $00:841E ClearOamBuffer
/// Spec: ledger/units/00841e.toml
///
/// Sets the Y byte of all 128 OAM low-table buffer entries ($0801 + 4n) to $F0.
/// Entered and left at m8x8 (JSR/RTS; the return address is not modelled).
/// Leaves A = $09E0, X = $E0, D restored, N/Z from the restored D, C = 0, V = 0.
pub fn clear_oam_buffer(s: &mut State) {
    let State { wram, cpu, .. } = s;

    // $841E PHD
    cpu.push16(wram, cpu.d);
    // $841F LDX #$60
    cpu.x = 0x0060;
    cpu.nz8(0x60);
    loop {
        for page in [0x08u8, 0x09] {
            // $8421 LDA #$08 / $8449 LDA #$09
            lda8(cpu, page);
            // XBA
            cpu.a = cpu.a.swap_bytes();
            cpu.nz8(cpu.a as u8);
            // LDA #$00
            lda8(cpu, 0x00);
            // TCD: full 16-bit A -> D.
            cpu.d = cpu.a;
            cpu.nz16(cpu.d);
            debug_assert_eq!(cpu.d, addr::OAM_BUFFER_LOW + ((page as u16 - 0x08) << 8));
            // LDA #$F0
            lda8(cpu, 0xF0);
            // STA dp,X x16
            // QUIRK: DP writes always land in bank $00 (WRAM mirror), regardless of DB.
            for off in DP_OFFSETS {
                // dp,X with D low byte = 0 and offset+X <= $FD: no page wrap.
                let ea = cpu.d.wrapping_add(off as u16).wrapping_add(cpu.x & 0x00FF);
                wram.w8(bank0_wram(ea), 0xF0);
            }
        }
        // $8471 TXA (8-bit: A high byte kept)
        // QUIRK: B keeps $09 from the last XBA/TCD setup, so A exits as $09E0.
        cpu.a = (cpu.a & 0xFF00) | (cpu.x & 0x00FF);
        cpu.nz8(cpu.a as u8);
        // $8472 SEC
        cpu.p |= flag::C;
        // $8473 SBC #$20 (binary; decimal flag assumed clear)
        sbc8_imm(cpu, 0x20);
        // $8475 TAX
        cpu.x = cpu.a & 0x00FF;
        cpu.nz8(cpu.x as u8);
        // $8476 BPL L_008421
        if cpu.p & flag::N == 0 {
            continue;
        }
        break;
    }
    // $8478 PLD
    // QUIRK: exit N/Z come from the restored 16-bit D, not from the SBC/TAX.
    cpu.d = cpu.pull16(wram);
    cpu.nz16(cpu.d);
    // $8479 RTS
}

/// LDA #imm at m8: replaces the low byte of A, sets N/Z.
fn lda8(cpu: &mut Cpu, v: u8) {
    cpu.a = (cpu.a & 0xFF00) | v as u16;
    cpu.nz8(v);
}

/// SBC #imm at m8, binary mode: sets N, V, Z, C.
fn sbc8_imm(cpu: &mut Cpu, v: u8) {
    let a = cpu.a as u8;
    let borrow = if cpu.p & flag::C != 0 { 0 } else { 1 };
    let r = (a as i16) - (v as i16) - borrow;
    let res = r as u8;
    let overflow = ((a ^ v) & (a ^ res) & 0x80) != 0;
    cpu.p &= !(flag::C | flag::V);
    if r >= 0 {
        cpu.p |= flag::C;
    }
    if overflow {
        cpu.p |= flag::V;
    }
    cpu.a = (cpu.a & 0xFF00) | res as u16;
    cpu.nz8(res);
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY_S: u16 = 0x01FF;

    fn setup(d: u16) -> State {
        let mut s = State::new();
        // Index-byte pattern over $0700-$0BFF, avoiding $F0 and $00 in the Y bytes.
        for a in 0x0700u32..0x0C00 {
            let mut v = (a & 0xFF) as u8;
            if v == 0xF0 || v == 0x00 {
                v = 0x5A;
            }
            s.wram.w8(a, v);
        }
        s.cpu = Cpu {
            a: 0x01C3,
            x: 0x0077,
            y: 0x0042,
            s: ENTRY_S,
            d,
            db: 0x81,
            pb: 0x00,
            pc: 0x841E,
            p: flag::M | flag::X | flag::I | flag::V,
        };
        s
    }

    fn is_y_byte(a: u32) -> bool {
        (0x0800..0x0A00).contains(&a) && a & 3 == 1
    }

    fn check_memory(before: &State, after: &State) {
        for a in 0x0700u32..0x0C00 {
            if is_y_byte(a) {
                assert_eq!(after.wram.r8(a), 0xF0, "Y byte ${a:04X}");
            } else {
                assert_eq!(after.wram.r8(a), before.wram.r8(a), "byte ${a:04X}");
            }
        }
    }

    #[test]
    fn only_y_bytes_change_d_zero() {
        let mut s = setup(0x0000);
        let before = setup(0x0000);
        clear_oam_buffer(&mut s);
        check_memory(&before, &s);
        assert_eq!(s.cpu.d, 0x0000);
        assert_eq!(s.cpu.p & (flag::N | flag::Z), flag::Z);
    }

    #[test]
    fn nonzero_d_restored() {
        for (d, nz) in [(0x1234u16, 0u8), (0x8000, flag::N)] {
            let mut s = setup(d);
            let before = setup(d);
            clear_oam_buffer(&mut s);
            check_memory(&before, &s);
            assert_eq!(s.cpu.d, d);
            assert_eq!(s.cpu.p & (flag::N | flag::Z), nz);
        }
    }

    #[test]
    fn exit_registers() {
        let mut s = setup(0x0000);
        clear_oam_buffer(&mut s);
        let c = s.cpu;
        assert_eq!(c.a, 0x09E0);
        assert_eq!(c.x, 0x00E0);
        assert_eq!(c.y, 0x0042);
        assert_eq!(c.s, ENTRY_S);
        assert_eq!(c.db, 0x81);
        assert_eq!(c.p & (flag::C | flag::V), 0);
        assert_eq!(
            c.p & (flag::M | flag::X | flag::I),
            flag::M | flag::X | flag::I
        );
        // PHD left D's bytes on the stack.
        assert_eq!(s.wram.r16(ENTRY_S as u32 - 1), 0x0000);
    }
}
