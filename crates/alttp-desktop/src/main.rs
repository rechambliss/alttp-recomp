//! Desktop host. For now: verify the ROM and print its header.
//! Rendering, audio, and input arrive in roadmap Phase 3.

use alttp_rom::Rom;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1).or_else(|| std::env::var("ALTTP_ROM").ok()) else {
        eprintln!("usage: alttp-desktop <path-to-rom.sfc>   (or set ALTTP_ROM)");
        return ExitCode::from(2);
    };
    let rom = match Rom::load(&path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let h = rom.header();
    let ok = rom.computed_checksum() == h.checksum;
    println!("ROM      {} ({})", h.title, rom.region);
    println!("SHA-1    {}", rom.sha1);
    println!("map      ${:02X}  cart ${:02X}  sram ${:02X}  region ${:02X}  v{}", h.map_mode, h.cart_type, h.sram_size, h.region, h.version);
    println!("checksum ${:04X} ({})", h.checksum, if ok { "valid" } else { "MISMATCH" });
    println!("RESET    $00:{:04X}", h.vec_reset);
    println!("NMI      $00:{:04X}", h.vec_nmi);
    println!("IRQ      $00:{:04X}", h.vec_irq);
    let _state = alttp_core::State::new();
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
