//! `EldenOps.exe --play`: starts Elden Ring through the bundled ModEngine2, offline with Easy
//! Anti-Cheat off, and records what happened (launch.log; in a Melty test, the test's checks file).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn melty_checks() -> Option<PathBuf> {
    let test = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Melty").join("test").join("elden-ops-2.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(test).ok()?).ok()?;
    v.get("checks")?.as_str().map(PathBuf::from)
}

fn report_fail(why: &str) {
    if let Some(p) = melty_checks() {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "fail mod-loads: {why}");
        }
    }
}

#[cfg(windows)]
fn elden_ring_running() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        let mut ok = Process32FirstW(snap, &mut e);
        while ok != 0 {
            let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
            if String::from_utf16_lossy(&e.szExeFile[..len]).eq_ignore_ascii_case("eldenring.exe") {
                found = true;
                break;
            }
            ok = Process32NextW(snap, &mut e);
        }
        CloseHandle(snap);
        found
    }
}

#[cfg(not(windows))]
fn elden_ring_running() -> bool {
    false
}

/// The Elden Ring exe under the folder Melty passes ({game}).
pub fn elden_ring_exe(game: &Path) -> Option<PathBuf> {
    [game.join("Game").join("eldenring.exe"), game.join("eldenring.exe")].into_iter().find(|p| p.exists())
}

pub fn play(managed: &Path) -> i32 {
    let mut log = std::fs::File::create(crate::data_dir().join("launch.log")).ok();
    let mut say = |m: String| {
        println!("{m}");
        if let Some(f) = log.as_mut() {
            let _ = writeln!(f, "{m}");
        }
    };
    if elden_ring_running() {
        say("note: an eldenring.exe was already running before this Play".into());
    }
    let me2 = managed.join("modengine2");
    let launcher = me2.join("modengine2_launcher.exe");
    let config = me2.join("config_eldenops.toml");
    let mut cmd = Command::new(&launcher);
    cmd.current_dir(&me2).arg("-t").arg("er").arg("-c").arg(&config);
    match std::env::var_os("ELDENOPS_ER").map(PathBuf::from) {
        Some(game) => match elden_ring_exe(&game) {
            Some(exe) => {
                say(format!("Elden Ring: {}", exe.display()));
                cmd.arg("-p").arg(exe);
            }
            None => say(format!("no eldenring.exe under {}; letting ModEngine2 find the game through Steam", game.display())),
        },
        None => say("ELDENOPS_ER not set; letting ModEngine2 find the game through Steam".into()),
    }
    say(format!("starting {} (cwd {})", launcher.display(), me2.display()));
    let out = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            let why = format!("could not start ModEngine2's launcher {}: {e}", launcher.display());
            say(why.clone());
            report_fail(&why);
            return 1;
        }
    };
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let text = text.trim();
    say(format!("ModEngine2 launcher exited {:?}: {text}", out.status.code()));
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(45) {
        if elden_ring_running() {
            say(format!("eldenring.exe is running after {:.1}s", t0.elapsed().as_secs_f32()));
            return 0;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let why = format!(
        "Elden Ring did not start: ModEngine2's launcher exited {:?}{}",
        out.status.code(),
        if text.is_empty() { String::new() } else { format!(" saying \"{}\"", text.lines().last().unwrap_or("").chars().take(200).collect::<String>()) }
    );
    say(why.clone());
    report_fail(&why);
    1
}
