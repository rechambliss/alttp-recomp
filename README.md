# alttp-recomp

Rust reimplementation of *The Legend of Zelda: A Link to the Past* (EU, SNES), translated routine by routine and verified against the original.

Process, ledger, and agent instructions live in the companion repo `alttp-decomp-harness`.

You need your own ROM dump. It is never included or committed.

## Layout

| Crate | Purpose |
|---|---|
| `alttp-rom` | Loads and hash-checks your ROM, reads the header, maps LoROM addresses. |
| `alttp-core` | Translated game logic. `State` = WRAM (original layout) + hardware write log. |
| `alttp-verify` | Differential testing against a reference emulator. |
| `alttp-desktop` | Desktop host. Currently prints ROM info. |

| Dir | Purpose |
|---|---|
| `packs/` | Theme pack format (later). |
| `tools/` | Disassembler and capture tooling (Phase 0). Output stays local. |

## Run

```sh
cargo test
ALTTP_ROM=/path/to/alttp-eu.sfc cargo test -p alttp-rom      # also runs the real-ROM test
cargo run -p alttp-desktop -- /path/to/alttp-eu.sfc
```
