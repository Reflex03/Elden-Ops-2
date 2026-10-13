//! Melty's test mode, ported from the code validate_recipe gives for slug "elden-ops-2".
//! While Melty tests a release it writes %LOCALAPPDATA%\Melty\test\elden-ops-2.json; its "checks"
//! names the file to append one line per check to: "pass <name>" or "fail <name>: why".
//! Without that file (a player's game), nothing happens. Never quits the game.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static CHECKS: OnceLock<Option<PathBuf>> = OnceLock::new();

fn checks() -> &'static Option<PathBuf> {
    CHECKS.get_or_init(|| {
        let test = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Melty").join("test").join("elden-ops-2.json");
        let text = std::fs::read_to_string(test).ok()?;
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        v.get("checks")?.as_str().map(PathBuf::from)
    })
}

pub fn active() -> bool {
    checks().is_some()
}

pub fn start() {
    if active() {
        report("mod-loads", true, None);
    }
}

pub fn report(name: &str, passed: bool, why: Option<&str>) {
    let Some(path) = checks() else { return };
    let mut line = format!("{} {name}", if passed { "pass" } else { "fail" });
    if let (false, Some(w)) = (passed, why) {
        line.push_str(": ");
        line.push_str(w);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
    crate::plugin::log(format!("melty test: {line}"));
}
