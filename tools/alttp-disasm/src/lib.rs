//! M/X-aware 65816 disassembler for ALttP units.
//!
//! Two modes:
//! - `trace`: follow control flow from one entry, the way a unit is defined in the
//!   harness methodology, and produce a listing plus a ledger-ready `[map]` summary.
//! - `linear`: decode a straight address range.
//!
//! Output is for local use only. Never commit it.

pub mod opcodes;

use opcodes::{Mode, OPS};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;

/// Anything that can serve ROM bytes by SNES address.
pub trait Mem {
    fn read(&self, bank: u8, addr: u16) -> Option<u8>;
}

impl Mem for alttp_rom::Rom {
    fn read(&self, bank: u8, addr: u16) -> Option<u8> {
        self.read_u8(bank, addr).ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum W {
    W8,
    W16,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mx {
    pub m: W,
    pub x: W,
}

impl Mx {
    pub fn new(m16: bool, x16: bool) -> Self {
        Mx { m: if m16 { W::W16 } else { W::W8 }, x: if x16 { W::W16 } else { W::W8 } }
    }

    pub fn label(&self) -> String {
        let f = |w: W| match w {
            W::W8 => "8",
            W::W16 => "16",
            W::Unknown => "?",
        };
        format!("m{}x{}", f(self.m), f(self.x))
    }

    /// Apply REP (clear) or SEP (set) with the given immediate.
    fn apply(self, imm: u8, set: bool) -> Self {
        let mut s = self;
        if imm & 0x20 != 0 {
            s.m = if set { W::W8 } else { W::W16 };
        }
        if imm & 0x10 != 0 {
            s.x = if set { W::W8 } else { W::W16 };
        }
        s
    }
}

/// 24-bit address helpers.
pub fn full(bank: u8, addr: u16) -> u32 {
    (bank as u32) << 16 | addr as u32
}
pub fn fmt_addr(a: u32) -> String {
    format!("${:02X}:{:04X}", a >> 16, a & 0xFFFF)
}
pub fn unit_id(a: u32) -> String {
    format!("{a:06x}")
}

/// Parse `$00:8000`, `00:8000`, `008000`, or `0x008000`.
pub fn parse_addr(s: &str) -> Option<u32> {
    let t = s.trim().trim_start_matches('$').trim_start_matches("0x").replace(':', "");
    if t.len() != 6 {
        return None;
    }
    u32::from_str_radix(&t, 16).ok()
}

#[derive(Debug, Clone)]
pub struct Insn {
    pub at: u32,
    pub op: u8,
    pub bytes: Vec<u8>,
    pub mx: Mx,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpTable {
    pub at: u32,
    pub entries: usize,
    pub targets: Vec<u32>,
    pub kind: &'static str,
}

#[derive(Debug, Default, Clone)]
pub struct Options {
    /// Addresses that are other units' entries. Reaching one by jump or fallthrough ends the path (tail exit).
    pub stop_at: HashSet<u32>,
    /// Routines that never return to the caller (e.g. dispatchers with an inline table after the call).
    pub noreturn: HashSet<u32>,
    /// Known jump tables: table address -> (entry count, long entries?).
    pub tables: BTreeMap<u32, (usize, bool)>,
    /// Jump tables split into parallel byte tables: low, high, optional bank, entry count.
    /// Without a bank table, targets are in the low table's bank.
    pub split_tables: Vec<SplitTable>,
    /// Max instructions before giving up (guards against decoding data as code).
    pub max_insns: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitTable {
    pub lo: u32,
    pub hi: u32,
    pub bank: Option<u32>,
    pub count: usize,
}

fn read_split_table(mem: &dyn Mem, t: &SplitTable) -> Result<Vec<u32>, String> {
    let byte = |base: u32, i: usize| {
        let a = (base as u16).wrapping_add(i as u16);
        let b = (base >> 16) as u8;
        mem.read(b, a).ok_or_else(|| format!("split table byte at {} unreadable", fmt_addr(full(b, a))))
    };
    (0..t.count)
        .map(|i| {
            let lo = byte(t.lo, i)? as u32;
            let hi = byte(t.hi, i)? as u32;
            let bank = match t.bank {
                Some(b) => byte(b, i)? as u32,
                None => t.lo >> 16,
            };
            Ok(bank << 16 | hi << 8 | lo)
        })
        .collect()
}

#[derive(Debug, Default)]
pub struct Trace {
    pub entry: u32,
    pub entry_mx: Option<Mx>,
    pub insns: Vec<Insn>,
    pub labels: BTreeSet<u32>,
    pub callees: BTreeSet<u32>,
    pub tail_exits: BTreeSet<u32>,
    pub jumps: BTreeSet<u32>,
    pub jump_tables: Vec<JumpTable>,
    pub warnings: Vec<String>,
}

fn operand_len(mode: Mode, mx: Mx) -> Result<usize, &'static str> {
    use Mode::*;
    Ok(match mode {
        Imp | Acc => 0,
        ImmM => match mx.m {
            W::W8 => 1,
            W::W16 => 2,
            W::Unknown => return Err("M width unknown"),
        },
        ImmX => match mx.x {
            W::W8 => 1,
            W::W16 => 2,
            W::Unknown => return Err("X width unknown"),
        },
        Imm8 | Dp | DpX | DpY | DpInd | DpIndX | DpIndY | DpIndLong | DpIndLongY | Sr | SrIndY | Rel8 => 1,
        Imm16 | Abs | AbsX | AbsY | AbsInd | AbsIndX | AbsIndLong | Rel16 | Block => 2,
        AbsLong | AbsLongX => 3,
    })
}

/// Decode one instruction. Returns the instruction and the address after it (same bank, wrapping).
pub fn decode(mem: &dyn Mem, at: u32, mx: Mx) -> Result<(Insn, u16), String> {
    let bank = (at >> 16) as u8;
    let pc = at as u16;
    let rd = |o: u16| mem.read(bank, pc.wrapping_add(o)).ok_or_else(|| format!("{} not readable ROM", fmt_addr(full(bank, pc.wrapping_add(o)))));
    let op = rd(0)?;
    let (mn, mode) = OPS[op as usize];
    let n = operand_len(mode, mx).map_err(|e| format!("{e} at {} ({mn})", fmt_addr(at)))?;
    let mut bytes = vec![op];
    for i in 1..=n {
        bytes.push(rd(i as u16)?);
    }
    let next = pc.wrapping_add(1 + n as u16);
    let v8 = || bytes[1] as u32;
    let v16 = || bytes[1] as u32 | (bytes[2] as u32) << 8;
    let v24 = || v16() | (bytes[3] as u32) << 16;
    use Mode::*;
    let operand = match mode {
        Imp => String::new(),
        Acc => "A".into(),
        ImmM | ImmX => if n == 1 { format!("#${:02X}", v8()) } else { format!("#${:04X}", v16()) },
        Imm8 => format!("#${:02X}", v8()),
        Imm16 => format!("#${:04X}", v16()),
        Dp => format!("${:02X}", v8()),
        DpX => format!("${:02X},X", v8()),
        DpY => format!("${:02X},Y", v8()),
        DpInd => format!("(${:02X})", v8()),
        DpIndX => format!("(${:02X},X)", v8()),
        DpIndY => format!("(${:02X}),Y", v8()),
        DpIndLong => format!("[${:02X}]", v8()),
        DpIndLongY => format!("[${:02X}],Y", v8()),
        Abs => format!("${:04X}", v16()),
        AbsX => format!("${:04X},X", v16()),
        AbsY => format!("${:04X},Y", v16()),
        AbsLong => format!("${:06X}", v24()),
        AbsLongX => format!("${:06X},X", v24()),
        AbsInd => format!("(${:04X})", v16()),
        AbsIndX => format!("(${:04X},X)", v16()),
        AbsIndLong => format!("[${:04X}]", v16()),
        Sr => format!("${:02X},S", v8()),
        SrIndY => format!("(${:02X},S),Y", v8()),
        Rel8 => {
            let t = next.wrapping_add(bytes[1] as i8 as u16);
            format!("L_{:06X}", full(bank, t))
        }
        Rel16 => {
            let t = next.wrapping_add(v16() as u16);
            format!("L_{:06X}", full(bank, t))
        }
        Block => format!("dst=${:02X} src=${:02X}", bytes[1], bytes[2]),
    };
    let text = if operand.is_empty() { mn.to_string() } else { format!("{mn} {operand}") };
    Ok((Insn { at, op, bytes, mx, text }, next))
}

fn rel_target(i: &Insn, next: u16) -> u32 {
    let bank = (i.at >> 16) as u8;
    let off = if i.bytes.len() == 2 { i.bytes[1] as i8 as u16 } else { u16::from_le_bytes([i.bytes[1], i.bytes[2]]) };
    full(bank, next.wrapping_add(off))
}

fn read_table(mem: &dyn Mem, at: u32, count: usize, long: bool, code_bank: u8) -> Result<Vec<u32>, String> {
    let bank = (at >> 16) as u8;
    let base = at as u16;
    let step = if long { 3 } else { 2 };
    (0..count)
        .map(|i| {
            let o = base.wrapping_add((i * step) as u16);
            let b = |k: u16| mem.read(bank, o.wrapping_add(k)).ok_or_else(|| format!("table entry at {} unreadable", fmt_addr(full(bank, o))));
            let lo = b(0)? as u32 | (b(1)? as u32) << 8;
            Ok(if long { lo | (b(2)? as u32) << 16 } else { full(code_bank, lo as u16) })
        })
        .collect()
}

/// Follow control flow from `entry` with the given starting M/X state.
pub fn trace(mem: &dyn Mem, entry: u32, mx0: Mx, opt: &Options) -> Trace {
    let max = if opt.max_insns == 0 { 4000 } else { opt.max_insns };
    let mut t = Trace { entry, entry_mx: Some(mx0), ..Default::default() };
    let mut seen: BTreeMap<u32, BTreeSet<Mx>> = BTreeMap::new();
    let mut work: Vec<(u32, Mx)> = vec![(entry, mx0)];
    let mut preserved_calls = 0usize;

    while let Some((at, mx)) = work.pop() {
        if at != entry && opt.stop_at.contains(&at) {
            t.tail_exits.insert(at);
            continue;
        }
        let states = seen.entry(at).or_default();
        if states.contains(&mx) {
            continue;
        }
        if !states.is_empty() {
            let prev: Vec<String> = states.iter().map(|s| s.label()).collect();
            t.warnings.push(format!("M/X conflict at {}: reached as {} and {}", fmt_addr(at), mx.label(), prev.join(",")));
        }
        states.insert(mx);
        if t.insns.len() >= max {
            t.warnings.push(format!("stopped after {max} instructions; likely decoding data as code, or the unit needs splitting"));
            break;
        }
        let (insn, next) = match decode(mem, at, mx) {
            Ok(v) => v,
            Err(e) => {
                t.warnings.push(format!("path stopped: {e}"));
                continue;
            }
        };
        let bank = (at >> 16) as u8;
        let next_full = full(bank, next);
        let mut fall = Some((next_full, mx));
        match insn.op {
            0xC2 => fall = Some((next_full, mx.apply(insn.bytes[1], false))), // REP
            0xE2 => fall = Some((next_full, mx.apply(insn.bytes[1], true))),  // SEP
            0x28 => {
                // PLP: width comes from the stack
                t.warnings.push(format!("PLP at {}: M/X unknown afterward until REP/SEP", fmt_addr(at)));
                fall = Some((next_full, Mx { m: W::Unknown, x: W::Unknown }));
            }
            0xFB => t.warnings.push(format!("XCE at {}: emulation-mode switch, check E flag by hand", fmt_addr(at))),
            0x40 | 0x60 | 0x6B | 0xDB | 0x00 | 0x02 => fall = None, // RTI RTS RTL STP BRK COP
            0x80 | 0x82 => {
                // BRA, BRL
                let tg = rel_target(&insn, next);
                t.labels.insert(tg);
                work.push((tg, mx));
                fall = None;
            }
            0x10 | 0x30 | 0x50 | 0x70 | 0x90 | 0xB0 | 0xD0 | 0xF0 => {
                let tg = rel_target(&insn, next);
                t.labels.insert(tg);
                work.push((tg, mx));
            }
            0x20 | 0x22 => {
                // JSR abs, JSL long
                let tg = if insn.op == 0x20 {
                    full(bank, u16::from_le_bytes([insn.bytes[1], insn.bytes[2]]))
                } else {
                    insn.bytes[1] as u32 | (insn.bytes[2] as u32) << 8 | (insn.bytes[3] as u32) << 16
                };
                t.callees.insert(tg);
                preserved_calls += 1;
                if opt.noreturn.contains(&tg) {
                    fall = None;
                    if let Some(&(n, long)) = opt.tables.get(&next_full) {
                        match read_table(mem, next_full, n, long, bank) {
                            Ok(targets) => t.jump_tables.push(JumpTable { at: next_full, entries: n, targets, kind: "inline after noreturn call" }),
                            Err(e) => t.warnings.push(e),
                        }
                    } else {
                        t.warnings.push(format!("call to noreturn {} at {}: if a table follows at {}, pass --table", fmt_addr(tg), fmt_addr(at), fmt_addr(next_full)));
                    }
                }
            }
            0x4C | 0x5C => {
                // JMP abs, JML long: follow unless it's another unit's entry
                let tg = if insn.op == 0x4C {
                    full(bank, u16::from_le_bytes([insn.bytes[1], insn.bytes[2]]))
                } else {
                    insn.bytes[1] as u32 | (insn.bytes[2] as u32) << 8 | (insn.bytes[3] as u32) << 16
                };
                t.jumps.insert(tg);
                t.labels.insert(tg);
                work.push((tg, mx));
                fall = None;
            }
            0x7C | 0xFC => {
                // JMP (abs,X) / JSR (abs,X): table in the current bank
                let tab = full(bank, u16::from_le_bytes([insn.bytes[1], insn.bytes[2]]));
                match opt.tables.get(&tab) {
                    Some(&(n, long)) => match read_table(mem, tab, n, long, bank) {
                        Ok(targets) => t.jump_tables.push(JumpTable { at: tab, entries: n, targets, kind: if insn.op == 0x7C { "JMP (abs,X)" } else { "JSR (abs,X)" } }),
                        Err(e) => t.warnings.push(e),
                    },
                    None => t.warnings.push(format!("jump table at {} used by {} at {}: count unknown, pass --table {}=N", fmt_addr(tab), insn.text, fmt_addr(at), unit_id(tab))),
                }
                if insn.op == 0x7C {
                    fall = None;
                }
            }
            0x6C | 0xDC => {
                t.warnings.push(format!("indirect jump {} at {}: target is runtime data", insn.text, fmt_addr(at)));
                fall = None;
            }
            _ => {}
        }
        t.insns.push(insn);
        if let Some(f) = fall {
            work.push(f);
        }
    }
    for st in &opt.split_tables {
        match read_split_table(mem, st) {
            Ok(targets) => t.jump_tables.push(JumpTable { at: st.lo, entries: st.count, targets, kind: "split byte tables" }),
            Err(e) => t.warnings.push(e),
        }
    }
    if preserved_calls > 0 {
        t.warnings.push(format!("{preserved_calls} call(s) assume the callee preserves M/X; confirm when the callee is mapped"));
    }
    t.insns.sort_by_key(|i| (i.at, i.mx));
    t
}

/// Decode a straight range, tracking REP/SEP. Stops at the first undecodable instruction.
pub fn linear(mem: &dyn Mem, start: u32, end: u32, mx0: Mx) -> (Vec<Insn>, Option<String>) {
    let mut out = vec![];
    let mut at = start;
    let mut mx = mx0;
    while at <= end {
        match decode(mem, at, mx) {
            Ok((i, next)) => {
                match i.op {
                    0xC2 => mx = mx.apply(i.bytes[1], false),
                    0xE2 => mx = mx.apply(i.bytes[1], true),
                    0x28 => mx = Mx { m: W::Unknown, x: W::Unknown },
                    _ => {}
                }
                out.push(i);
                let n = full((at >> 16) as u8, next);
                if n <= at {
                    break; // wrapped
                }
                at = n;
            }
            Err(e) => return (out, Some(e)),
        }
    }
    (out, None)
}

/// Merge instruction byte spans into inclusive address ranges.
pub fn bounds(insns: &[Insn]) -> Vec<(u32, u32)> {
    let mut spans: Vec<(u32, u32)> = insns.iter().map(|i| (i.at, i.at + i.bytes.len() as u32 - 1)).collect();
    spans.sort();
    let mut out: Vec<(u32, u32)> = vec![];
    for (s, e) in spans {
        match out.last_mut() {
            Some(last) if s <= last.1 + 1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

pub fn render_listing(insns: &[Insn], labels: &BTreeSet<u32>) -> String {
    let mut s = String::new();
    let mut prev_end: Option<u32> = None;
    for i in insns {
        if let Some(p) = prev_end {
            if i.at > p + 1 {
                s.push('\n');
            }
        }
        if labels.contains(&i.at) {
            let _ = writeln!(s, "L_{:06X}:", i.at);
        }
        let hex: Vec<String> = i.bytes.iter().map(|b| format!("{b:02X}")).collect();
        let _ = writeln!(s, "  {}  {:<12} {:<22} ; {}", fmt_addr(i.at), hex.join(" "), i.text, i.mx.label());
        prev_end = Some(i.at + i.bytes.len() as u32 - 1);
    }
    s
}

/// The `[map]` block for the unit's ledger file, plus warnings as TOML comments.
pub fn render_summary(t: &Trace) -> String {
    let mut s = String::new();
    let q = |v: Vec<String>| v.iter().map(|x| format!("\"{x}\"")).collect::<Vec<_>>().join(", ");
    let b: Vec<String> = bounds(&t.insns).iter().map(|(a, e)| format!("{}-{}", fmt_addr(*a), fmt_addr(*e))).collect();
    let mut callees: BTreeSet<u32> = t.callees.clone();
    callees.extend(&t.tail_exits);
    let _ = writeln!(s, "[map]");
    let _ = writeln!(s, "bounds = [{}]", q(b));
    let _ = writeln!(s, "mx_entry = \"{}\"", t.entry_mx.map(|m| m.label()).unwrap_or_default());
    let _ = writeln!(s, "callees = [{}]", q(callees.iter().map(|&a| unit_id(a)).collect()));
    let jt: Vec<String> = t
        .jump_tables
        .iter()
        .map(|j| format!("{{ at = \"{}\", entries = {}, targets = [{}] }}", fmt_addr(j.at), j.entries, q(j.targets.iter().map(|&a| unit_id(a)).collect())))
        .collect();
    let _ = writeln!(s, "jump_tables = [{}]", jt.join(", "));
    let _ = writeln!(s, "size_insns = {}", t.insns.len());
    let _ = writeln!(s, "merged_into = \"\"");
    if !t.tail_exits.is_empty() {
        let _ = writeln!(s, "# tail exits (in callees): {}", t.tail_exits.iter().map(|&a| fmt_addr(a)).collect::<Vec<_>>().join(", "));
    }
    let followed: Vec<u32> = t.jumps.iter().copied().filter(|a| !t.tail_exits.contains(a)).collect();
    if !followed.is_empty() {
        let _ = writeln!(s, "# JMP/JML targets followed as part of this unit (pass --stop-at if any is another routine): {}", followed.iter().map(|&a| fmt_addr(a)).collect::<Vec<_>>().join(", "));
    }
    for w in &t.warnings {
        let _ = writeln!(s, "# WARN {w}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Sparse test memory: bank 00, bytes placed at given addresses.
    struct TestMem(HashMap<u32, u8>);
    impl TestMem {
        fn new(at: u32, code: &[u8]) -> Self {
            let mut m = HashMap::new();
            for (i, b) in code.iter().enumerate() {
                m.insert(at + i as u32, *b);
            }
            TestMem(m)
        }
        fn put(&mut self, at: u32, bytes: &[u8]) {
            for (i, b) in bytes.iter().enumerate() {
                self.0.insert(at + i as u32, *b);
            }
        }
    }
    impl Mem for TestMem {
        fn read(&self, bank: u8, addr: u16) -> Option<u8> {
            self.0.get(&full(bank, addr)).copied()
        }
    }

    fn m8x8() -> Mx {
        Mx::new(false, false)
    }

    #[test]
    fn immediate_size_follows_rep_sep() {
        // REP #$30 ; LDA #$1234 ; LDX #$5678 ; SEP #$20 ; LDA #$12 ; RTS
        let mem = TestMem::new(0x8000, &[0xC2, 0x30, 0xA9, 0x34, 0x12, 0xA2, 0x78, 0x56, 0xE2, 0x20, 0xA9, 0x12, 0x60]);
        let t = trace(&mem, 0x8000, m8x8(), &Options::default());
        let texts: Vec<&str> = t.insns.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts, ["REP #$30", "LDA #$1234", "LDX #$5678", "SEP #$20", "LDA #$12", "RTS"]);
        assert_eq!(t.insns[4].mx.label(), "m8x16");
        assert!(t.warnings.is_empty(), "{:?}", t.warnings);
        assert_eq!(bounds(&t.insns), vec![(0x8000, 0x800C)]);
    }

    #[test]
    fn branches_both_paths_and_labels() {
        // $8000 BEQ +2 ; $8002 LDA #$01 ; $8004 RTS ; $8005? no: BEQ target = $8004
        let mem = TestMem::new(0x8000, &[0xF0, 0x02, 0xA9, 0x01, 0x60]);
        let t = trace(&mem, 0x8000, m8x8(), &Options::default());
        assert_eq!(t.insns.len(), 3);
        assert!(t.labels.contains(&0x8004));
        assert!(render_listing(&t.insns, &t.labels).contains("L_008004:"));
    }

    #[test]
    fn calls_recorded_and_continue() {
        // JSR $9000 ; JSL $018000 ; RTL
        let mem = TestMem::new(0x8000, &[0x20, 0x00, 0x90, 0x22, 0x00, 0x80, 0x01, 0x6B]);
        let t = trace(&mem, 0x8000, m8x8(), &Options::default());
        assert_eq!(t.insns.len(), 3);
        assert_eq!(t.callees.iter().copied().collect::<Vec<_>>(), vec![0x009000, 0x018000]);
        let s = render_summary(&t);
        assert!(s.contains("callees = [\"009000\", \"018000\"]"), "{s}");
    }

    #[test]
    fn stop_at_makes_tail_exit() {
        // JMP $9000 where $9000 is another unit
        let mem = TestMem::new(0x8000, &[0x4C, 0x00, 0x90]);
        let mut opt = Options::default();
        opt.stop_at.insert(0x9000);
        let t = trace(&mem, 0x8000, m8x8(), &opt);
        assert_eq!(t.insns.len(), 1);
        assert!(t.tail_exits.contains(&0x9000));
        assert!(render_summary(&t).contains("\"009000\""));
    }

    #[test]
    fn jump_table_decoded_when_count_given() {
        // ASL A ; TAX ; JMP ($8010,X) ; table at $8010: $8100, $8200
        let mut mem = TestMem::new(0x8000, &[0x0A, 0xAA, 0x7C, 0x10, 0x80]);
        mem.put(0x8010, &[0x00, 0x81, 0x00, 0x82]);
        let mut opt = Options::default();
        let t0 = trace(&mem, 0x8000, m8x8(), &opt);
        assert!(t0.warnings.iter().any(|w| w.contains("count unknown")));
        opt.tables.insert(0x8010, (2, false));
        let t = trace(&mem, 0x8000, m8x8(), &opt);
        assert_eq!(t.jump_tables[0].targets, vec![0x8100, 0x8200]);
        assert!(render_summary(&t).contains("targets = [\"008100\", \"008200\"]"));
    }

    #[test]
    fn inline_table_after_noreturn_call() {
        // JSL $008781 ; .dw $8100, $8200
        let mem = TestMem::new(0x8000, &[0x22, 0x81, 0x87, 0x00, 0x00, 0x81, 0x00, 0x82]);
        let mut opt = Options::default();
        opt.noreturn.insert(0x008781);
        opt.tables.insert(0x8004, (2, false));
        let t = trace(&mem, 0x8000, m8x8(), &opt);
        assert_eq!(t.insns.len(), 1, "table bytes must not be decoded as code");
        assert_eq!(t.jump_tables[0].targets, vec![0x8100, 0x8200]);
    }

    #[test]
    fn split_byte_tables_decoded() {
        // lo at $8100, hi at $8110, bank at $8120; 2 entries -> $02:9000, $03:A0B0
        let mut mem = TestMem::new(0x8000, &[0x6B]);
        mem.put(0x8100, &[0x00, 0xB0]);
        mem.put(0x8110, &[0x90, 0xA0]);
        mem.put(0x8120, &[0x02, 0x03]);
        let mut opt = Options::default();
        opt.split_tables.push(SplitTable { lo: 0x8100, hi: 0x8110, bank: Some(0x8120), count: 2 });
        let t = trace(&mem, 0x8000, m8x8(), &opt);
        assert_eq!(t.jump_tables[0].targets, vec![0x029000, 0x03A0B0]);
        assert!(render_summary(&t).contains("{ at = \"$00:8100\", entries = 2, targets = [\"029000\", \"03a0b0\"] }"));
        // without a bank table, targets stay in the table's bank
        opt.split_tables[0].bank = None;
        let t = trace(&mem, 0x8000, m8x8(), &opt);
        assert_eq!(t.jump_tables[0].targets, vec![0x009000, 0x00A0B0]);
    }

    #[test]
    fn plp_makes_width_unknown_and_stops_on_immediate() {
        // PLP ; LDA #$?? -> cannot decode
        let mem = TestMem::new(0x8000, &[0x28, 0xA9, 0x00, 0x60]);
        let t = trace(&mem, 0x8000, m8x8(), &Options::default());
        assert_eq!(t.insns.len(), 1);
        assert!(t.warnings.iter().any(|w| w.contains("M width unknown")));
    }

    #[test]
    fn mx_conflict_reported() {
        // $8000 BCC $8005 ; $8002 REP #$20 ; $8004 NOP ; $8005 RTS
        // $8005 reached as m8 (branch) and m16 (fallthrough)
        let mem = TestMem::new(0x8000, &[0x90, 0x03, 0xC2, 0x20, 0xEA, 0x60]);
        let t = trace(&mem, 0x8000, m8x8(), &Options::default());
        assert!(t.warnings.iter().any(|w| w.contains("M/X conflict at $00:8005")), "{:?}", t.warnings);
    }

    #[test]
    fn parse_addr_forms() {
        for s in ["$00:8000", "00:8000", "008000", "0x008000"] {
            assert_eq!(parse_addr(s), Some(0x008000), "{s}");
        }
        assert_eq!(parse_addr("8000"), None);
    }
}
