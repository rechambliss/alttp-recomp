use alttp_disasm::*;
use alttp_rom::Rom;
use std::process::ExitCode;

const USAGE: &str = "\
alttp-disasm: M/X-aware 65816 disassembler (output is local only, never commit it)

USAGE
  alttp-disasm trace  <ADDR> --mx <MX> [options]
  alttp-disasm linear <START> <END> --mx <MX> [--rom PATH]

ADDR forms: $00:8000  00:8000  008000
MX forms:   m8x8  m8x16  m16x8  m16x16   (required: never guess widths)

trace options
  --rom PATH            ROM file (default: $ALTTP_ROM)
  --stop-at A,B,...     other units' entries; jumping there ends the path (tail exit)
  --noreturn A,B,...    routines that never return to the caller
  --table A=N[L],...    jump table at A with N entries (L = 24-bit entries)
  --split-table LO,HI[,BANK]=N
                        jump table stored as parallel byte tables (repeatable).
                        No BANK table: targets are in LO's bank.
  --max N               instruction cap (default 4000)
  --summary-only        print only the ledger [map] block
  --listing-only        print only the listing
";

fn parse_mx(s: &str) -> Option<Mx> {
    match s {
        "m8x8" => Some(Mx::new(false, false)),
        "m8x16" => Some(Mx::new(false, true)),
        "m16x8" => Some(Mx::new(true, false)),
        "m16x16" => Some(Mx::new(true, true)),
        _ => None,
    }
}

fn parse_list(s: &str) -> Result<Vec<u32>, String> {
    s.split(',').filter(|x| !x.is_empty()).map(|x| parse_addr(x).ok_or(format!("bad address '{x}'"))).collect()
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return Ok(());
    }
    let mut pos = vec![];
    let mut rom_path = std::env::var("ALTTP_ROM").ok();
    let mut mx = None;
    let mut opt = Options::default();
    let (mut summary_only, mut listing_only) = (false, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().cloned().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--rom" => rom_path = Some(val()?),
            "--mx" => {
                let v = val()?;
                mx = Some(parse_mx(&v).ok_or(format!("bad --mx '{v}'"))?);
            }
            "--stop-at" => opt.stop_at.extend(parse_list(&val()?)?),
            "--noreturn" => opt.noreturn.extend(parse_list(&val()?)?),
            "--table" => {
                for spec in val()?.split(',') {
                    let (a, n) = spec.split_once('=').ok_or(format!("bad --table '{spec}', want ADDR=N"))?;
                    let long = n.ends_with(['L', 'l']);
                    let n: usize = n.trim_end_matches(['L', 'l']).parse().map_err(|_| format!("bad count in '{spec}'"))?;
                    opt.tables.insert(parse_addr(a).ok_or(format!("bad address '{a}'"))?, (n, long));
                }
            }
            "--split-table" => {
                let spec = val()?;
                let (addrs, n) = spec.split_once('=').ok_or(format!("bad --split-table '{spec}', want LO,HI[,BANK]=N"))?;
                let a = parse_list(addrs)?;
                if !(2..=3).contains(&a.len()) {
                    return Err(format!("--split-table needs 2 or 3 addresses, got {}", a.len()));
                }
                let count: usize = n.parse().map_err(|_| format!("bad count in '{spec}'"))?;
                opt.split_tables.push(SplitTable { lo: a[0], hi: a[1], bank: a.get(2).copied(), count });
            }
            "--max" => opt.max_insns = val()?.parse().map_err(|_| "bad --max")?,
            "--summary-only" => summary_only = true,
            "--listing-only" => listing_only = true,
            s if s.starts_with("--") => return Err(format!("unknown option {s}\n\n{USAGE}")),
            _ => pos.push(a.clone()),
        }
    }
    let mx = mx.ok_or(format!("--mx is required\n\n{USAGE}"))?;
    let rom = Rom::load(rom_path.ok_or("no ROM: pass --rom or set ALTTP_ROM")?).map_err(|e| e.to_string())?;

    match pos.first().map(String::as_str) {
        Some("trace") => {
            let entry = pos.get(1).and_then(|s| parse_addr(s)).ok_or("trace needs an address")?;
            let t = trace(&rom, entry, mx, &opt);
            if !summary_only {
                println!("; unit {}  entry {}  {}", unit_id(entry), fmt_addr(entry), mx.label());
                print!("{}", render_listing(&t.insns, &t.labels));
            }
            if !listing_only {
                if !summary_only {
                    println!();
                }
                print!("{}", render_summary(&t));
            }
        }
        Some("linear") => {
            let s = pos.get(1).and_then(|s| parse_addr(s)).ok_or("linear needs START")?;
            let e = pos.get(2).and_then(|s| parse_addr(s)).ok_or("linear needs END")?;
            let (insns, err) = linear(&rom, s, e, mx);
            print!("{}", render_listing(&insns, &Default::default()));
            if let Some(err) = err {
                println!("; stopped: {err}");
            }
        }
        _ => return Err(format!("expected 'trace' or 'linear'\n\n{USAGE}")),
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}
