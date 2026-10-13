//! Elden Ops first-Play setup.
//!
//! Melty runs `EldenOpsSetup.exe --prepare` once, with ELDENOPS_BO2 = the player's Black Ops II
//! folder. It reads only alias *names* from the player's own BO2 Zombies fastfiles (OpenAssetTools'
//! Unlinker with --skip-obj, which loads no audio), then finds where each sound lives in the
//! player's own .sabl/.sabs banks. The result is a small index of names and offsets; no BO2 audio
//! is written anywhere. The mod reads the sounds from the banks where they are.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

mod aliases;
use aliases::ALIASES;

/// Zombies fastfiles to read alias tables from, in order; the rest of zone/all/zm_*.ff follow.
const PREFERRED_ZONES: &[&str] = &["zm_transit", "zm_nuked", "zm_highrise", "zm_prison", "zm_buried", "zm_tomb"];

pub const READY_TAG: &str = "eldenops-0.1.0-prep-1";

struct Ctx {
    log: std::fs::File,
}

impl Ctx {
    fn say(&mut self, m: impl AsRef<str>) {
        let m = m.as_ref();
        println!("{m}");
        let _ = writeln!(self.log, "{m}");
    }
}

fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("EldenOps")
}

fn managed_dir(exe: &Path) -> PathBuf {
    if let Some(m) = std::env::var_os("ELDENOPS_MANAGED") {
        return PathBuf::from(m);
    }
    // {managed}/modengine2/eldenops/EldenOpsSetup.exe
    exe.parent().and_then(Path::parent).and_then(Path::parent).map(Path::to_path_buf).unwrap_or_default()
}

fn zones(bo2: &Path) -> Vec<PathBuf> {
    let all = bo2.join("zone").join("all");
    let mut found: Vec<PathBuf> = std::fs::read_dir(&all)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let n = p.file_name().unwrap().to_string_lossy().to_ascii_lowercase();
                    n.starts_with("zm_") && n.ends_with(".ff") && !n.contains("_patch")
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort_by_key(|p| {
        let stem = p.file_stem().unwrap().to_string_lossy().to_ascii_lowercase();
        (PREFERRED_ZONES.iter().position(|z| *z == stem).unwrap_or(usize::MAX), stem)
    });
    found
}

fn run_unlinker(cx: &mut Ctx, unlinker: &Path, zone: &Path, out: &Path, filtered: bool) -> bool {
    let mut cmd = Command::new(unlinker);
    cmd.arg("--skip-obj").arg("--no-color");
    if filtered {
        cmd.arg("--include-assets").arg("soundbank"); // T6 asset type name (OpenAssetTools GameT6.cpp)
    }
    cmd.arg("--output-folder").arg(out).arg(zone);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    match cmd.output() {
        Ok(o) => {
            let tail: String = String::from_utf8_lossy(&o.stdout).lines().rev().take(3).collect::<Vec<_>>().join(" | ");
            cx.say(format!("  unlinker exit {:?}: {tail}", o.status.code()));
            o.status.success()
        }
        Err(e) => {
            cx.say(format!("  could not run {}: {e}", unlinker.display()));
            false
        }
    }
}

/// Returns how many alias tables were read.
fn collect_aliases(dir: &Path, wanted: &mut HashMap<String, String>) -> usize {
    let mut tables = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.to_string_lossy().to_ascii_lowercase().ends_with(".aliases.csv") {
                if let Ok(t) = std::fs::read_to_string(&p) {
                    tables += 1;
                    for (name, file) in bo2snd::parse_alias_csv(&t) {
                        if ALIASES.contains(&name.as_str()) && !wanted.contains_key(&name) {
                            wanted.insert(name, file);
                        }
                    }
                }
            }
        }
    }
    tables
}

fn prepare(cx: &mut Ctx, exe: &Path) -> Result<(), String> {
    let bo2 = std::env::var_os("ELDENOPS_BO2").map(PathBuf::from).ok_or("ELDENOPS_BO2 is not set: Melty passes the Black Ops II folder here")?;
    cx.say(format!("Black Ops II folder: {}", bo2.display()));
    let zone_list = zones(&bo2);
    if zone_list.is_empty() {
        return Err(format!(
            "No Black Ops II Zombies maps found in {}. Install Call of Duty: Black Ops II - Zombies, then press Play.",
            bo2.join("zone").join("all").display()
        ));
    }
    let unlinker = exe.parent().unwrap().join("oat").join("Unlinker.exe");
    if !unlinker.exists() {
        return Err(format!("{} is missing. Reinstall the mod.", unlinker.display()));
    }
    let work = data_dir().join("alias-scan");
    let _ = std::fs::remove_dir_all(&work);
    let mut wanted: HashMap<String, String> = HashMap::new();
    let mut filtered = true;
    for zone in &zone_list {
        if wanted.len() == ALIASES.len() {
            break;
        }
        cx.say(format!("Reading alias names from {}", zone.display()));
        let out = work.join(zone.file_stem().unwrap());
        run_unlinker(cx, &unlinker, zone, &out, filtered);
        if collect_aliases(&out, &mut wanted) == 0 && filtered {
            // No alias table came out: this Unlinker build may filter differently. Retry unfiltered.
            filtered = false;
            cx.say("  no alias tables written; retrying without the asset filter");
            let _ = std::fs::remove_dir_all(&out);
            run_unlinker(cx, &unlinker, zone, &out, false);
            let n = collect_aliases(&out, &mut wanted);
            cx.say(format!("  {n} alias tables"));
        }
        let _ = std::fs::remove_dir_all(&out);
        cx.say(format!("  {} of {} sounds named so far", wanted.len(), ALIASES.len()));
    }
    let _ = std::fs::remove_dir_all(&work);

    let mut index = bo2snd::locate(&bo2, &wanted, &mut |m| {
        let _ = writeln!(cx.log, "{m}");
    });
    index.missing = ALIASES.iter().filter(|a| !index.sounds.contains_key(**a)).map(|a| a.to_string()).collect();
    for (alias, s) in &index.sounds {
        cx.say(format!("  {alias} -> {} @ {} ({} bytes, fmt {}, {} Hz, {} ch)", s.bank, s.offset, s.size, s.format, s.rate, s.channels));
    }
    for m in &index.missing {
        cx.say(format!("  not found: {m}"));
    }
    if index.sounds.is_empty() {
        return Err("Could not find Black Ops II Zombies' sounds in its sound banks. Verify Black Ops II's files in Steam, then press Play.".into());
    }
    let path = data_dir().join("bo2_index.json");
    std::fs::write(&path, serde_json::to_string_pretty(&index).unwrap()).map_err(|e| format!("writing {}: {e}", path.display()))?;
    cx.say(format!("Wrote {} ({} of {} sounds)", path.display(), index.sounds.len(), ALIASES.len()));
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let exe = std::env::current_exe().unwrap_or_default();
    let _ = std::fs::create_dir_all(data_dir());
    let log = std::fs::File::create(data_dir().join("setup.log")).expect("setup log");
    let mut cx = Ctx { log };
    if !args.iter().any(|a| a == "--prepare") {
        cx.say("usage: EldenOpsSetup.exe --prepare   (ELDENOPS_BO2 = Black Ops II folder)");
        std::process::exit(2);
    }
    let managed = managed_dir(&exe);
    let ready = managed.join(".melty-ready");
    let failure = managed.join(".melty-failure.txt");
    let _ = std::fs::remove_file(&failure);
    match prepare(&mut cx, &exe) {
        Ok(()) => {
            let _ = std::fs::write(&ready, READY_TAG);
            cx.say("ready");
        }
        Err(e) => {
            cx.say(format!("setup failed: {e}"));
            let _ = std::fs::write(&failure, &e);
            std::process::exit(1);
        }
    }
}
