# Tools

Everything these tools output goes to gitignored directories (`disasm/`, `captures/`, `asset-cache/`). Never commit it.

## alttp-disasm

A flow-following 65816 disassembler that tracks M/X register width. It prints a listing and the ledger `[map]` block for one unit.

```sh
export ALTTP_ROM=/path/to/alttp-eu.sfc
cargo run -q -p alttp-disasm -- trace 00:8000 --mx m8x8 > disasm/008000.txt
cargo run -q -p alttp-disasm -- trace 00:8000 --mx m8x8 --summary-only
cargo run -q -p alttp-disasm -- linear 00:8000 00:80FF --mx m8x8
cargo run -q -p alttp-disasm -- --help
```

What it handles:
- REP/SEP width changes. PLP marks the width unknown, and an immediate after that stops the path with a warning.
- The same address reached with different widths, reported as an M/X conflict.
- Branches, BRA/BRL, and JMP/JML (followed, and listed so you can mark tail calls with `--stop-at`).
- JSR/JSL recorded as callees. The tool assumes they preserve M/X, and the warning says so.
- Jump tables via `JMP (abs,X)` and `JSR (abs,X)` with `--table`, and inline tables after `--noreturn` calls.
- Indirect `JMP (abs)` and `JML [abs]` are flagged, since their targets are runtime data.

Not handled yet: code that runs from WRAM, or tracking the E flag after `XCE` (flagged for manual review).

Still to build in Phase 0: state capture and the routine runner.
