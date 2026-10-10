//! $00:85EC PrepFrameUploads

use crate::cpu::{bank0_wram, flag};
use crate::wram::{addr, Wram};
use crate::{Cpu, State};

/// Reads one byte of cartridge ROM at `$BB:AAAA` (`AAAA >= $8000`).
///
/// `State` has no ROM yet, so this routine takes ROM access as a parameter
/// (see the followup in the unit's ledger file).
pub type RomRead<'a> = &'a dyn Fn(u8, u16) -> u8;

/// $00:85EC PrepFrameUploads
/// Spec: ledger/units/0085ec.toml
///
/// Part 1 packs the per-sprite bytes $0A20-$0A9F into the OAM high table
/// $0A00-$0A1F. Part 2 rebuilds the 16-bit upload pointers $0AC0-$0AF8 from ROM
/// tables (bank DB) and steps the two animation countdowns at $7E:C00D/$C013.
/// Entered and left at m8x8 (JSR/RTS; the return address is not modelled).
/// D is pushed/pulled twice, so the two stack bytes below S are written.
/// Leaves A = the $0AF8 value, X/Y as the spec says, C/V from the last ADC,
/// N/Z from the final PLD, I and D(ecimal) unchanged, D/DB/S unchanged.
pub fn prep_frame_uploads(s: &mut State, rom: RomRead) {
    let State { wram, cpu, .. } = s;
    // ADC is translated in binary mode only; Reset clears decimal and the spec
    // relies on that.
    assert_eq!(
        cpu.p & flag::D,
        0,
        "PrepFrameUploads entered with decimal mode set"
    );
    let mut b = Bus { wram, rom };

    // ---- Part 1: OAM high table, m8x8 ----
    // $85EC PHD
    cpu.push16(b.wram, cpu.d);
    // $85ED LDA #$0A / XBA / LDA #$00 / TCD
    cpu.a = (cpu.a & 0xFF00) | 0x0A;
    cpu.nz8(0x0A);
    cpu.a = cpu.a.swap_bytes();
    cpu.nz8(cpu.a as u8);
    cpu.a &= 0xFF00;
    cpu.nz8(0x00);
    cpu.d = cpu.a;
    cpu.nz16(cpu.d);
    // $85F3 LDY #$1C
    cpu.y = 0x1C;
    cpu.nz8(0x1C);
    loop {
        // $85F5 TYA / ASL / ASL / TAX
        let mut a = cpu.y as u8;
        a = asl8(cpu, a);
        a = asl8(cpu, a);
        cpu.x = a as u16;
        cpu.nz8(a);
        // Four outputs: $0A00+Y+k from dp $20+X+4k .. $23+X+4k.
        for k in 0..4u16 {
            let base = 0x20 + 4 * k;
            // LDA $23+4k,X ; (ASL ASL ORA)x3
            // QUIRK: no masking; high bits of each source byte bleed into the
            // neighbouring 2-bit fields (only s3 loses bits by shift-out).
            a = b.r8(dp_x8(cpu, base + 3));
            cpu.nz8(a);
            for j in (0..3u16).rev() {
                a = asl8(cpu, a);
                a = asl8(cpu, a);
                a |= b.r8(dp_x8(cpu, base + j));
                cpu.nz8(a);
            }
            // STA $0A00+k,Y (DB-relative)
            b.w8(abs_idx(cpu.db, 0x0A00 + k, cpu.y), a);
        }
        // $863D DEY x4 (8-bit)
        for _ in 0..4 {
            cpu.y = (cpu.y as u8).wrapping_sub(1) as u16;
            cpu.nz8(cpu.y as u8);
        }
        // $8641 BPL L_0085F5
        // QUIRK: stores go Y=$1C first down to Y=0; exits with Y=$FC.
        if cpu.y & 0x80 != 0 {
            break;
        }
    }
    // $8643 PLD
    cpu.d = cpu.pull16(b.wram);
    cpu.nz16(cpu.d);

    // ---- Part 2: upload pointers ----
    // $8644 REP #$31 (clears C too)
    cpu.rep(flag::M | flag::X | flag::C);
    // $8646 PHD / LDA #$0A00 / TCD
    cpu.push16(b.wram, cpu.d);
    lda16(cpu, 0x0A00);
    cpu.d = cpu.a;
    cpu.nz16(cpu.d);

    // $864B LDX $0100 (16-bit)
    ldx16(cpu, b.r16(abs(cpu.db, 0x0100)));
    // $864E LDA $9386,X / STA $CC
    // QUIRK: 16-bit X is unchecked; $9386+X past $FFFF carries into bank DB+1.
    let v = b.r16(abs_idx(cpu.db, 0x9386, cpu.x));
    lda16(cpu, v);
    b.w16(dp(cpu, 0xCC), cpu.a);
    // $8653 ADC #$0200 / STA $CE
    // QUIRK: no CLC; carry in is 0 from REP #$31.
    adc16(cpu, 0x0200);
    b.w16(dp(cpu, 0xCE), cpu.a);
    // $8658 LDA $95E4,X / STA $D0 / CLC / ADC #$0200 / STA $D2
    let v = b.r16(abs_idx(cpu.db, 0x95E4, cpu.x));
    lda16(cpu, v);
    b.w16(dp(cpu, 0xD0), cpu.a);
    cpu.p &= !flag::C;
    adc16(cpu, 0x0200);
    b.w16(dp(cpu, 0xD2), cpu.a);
    // $8663 LDX $0102 / LDA $9842,X / STA $D4
    ldx16(cpu, b.r16(abs(cpu.db, 0x0102)));
    let v = b.r16(abs_idx(cpu.db, 0x9842, cpu.x));
    lda16(cpu, v);
    b.w16(dp(cpu, 0xD4), cpu.a);
    // $866B LDX $0104 / LDA $9842,X / STA $D6
    ldx16(cpu, b.r16(abs(cpu.db, 0x0104)));
    let v = b.r16(abs_idx(cpu.db, 0x9842, cpu.x));
    lda16(cpu, v);
    b.w16(dp(cpu, 0xD6), cpu.a);

    // $8673 SEP #$10 -> m16x8
    cpu.sep(flag::X);
    // $8675 LDX $0107 / LDA $848C,X / STA $C0 / CLC / ADC #$0180 / STA $C2
    ldx8(cpu, b.r8(abs(cpu.db, 0x0107)));
    let v = b.r16(abs_idx(cpu.db, 0x848C, cpu.x));
    lda16(cpu, v);
    b.w16(dp(cpu, 0xC0), cpu.a);
    cpu.p &= !flag::C;
    adc16(cpu, 0x0180);
    b.w16(dp(cpu, 0xC2), cpu.a);
    // $8683 LDX $0108 / LDA $849C,X / STA $C4 / CLC / ADC #$00C0 / STA $C6
    ldx8(cpu, b.r8(abs(cpu.db, 0x0108)));
    let v = b.r16(abs_idx(cpu.db, 0x849C, cpu.x));
    lda16(cpu, v);
    b.w16(dp(cpu, 0xC4), cpu.a);
    cpu.p &= !flag::C;
    adc16(cpu, 0x00C0);
    b.w16(dp(cpu, 0xC6), cpu.a);
    // $8691 LDA $0109 / AND #$00F8 / LSR / LSR / TAY
    // QUIRK: reads $0109 as a word; $010A is masked off here.
    let v = b.r16(abs(cpu.db, 0x0109));
    lda16(cpu, v);
    lda16(cpu, cpu.a & 0x00F8);
    lsr16(cpu);
    lsr16(cpu);
    tay8(cpu);
    // $869A LDA $0109 / ASL / TAX
    // QUIRK: 8-bit X keeps only the low byte of $0109<<1, dropping bit 7 of $0109.
    let v = b.r16(abs(cpu.db, 0x0109));
    lda16(cpu, v);
    asl16(cpu);
    tax8(cpu);
    // $869F LDA $84A2,X / STA $C8
    let v = b.r16(abs_idx(cpu.db, 0x84A2, cpu.x));
    lda16(cpu, v);
    b.w16(dp(cpu, 0xC8), cpu.a);
    // $86A4 CLC / TYX / ADC $85A2,X / STA $CA
    cpu.p &= !flag::C;
    cpu.x = cpu.y & 0x00FF;
    cpu.nz8(cpu.x as u8);
    let v = b.r16(abs_idx(cpu.db, 0x85A2, cpu.x));
    adc16(cpu, v);
    b.w16(dp(cpu, 0xCA), cpu.a);
    // $86AB LDA $02C3 / AND #$0003 / ASL / TAX
    let v = b.r16(abs(cpu.db, 0x02C3));
    lda16(cpu, v);
    lda16(cpu, cpu.a & 0x0003);
    asl16(cpu);
    tax8(cpu);
    // $86B3 LDA $8484,X / STA $D8 / CLC / ADC #$0100 / STA $DA
    let v = b.r16(abs_idx(cpu.db, 0x8484, cpu.x));
    lda16(cpu, v);
    b.w16(dp(cpu, 0xD8), cpu.a);
    cpu.p &= !flag::C;
    adc16(cpu, 0x0100);
    b.w16(dp(cpu, 0xDA), cpu.a);

    // $86BE LDA $7EC00D / DEC / STA $7EC00D
    // QUIRK: DEC before the test; a start value of 0 becomes $FFFF and does not reload.
    let timer_a = Wram::long(0x7E, addr::ANIM_TILE_TIMER_A);
    lda16(cpu, b.wram.r16(timer_a));
    lda16(cpu, cpu.a.wrapping_sub(1));
    b.wram.w16(timer_a, cpu.a);
    // $86C7 BNE L_0086F7
    if cpu.a == 0 {
        // $86C9 LDA #$0009
        lda16(cpu, 0x0009);
        // $86CC LDX $8C
        // QUIRK: D=$0A00 here, so this reads $7E:0A8C, not $7E:008C.
        ldx8(cpu, b.r8(dp(cpu, 0x8C)));
        // $86CE CPX #$B5 / BEQ / CPX #$BC / BNE
        let hit = cpx8(cpu, 0xB5) || cpx8(cpu, 0xBC);
        if hit {
            // $86D6 LDA #$0017
            lda16(cpu, 0x0017);
        }
        // $86D9 STA $7EC00D
        b.wram.w16(timer_a, cpu.a);
        // $86DD LDA $7EC00F / CLC / ADC #$0400 / CMP #$0C00 / BNE / LDA #$0000
        // QUIRK: wraps to 0 only on exactly $0C00; off-grid values keep adding.
        let ofs_a = Wram::long(0x7E, addr::ANIM_TILE_OFS_A);
        lda16(cpu, b.wram.r16(ofs_a));
        cpu.p &= !flag::C;
        adc16(cpu, 0x0400);
        if cmp16(cpu, 0x0C00) {
            lda16(cpu, 0x0000);
        }
        // $86ED STA $7EC00F / CLC / ADC #$A680 / STA $DC
        b.wram.w16(ofs_a, cpu.a);
        cpu.p &= !flag::C;
        adc16(cpu, 0xA680);
        b.w16(dp(cpu, 0xDC), cpu.a);
    }

    // $86F7 LDA $7EC013 / DEC / STA $7EC013 / BNE L_00872A
    let timer_b = Wram::long(0x7E, addr::ANIM_TILE_TIMER_B);
    lda16(cpu, b.wram.r16(timer_b));
    lda16(cpu, cpu.a.wrapping_sub(1));
    b.wram.w16(timer_b, cpu.a);
    if cpu.a == 0 {
        // $8702 LDA $7EC015 / TAX / INX / INX
        // QUIRK: 8-bit X drops the high byte of $C015, and INX wraps at 8 bits.
        let idx_b = Wram::long(0x7E, addr::ANIM_TILE_IDX_B);
        lda16(cpu, b.wram.r16(idx_b));
        tax8(cpu);
        inx8(cpu);
        inx8(cpu);
        // $8709 CPX #$0C / BNE / LDX #$00
        // QUIRK: resets only on exactly $0C; odd or >$0A starts never hit it.
        if cpx8(cpu, 0x0C) {
            ldx8(cpu, 0x00);
        }
        // $870F TXA (m16x8: zero-extends) / STA $7EC015
        lda16(cpu, cpu.x & 0x00FF);
        b.wram.w16(idx_b, cpu.a);
        // $8714 LDA $85C2,X / STA $7EC013
        let v = b.r16(abs_idx(cpu.db, 0x85C2, cpu.x));
        lda16(cpu, v);
        b.wram.w16(timer_b, cpu.a);
        // $871B LDA #$B280 / CLC / ADC $85CE,X / STA $E0
        lda16(cpu, 0xB280);
        cpu.p &= !flag::C;
        let v = b.r16(abs_idx(cpu.db, 0x85CE, cpu.x));
        adc16(cpu, v);
        b.w16(dp(cpu, 0xE0), cpu.a);
        // $8724 CLC / ADC #$0060 / STA $E2
        cpu.p &= !flag::C;
        adc16(cpu, 0x0060);
        b.w16(dp(cpu, 0xE2), cpu.a);
    }

    // $872A..$8752: three (LDA abs / ASL / ADC #base / STA / ADC #$0200 / STA)
    // QUIRK: no CLC; carry in is bit 15 of the source from ASL, and the +$0200
    // chains the carry out of the base add.
    for (src, base, lo, hi) in [
        (0x0AE8u16, 0xB940u16, 0xECu16, 0xEEu16),
        (0x0AEA, 0xB940, 0xF0, 0xF2),
        (0x0AF4, 0xB540, 0xF6, 0xF8),
    ] {
        let v = b.r16(abs(cpu.db, src));
        lda16(cpu, v);
        asl16(cpu);
        adc16(cpu, base);
        b.w16(dp(cpu, lo), cpu.a);
        adc16(cpu, 0x0200);
        b.w16(dp(cpu, hi), cpu.a);
    }

    // $8754 PLD
    // QUIRK: N/Z at return come from the restored D, not the last ADC.
    cpu.d = cpu.pull16(b.wram);
    cpu.nz16(cpu.d);
    // $8755 SEP #$20
    cpu.sep(flag::M);
    // $8757 RTS
}

/// CPU data bus as this routine sees it: WRAM (incl. the low mirror) and ROM.
struct Bus<'a> {
    wram: &'a mut Wram,
    rom: RomRead<'a>,
}

impl Bus<'_> {
    /// Reads a 24-bit address. Anything outside WRAM/ROM is not reachable with
    /// the spec's inputs and panics.
    fn r8(&self, full: u32) -> u8 {
        let bank = (full >> 16) as u8;
        let a = full as u16;
        match bank {
            0x7E | 0x7F => self.wram.r8(Wram::long(bank, a)),
            _ if a >= 0x8000 => (self.rom)(bank, a),
            _ if bank & 0x40 == 0 && a < 0x2000 => self.wram.r8(a as u32),
            _ => panic!("${bank:02X}:{a:04X} read is not modelled"),
        }
    }

    /// 16-bit data read; the high byte comes from the next 24-bit address.
    fn r16(&self, full: u32) -> u16 {
        u16::from_le_bytes([self.r8(full), self.r8(full.wrapping_add(1) & 0xFF_FFFF)])
    }

    fn w8(&mut self, full: u32, v: u8) {
        let bank = (full >> 16) as u8;
        let a = full as u16;
        match bank {
            0x7E | 0x7F => self.wram.w8(Wram::long(bank, a), v),
            _ if bank & 0x40 == 0 && a < 0x2000 => self.wram.w8(a as u32, v),
            _ => panic!("${bank:02X}:{a:04X} write is not modelled"),
        }
    }

    fn w16(&mut self, full: u32, v: u16) {
        let [lo, hi] = v.to_le_bytes();
        self.w8(full, lo);
        self.w8(full.wrapping_add(1) & 0xFF_FFFF, hi);
    }
}

/// Direct page `dp` (bank $00, native mode: D + dp wraps at 16 bits).
fn dp(cpu: &Cpu, off: u16) -> u32 {
    bank0_wram(cpu.d.wrapping_add(off))
}

/// Direct page indexed `dp,X` with 8-bit X.
fn dp_x8(cpu: &Cpu, off: u16) -> u32 {
    bank0_wram(cpu.d.wrapping_add(off).wrapping_add(cpu.x & 0x00FF))
}

/// Absolute `abs` (bank DB).
fn abs(db: u8, a: u16) -> u32 {
    (db as u32) << 16 | a as u32
}

/// Absolute indexed `abs,X`/`abs,Y`: the index carries into the bank.
fn abs_idx(db: u8, a: u16, i: u16) -> u32 {
    (abs(db, a) + i as u32) & 0xFF_FFFF
}

fn asl8(cpu: &mut Cpu, v: u8) -> u8 {
    cpu.p = (cpu.p & !flag::C) | (v >> 7);
    let r = v << 1;
    cpu.nz8(r);
    r
}

fn asl16(cpu: &mut Cpu) {
    cpu.p = (cpu.p & !flag::C) | (cpu.a >> 15) as u8;
    cpu.a <<= 1;
    cpu.nz16(cpu.a);
}

fn lsr16(cpu: &mut Cpu) {
    cpu.p = (cpu.p & !flag::C) | (cpu.a & 1) as u8;
    cpu.a >>= 1;
    cpu.nz16(cpu.a);
}

fn lda16(cpu: &mut Cpu, v: u16) {
    cpu.a = v;
    cpu.nz16(v);
}

fn ldx16(cpu: &mut Cpu, v: u16) {
    cpu.x = v;
    cpu.nz16(v);
}

fn ldx8(cpu: &mut Cpu, v: u8) {
    cpu.x = v as u16;
    cpu.nz8(v);
}

fn tax8(cpu: &mut Cpu) {
    cpu.x = cpu.a & 0x00FF;
    cpu.nz8(cpu.x as u8);
}

fn tay8(cpu: &mut Cpu) {
    cpu.y = cpu.a & 0x00FF;
    cpu.nz8(cpu.y as u8);
}

fn inx8(cpu: &mut Cpu) {
    cpu.x = (cpu.x as u8).wrapping_add(1) as u16;
    cpu.nz8(cpu.x as u8);
}

/// Binary-mode 16-bit ADC: A = A + v + C, sets N V Z C.
fn adc16(cpu: &mut Cpu, v: u16) {
    let c = (cpu.p & flag::C) as u32;
    let sum = cpu.a as u32 + v as u32 + c;
    let r = sum as u16;
    let ov = (!(cpu.a ^ v) & (cpu.a ^ r)) & 0x8000 != 0;
    cpu.p &= !(flag::C | flag::V);
    if sum > 0xFFFF {
        cpu.p |= flag::C;
    }
    if ov {
        cpu.p |= flag::V;
    }
    lda16(cpu, r);
}

/// CMP #imm at 16 bits; returns true when equal (Z).
fn cmp16(cpu: &mut Cpu, v: u16) -> bool {
    let r = cpu.a.wrapping_sub(v);
    cpu.p = (cpu.p & !flag::C) | if cpu.a >= v { flag::C } else { 0 };
    cpu.nz16(r);
    r == 0
}

/// CPX #imm at 8 bits; returns true when equal (Z).
fn cpx8(cpu: &mut Cpu, v: u8) -> bool {
    let x = cpu.x as u8;
    cpu.p = (cpu.p & !flag::C) | if x >= v { flag::C } else { 0 };
    cpu.nz8(x.wrapping_sub(v));
    x == v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic ROM: a fixed hash of the address, so tests never need ROM bytes.
    fn fake_rom(bank: u8, a: u16) -> u8 {
        assert!(a >= 0x8000, "fake ROM read below $8000");
        let h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (bank as u32) << 24;
        (h >> 13) as u8
    }

    fn rom16(a: u16) -> u16 {
        u16::from_le_bytes([fake_rom(0, a), fake_rom(0, a.wrapping_add(1))])
    }

    fn rng(seed: &mut u32) -> u32 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 17;
        *seed ^= *seed << 5;
        *seed
    }

    fn entry_cpu() -> Cpu {
        Cpu {
            p: flag::M | flag::X,
            s: 0x01FF,
            ..Default::default()
        }
    }

    fn run(s: &mut State) {
        prep_frame_uploads(s, &fake_rom);
    }

    /// Independent model of the spec's output formulas. Returns the expected
    /// WRAM and final A/X/Y/P.
    fn model(w0: &Wram, cpu0: &Cpu) -> (Wram, u16, u16, u16, u8) {
        let mut w = Wram::new();
        w.load(w0.as_bytes());
        let r8 = |a: u16| w0.r8(a as u32);
        let r16 = |a: u16| w0.r16(a as u32);
        for n in 0..32u16 {
            let sk = |k: u16| r8(0x0A20 + 4 * n + k);
            let v = (sk(3) << 6) | (sk(2) << 4) | (sk(1) << 2) | sk(0);
            w.w8(0x0A00 + n as u32, v);
        }
        let x0 = r16(0x0100);
        let t = |base: u16, i: u16| rom16(base.wrapping_add(i));
        w.w16(0x0AC0, t(0x848C, r8(0x0107) as u16));
        w.w16(0x0AC2, t(0x848C, r8(0x0107) as u16).wrapping_add(0x0180));
        w.w16(0x0AC4, t(0x849C, r8(0x0108) as u16));
        w.w16(0x0AC6, t(0x849C, r8(0x0108) as u16).wrapping_add(0x00C0));
        let i109 = ((r8(0x0109) as u16) << 1) & 0xFF;
        let y = ((r8(0x0109) & 0xF8) >> 2) as u16;
        w.w16(0x0AC8, t(0x84A2, i109));
        w.w16(0x0ACA, t(0x84A2, i109).wrapping_add(t(0x85A2, y)));
        w.w16(0x0ACC, t(0x9386, x0));
        w.w16(0x0ACE, t(0x9386, x0).wrapping_add(0x0200));
        w.w16(0x0AD0, t(0x95E4, x0));
        w.w16(0x0AD2, t(0x95E4, x0).wrapping_add(0x0200));
        w.w16(0x0AD4, t(0x9842, r16(0x0102)));
        w.w16(0x0AD6, t(0x9842, r16(0x0104)));
        let sel = (r8(0x02C3) as u16 & 3) * 2;
        let mut x = sel;
        w.w16(0x0AD8, t(0x8484, sel));
        w.w16(0x0ADA, t(0x8484, sel).wrapping_add(0x0100));
        let ta = r16(0xC00D).wrapping_sub(1);
        w.w16(0xC00D, ta);
        if ta == 0 {
            let b = r8(0x0A8C);
            x = b as u16;
            w.w16(0xC00D, if b == 0xB5 || b == 0xBC { 0x17 } else { 0x09 });
            let mut o = r16(0xC00F).wrapping_add(0x0400);
            if o == 0x0C00 {
                o = 0;
            }
            w.w16(0xC00F, o);
            w.w16(0x0ADC, o.wrapping_add(0xA680));
        }
        let tb = r16(0xC013).wrapping_sub(1);
        w.w16(0xC013, tb);
        if tb == 0 {
            let mut idx = (r8(0xC015).wrapping_add(2)) as u16;
            if idx == 0x0C {
                idx = 0;
            }
            x = idx;
            w.w16(0xC015, idx);
            w.w16(0xC013, t(0x85C2, idx));
            let e0 = 0xB280u16.wrapping_add(t(0x85CE, idx));
            w.w16(0x0AE0, e0);
            w.w16(0x0AE2, e0.wrapping_add(0x0060));
        }
        let mut a = 0;
        let mut cv = 0;
        for (src, base, lo, hi) in [
            (0x0AE8u16, 0xB940u32, 0x0AECu32, 0x0AEEu32),
            (0x0AEA, 0xB940, 0x0AF0, 0x0AF2),
            (0x0AF4, 0xB540, 0x0AF6, 0x0AF8),
        ] {
            let v = r16(src) as u32;
            let s1 = ((v << 1) & 0xFFFF) + base + (v >> 15);
            w.w16(lo, s1 as u16);
            let s2 = (s1 & 0xFFFF) + 0x0200 + (s1 >> 16);
            w.w16(hi, s2 as u16);
            a = s2 as u16;
            let a1 = s1 as u16;
            let ov = (!(a1 ^ 0x0200) & (a1 ^ a)) & 0x8000 != 0;
            cv = if s2 > 0xFFFF { flag::C } else { 0 } | if ov { flag::V } else { 0 };
        }
        // Stack bytes written by PHD.
        w.w16(cpu0.s.wrapping_sub(1) as u32, cpu0.d);
        let nz = ((cpu0.d >> 8) as u8 & flag::N) | if cpu0.d == 0 { flag::Z } else { 0 };
        let p = (cpu0.p & flag::I) | flag::M | flag::X | cv | nz;
        (w, a, x, y, p)
    }

    fn check(w0: &Wram, cpu0: Cpu) -> State {
        let mut s = State::new();
        s.wram.load(w0.as_bytes());
        s.cpu = cpu0;
        run(&mut s);
        let (w, a, x, y, p) = model(w0, &cpu0);
        for i in 0..0x2_0000u32 {
            assert_eq!(s.wram.r8(i), w.r8(i), "WRAM ${i:05X}");
        }
        assert_eq!(s.cpu.a, a, "A");
        assert_eq!(s.cpu.x, x, "X");
        assert_eq!(s.cpu.y, y, "Y");
        assert_eq!(s.cpu.p, p, "P");
        assert_eq!((s.cpu.s, s.cpu.d, s.cpu.db), (cpu0.s, cpu0.d, cpu0.db));
        s
    }

    fn random_wram(seed: &mut u32) -> Wram {
        let mut w = Wram::new();
        for a in 0x0A20..0x0AA0u32 {
            w.w8(a, rng(seed) as u8);
        }
        for a in 0x0100..0x010Bu32 {
            w.w8(a, rng(seed) as u8);
        }
        // Keep table indices inside bank 00 ROM.
        w.w16(0x0100, (rng(seed) as u16) & 0x0FFE);
        w.w16(0x0102, (rng(seed) as u16) & 0x0FFE);
        w.w16(0x0104, (rng(seed) as u16) & 0x0FFE);
        w.w8(0x02C3, rng(seed) as u8);
        w.w8(0x02C4, rng(seed) as u8);
        for a in [0x0AE8u32, 0x0AEA, 0x0AF4, 0xC00F, 0xC015] {
            w.w16(a, rng(seed) as u16);
        }
        w.w16(0xC00D, (rng(seed) % 3) as u16);
        w.w16(0xC013, (rng(seed) % 3) as u16);
        w
    }

    #[test]
    fn random_states_match_spec_model() {
        let mut seed = 0x0085_EC01;
        for _ in 0..200 {
            let w = random_wram(&mut seed);
            let r = rng(&mut seed);
            let cpu = Cpu {
                a: r as u16,
                x: (r >> 16) as u16 & 0xFF,
                y: (r >> 24) as u16,
                d: if r & 0x100 != 0 { 0 } else { (r >> 8) as u16 },
                p: flag::M | flag::X | (r as u8 & (flag::C | flag::V | flag::I | flag::N)),
                ..entry_cpu()
            };
            check(&w, cpu);
        }
    }

    #[test]
    fn oam_high_bit_bleed() {
        let mut w = Wram::new();
        // Sprite group 0: s0=$FF bleeds into every field; s3=$03 shifts into bits 7-6.
        w.w8(0x0A20, 0xFF);
        // Group 31: only s1=$05 -> ($05<<2)=$14.
        w.w8(0x0A20 + 4 * 31 + 1, 0x05);
        // Group 7: s3=$FF loses all but bits 1-0 -> $C0.
        w.w8(0x0A20 + 4 * 7 + 3, 0xFF);
        w.w16(0xC00D, 5);
        w.w16(0xC013, 5);
        let s = check(&w, entry_cpu());
        assert_eq!(s.wram.r8(0x0A00), 0xFF);
        assert_eq!(s.wram.r8(0x0A1F), 0x14);
        assert_eq!(s.wram.r8(0x0A07), 0xC0);
        assert_eq!(s.wram.r8(0x0A01), 0x00);
    }

    fn timers(c00d: u16, c00f: u16, a8c: u8, c013: u16, c015: u16) -> State {
        let mut w = Wram::new();
        w.w16(0xC00D, c00d);
        w.w16(0xC00F, c00f);
        w.w8(0x0A8C, a8c);
        // Wrong-page decoy: $008C must not be used.
        w.w8(0x008C, 0xB5);
        w.w16(0xC013, c013);
        w.w16(0xC015, c015);
        w.w16(0x0ADC, 0x1111);
        w.w16(0x0AE0, 0x2222);
        w.w16(0x0AE2, 0x3333);
        check(&w, entry_cpu())
    }

    fn r16(s: &State, a: u32) -> u16 {
        s.wram.r16(a)
    }

    #[test]
    fn timer_a_no_reload() {
        let s = timers(2, 0x0400, 0, 5, 0);
        assert_eq!(r16(&s, 0xC00D), 1);
        assert_eq!(r16(&s, 0xC00F), 0x0400);
        assert_eq!(r16(&s, 0x0ADC), 0x1111);
    }

    #[test]
    fn timer_a_reload_values() {
        assert_eq!(r16(&timers(1, 0, 0xB5, 5, 0), 0xC00D), 0x17);
        assert_eq!(r16(&timers(1, 0, 0xBC, 5, 0), 0xC00D), 0x17);
        let s = timers(1, 0, 0x00, 5, 0);
        assert_eq!(r16(&s, 0xC00D), 0x09);
        assert_eq!(s.cpu.x, 0x00);
    }

    #[test]
    fn timer_a_offset_steps() {
        let s = timers(1, 0x0800, 0, 5, 0);
        assert_eq!(r16(&s, 0xC00F), 0);
        assert_eq!(r16(&s, 0x0ADC), 0xA680);
        let s = timers(1, 0x0000, 0, 5, 0);
        assert_eq!(r16(&s, 0xC00F), 0x0400);
        assert_eq!(r16(&s, 0x0ADC), 0xAA80);
        assert_eq!(r16(&timers(1, 0x0200, 0, 5, 0), 0xC00F), 0x0600);
        let s = timers(1, 0xFE00, 0, 5, 0);
        assert_eq!(r16(&s, 0xC00F), 0x0200);
        assert_eq!(r16(&s, 0x0ADC), 0xA880);
    }

    #[test]
    fn timer_a_zero_underflows() {
        let s = timers(0, 0x0400, 0xB5, 5, 0);
        assert_eq!(r16(&s, 0xC00D), 0xFFFF);
        assert_eq!(r16(&s, 0xC00F), 0x0400);
        assert_eq!(r16(&s, 0x0ADC), 0x1111);
    }

    #[test]
    fn timer_b_paths() {
        let s = timers(5, 0, 0, 2, 0x0004);
        assert_eq!((r16(&s, 0xC013), r16(&s, 0xC015)), (1, 4));
        assert_eq!(r16(&s, 0x0AE0), 0x2222);
        assert_eq!(r16(&s, 0x0AE2), 0x3333);

        let s = timers(5, 0, 0, 1, 0x000A);
        assert_eq!(r16(&s, 0xC015), 0);
        assert_eq!(r16(&s, 0xC013), rom16(0x85C2));
        assert_eq!(r16(&s, 0x0AE0), 0xB280u16.wrapping_add(rom16(0x85CE)));
        assert_eq!(s.cpu.x, 0);

        let s = timers(5, 0, 0, 1, 0x0004);
        assert_eq!(r16(&s, 0xC015), 6);
        assert_eq!(r16(&s, 0x0AE2), 0xB2E0u16.wrapping_add(rom16(0x85CE + 6)));

        let s = timers(5, 0, 0, 1, 0xFF0A);
        assert_eq!(r16(&s, 0xC015), 0x0000);

        let s = timers(5, 0, 0, 1, 0x0001);
        assert_eq!(r16(&s, 0xC015), 3);

        // Even start above $0A wraps $FE -> $00 at 8 bits.
        let s = timers(5, 0, 0, 1, 0x00FE);
        assert_eq!(r16(&s, 0xC015), 0x0000);
    }

    #[test]
    fn carry_chain_into_second_pointer() {
        let mut w = Wram::new();
        w.w16(0xC00D, 5);
        w.w16(0xC013, 5);
        // $2400<<1 = $4800, + $B940 = $10140 -> $0140 with C=1 -> $0AEE = $0341.
        w.w16(0x0AE8, 0x2400);
        // Bit 15 set: ($8001<<1)=$0002 + $B940 + 1 = $B943.
        w.w16(0x0AEA, 0x8001);
        // $0AF4 = $FFFF: $FFFE + $B540 + 1 = $1B53F -> $B53F C=1, + $0200 + 1 = $B740.
        w.w16(0x0AF4, 0xFFFF);
        let s = check(&w, entry_cpu());
        assert_eq!(r16(&s, 0x0AEC), 0x0140);
        assert_eq!(r16(&s, 0x0AEE), 0x0341);
        assert_eq!(r16(&s, 0x0AF0), 0xB943);
        assert_eq!(r16(&s, 0x0AF2), 0xBB43);
        assert_eq!(r16(&s, 0x0AF6), 0xB53F);
        assert_eq!(r16(&s, 0x0AF8), 0xB740);
        assert_eq!(s.cpu.a, 0xB740);
        assert_eq!(s.cpu.p & (flag::C | flag::V), 0);
    }

    #[test]
    fn exit_flags_from_pld() {
        let mut w = Wram::new();
        w.w16(0xC00D, 5);
        w.w16(0xC013, 5);
        let s = check(&w, entry_cpu());
        assert_eq!(s.cpu.p & (flag::N | flag::Z), flag::Z);
        let s = check(
            &w,
            Cpu {
                d: 0x8000,
                ..entry_cpu()
            },
        );
        assert_eq!(s.cpu.p & (flag::N | flag::Z), flag::N);
    }

    #[test]
    fn index_109_truncation_and_010a_ignored() {
        let mut w = Wram::new();
        w.w16(0xC00D, 5);
        w.w16(0xC013, 5);
        w.w8(0x0109, 0x83);
        w.w8(0x010A, 0xFF);
        w.w8(0x02C3, 0xFE);
        let s = check(&w, entry_cpu());
        assert_eq!(r16(&s, 0x0AC8), rom16(0x84A2 + 0x06));
        assert_eq!(s.cpu.y, 0x20);
        assert_eq!(r16(&s, 0x0AD8), rom16(0x8484 + 4));
        assert_eq!(s.cpu.x, 4);
    }

    #[test]
    fn index_carries_into_next_bank() {
        // $9386 + $6C7A = $1:0000 -> bank $01 low-WRAM mirror at $0000.
        let mut w = Wram::new();
        w.w16(0xC00D, 5);
        w.w16(0xC013, 5);
        w.w16(0x0100, 0x6C7A);
        w.w16(0x0000, 0xBEEF);
        let mut s = State::new();
        s.wram.load(w.as_bytes());
        s.cpu = entry_cpu();
        run(&mut s);
        assert_eq!(s.wram.r16(0x0ACC), 0xBEEF);
        assert_eq!(s.wram.r16(0x0ACE), 0xC0EF);
    }

    #[test]
    #[should_panic]
    fn decimal_mode_rejected() {
        let mut s = State::new();
        s.cpu = Cpu {
            p: flag::M | flag::X | flag::D,
            ..entry_cpu()
        };
        run(&mut s);
    }
}
