//! 65816 opcode table: mnemonic and addressing mode for all 256 opcodes.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Imp,        // no operand
    Acc,        // A
    ImmM,       // #imm, size from M flag
    ImmX,       // #imm, size from X flag
    Imm8,       // #imm8 (REP, SEP, BRK, COP, WDM)
    Imm16,      // #imm16 (PEA)
    Dp,         // dp
    DpX,        // dp,X
    DpY,        // dp,Y
    DpInd,      // (dp)
    DpIndX,     // (dp,X)
    DpIndY,     // (dp),Y
    DpIndLong,  // [dp]
    DpIndLongY, // [dp],Y
    Abs,        // abs
    AbsX,       // abs,X
    AbsY,       // abs,Y
    AbsLong,    // long
    AbsLongX,   // long,X
    AbsInd,     // (abs)
    AbsIndX,    // (abs,X)
    AbsIndLong, // [abs]
    Sr,         // sr,S
    SrIndY,     // (sr,S),Y
    Rel8,       // 8-bit relative branch
    Rel16,      // 16-bit relative (BRL, PER)
    Block,      // MVN/MVP: dst bank, src bank
}

use Mode::*;

pub static OPS: [(&str, Mode); 256] = [
    // 0x00
    ("BRK", Imm8), ("ORA", DpIndX), ("COP", Imm8), ("ORA", Sr),
    ("TSB", Dp), ("ORA", Dp), ("ASL", Dp), ("ORA", DpIndLong),
    ("PHP", Imp), ("ORA", ImmM), ("ASL", Acc), ("PHD", Imp),
    ("TSB", Abs), ("ORA", Abs), ("ASL", Abs), ("ORA", AbsLong),
    // 0x10
    ("BPL", Rel8), ("ORA", DpIndY), ("ORA", DpInd), ("ORA", SrIndY),
    ("TRB", Dp), ("ORA", DpX), ("ASL", DpX), ("ORA", DpIndLongY),
    ("CLC", Imp), ("ORA", AbsY), ("INC", Acc), ("TCS", Imp),
    ("TRB", Abs), ("ORA", AbsX), ("ASL", AbsX), ("ORA", AbsLongX),
    // 0x20
    ("JSR", Abs), ("AND", DpIndX), ("JSL", AbsLong), ("AND", Sr),
    ("BIT", Dp), ("AND", Dp), ("ROL", Dp), ("AND", DpIndLong),
    ("PLP", Imp), ("AND", ImmM), ("ROL", Acc), ("PLD", Imp),
    ("BIT", Abs), ("AND", Abs), ("ROL", Abs), ("AND", AbsLong),
    // 0x30
    ("BMI", Rel8), ("AND", DpIndY), ("AND", DpInd), ("AND", SrIndY),
    ("BIT", DpX), ("AND", DpX), ("ROL", DpX), ("AND", DpIndLongY),
    ("SEC", Imp), ("AND", AbsY), ("DEC", Acc), ("TSC", Imp),
    ("BIT", AbsX), ("AND", AbsX), ("ROL", AbsX), ("AND", AbsLongX),
    // 0x40
    ("RTI", Imp), ("EOR", DpIndX), ("WDM", Imm8), ("EOR", Sr),
    ("MVP", Block), ("EOR", Dp), ("LSR", Dp), ("EOR", DpIndLong),
    ("PHA", Imp), ("EOR", ImmM), ("LSR", Acc), ("PHK", Imp),
    ("JMP", Abs), ("EOR", Abs), ("LSR", Abs), ("EOR", AbsLong),
    // 0x50
    ("BVC", Rel8), ("EOR", DpIndY), ("EOR", DpInd), ("EOR", SrIndY),
    ("MVN", Block), ("EOR", DpX), ("LSR", DpX), ("EOR", DpIndLongY),
    ("CLI", Imp), ("EOR", AbsY), ("PHY", Imp), ("TCD", Imp),
    ("JML", AbsLong), ("EOR", AbsX), ("LSR", AbsX), ("EOR", AbsLongX),
    // 0x60
    ("RTS", Imp), ("ADC", DpIndX), ("PER", Rel16), ("ADC", Sr),
    ("STZ", Dp), ("ADC", Dp), ("ROR", Dp), ("ADC", DpIndLong),
    ("PLA", Imp), ("ADC", ImmM), ("ROR", Acc), ("RTL", Imp),
    ("JMP", AbsInd), ("ADC", Abs), ("ROR", Abs), ("ADC", AbsLong),
    // 0x70
    ("BVS", Rel8), ("ADC", DpIndY), ("ADC", DpInd), ("ADC", SrIndY),
    ("STZ", DpX), ("ADC", DpX), ("ROR", DpX), ("ADC", DpIndLongY),
    ("SEI", Imp), ("ADC", AbsY), ("PLY", Imp), ("TDC", Imp),
    ("JMP", AbsIndX), ("ADC", AbsX), ("ROR", AbsX), ("ADC", AbsLongX),
    // 0x80
    ("BRA", Rel8), ("STA", DpIndX), ("BRL", Rel16), ("STA", Sr),
    ("STY", Dp), ("STA", Dp), ("STX", Dp), ("STA", DpIndLong),
    ("DEY", Imp), ("BIT", ImmM), ("TXA", Imp), ("PHB", Imp),
    ("STY", Abs), ("STA", Abs), ("STX", Abs), ("STA", AbsLong),
    // 0x90
    ("BCC", Rel8), ("STA", DpIndY), ("STA", DpInd), ("STA", SrIndY),
    ("STY", DpX), ("STA", DpX), ("STX", DpY), ("STA", DpIndLongY),
    ("TYA", Imp), ("STA", AbsY), ("TXS", Imp), ("TXY", Imp),
    ("STZ", Abs), ("STA", AbsX), ("STZ", AbsX), ("STA", AbsLongX),
    // 0xA0
    ("LDY", ImmX), ("LDA", DpIndX), ("LDX", ImmX), ("LDA", Sr),
    ("LDY", Dp), ("LDA", Dp), ("LDX", Dp), ("LDA", DpIndLong),
    ("TAY", Imp), ("LDA", ImmM), ("TAX", Imp), ("PLB", Imp),
    ("LDY", Abs), ("LDA", Abs), ("LDX", Abs), ("LDA", AbsLong),
    // 0xB0
    ("BCS", Rel8), ("LDA", DpIndY), ("LDA", DpInd), ("LDA", SrIndY),
    ("LDY", DpX), ("LDA", DpX), ("LDX", DpY), ("LDA", DpIndLongY),
    ("CLV", Imp), ("LDA", AbsY), ("TSX", Imp), ("TYX", Imp),
    ("LDY", AbsX), ("LDA", AbsX), ("LDX", AbsY), ("LDA", AbsLongX),
    // 0xC0
    ("CPY", ImmX), ("CMP", DpIndX), ("REP", Imm8), ("CMP", Sr),
    ("CPY", Dp), ("CMP", Dp), ("DEC", Dp), ("CMP", DpIndLong),
    ("INY", Imp), ("CMP", ImmM), ("DEX", Imp), ("WAI", Imp),
    ("CPY", Abs), ("CMP", Abs), ("DEC", Abs), ("CMP", AbsLong),
    // 0xD0
    ("BNE", Rel8), ("CMP", DpIndY), ("CMP", DpInd), ("CMP", SrIndY),
    ("PEI", DpInd), ("CMP", DpX), ("DEC", DpX), ("CMP", DpIndLongY),
    ("CLD", Imp), ("CMP", AbsY), ("PHX", Imp), ("STP", Imp),
    ("JML", AbsIndLong), ("CMP", AbsX), ("DEC", AbsX), ("CMP", AbsLongX),
    // 0xE0
    ("CPX", ImmX), ("SBC", DpIndX), ("SEP", Imm8), ("SBC", Sr),
    ("CPX", Dp), ("SBC", Dp), ("INC", Dp), ("SBC", DpIndLong),
    ("INX", Imp), ("SBC", ImmM), ("NOP", Imp), ("XBA", Imp),
    ("CPX", Abs), ("SBC", Abs), ("INC", Abs), ("SBC", AbsLong),
    // 0xF0
    ("BEQ", Rel8), ("SBC", DpIndY), ("SBC", DpInd), ("SBC", SrIndY),
    ("PEA", Imm16), ("SBC", DpX), ("INC", DpX), ("SBC", DpIndLongY),
    ("SED", Imp), ("SBC", AbsY), ("PLX", Imp), ("XCE", Imp),
    ("JSR", AbsIndX), ("SBC", AbsX), ("INC", AbsX), ("SBC", AbsLongX),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_check_table() {
        assert_eq!(OPS[0xA9], ("LDA", ImmM));
        assert_eq!(OPS[0xA2], ("LDX", ImmX));
        assert_eq!(OPS[0xC2], ("REP", Imm8));
        assert_eq!(OPS[0xE2], ("SEP", Imm8));
        assert_eq!(OPS[0x22], ("JSL", AbsLong));
        assert_eq!(OPS[0x5C], ("JML", AbsLong));
        assert_eq!(OPS[0xDC], ("JML", AbsIndLong));
        assert_eq!(OPS[0x7C], ("JMP", AbsIndX));
        assert_eq!(OPS[0xFC], ("JSR", AbsIndX));
        assert_eq!(OPS[0x6B], ("RTL", Imp));
        assert_eq!(OPS[0x82], ("BRL", Rel16));
        assert_eq!(OPS[0x96], ("STX", DpY));
        assert_eq!(OPS[0xBE], ("LDX", AbsY));
        assert_eq!(OPS[0x54], ("MVN", Block));
    }

    #[test]
    fn every_mnemonic_is_three_letters() {
        assert!(OPS.iter().all(|(m, _)| m.len() == 3));
    }
}
