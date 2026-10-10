# Agent entry point (code repo)

Process lives in `../alttp-decomp-harness`. Read `../alttp-decomp-harness/CLAUDE.md` first, then your role file there.

## Rules for this repo

- Never commit ROM bytes, extracted assets, disassembly text, or large WRAM dumps. `.gitignore` covers the common file types, but check your diff anyway.
- One unit per commit, with the message prefixed by the unit id: `[02a1b2] translate Player_HandleSword`.
- Don't edit code outside the unit you own. Record needed changes as `followups` in the ledger.
- Routines go in `crates/alttp-core/src/bankXX/`. Doc comments start with the address and link the spec.
- `cargo test` must pass before you update the ledger.
