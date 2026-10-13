//! The DLL ModEngine2 loads into Elden Ring (config_eldenops.toml external_dlls).

use crate::game::Game;
use crate::sheets::PERKS;
use eldenring::cs::{CSTaskGroupIndex, CSTaskImp};
use eldenring::fd4::FD4TaskData;
use fromsoftware_shared::SharedTaskImpExt;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

static LOG: Mutex<Option<std::fs::File>> = Mutex::new(None);
static MODULE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// {managed}: this DLL is at {managed}/modengine2/eldenops/eldenops.dll.
fn managed_dir() -> Option<PathBuf> {
    if let Some(m) = std::env::var_os("ELDENOPS_MANAGED") {
        return Some(PathBuf::from(m));
    }
    MODULE.get()?.parent()?.parent()?.parent().map(PathBuf::from)
}

/// A setting from the environment, else from {managed}/eldenops.env (Melty writes it before every Play).
pub fn setting(key: &str) -> Option<String> {
    if let Some(v) = std::env::var_os(key) {
        return Some(v.to_string_lossy().into_owned());
    }
    let text = std::fs::read_to_string(managed_dir()?.join("eldenops.env")).ok()?;
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("EldenOps")
}

pub fn log(m: impl AsRef<str>) {
    if let Ok(mut g) = LOG.lock() {
        if let Some(f) = g.as_mut() {
            let _ = writeln!(f, "[{:>8.1}] {}", START.get().map(|s| s.elapsed().as_secs_f32()).unwrap_or(0.0), m.as_ref());
            let _ = f.flush();
        }
    }
}

static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Elden Ring's own top-level window is up and visible.
fn game_window_visible() -> bool {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible};
    unsafe extern "system" fn each(w: HWND, found: LPARAM) -> BOOL {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(w, &mut pid) };
        if pid == unsafe { GetCurrentProcessId() } && unsafe { IsWindowVisible(w) } != 0 {
            unsafe { *(found as *mut bool) = true };
            return 0;
        }
        1
    }
    let mut found = false;
    unsafe { EnumWindows(Some(each), &mut found as *mut bool as LPARAM) };
    found
}

fn game_focused() -> bool {
    unsafe {
        let w = GetForegroundWindow();
        if w.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(w, &mut pid);
        pid == GetCurrentProcessId()
    }
}

fn perk_keys() -> [bool; 4] {
    let mut k = [false; 4];
    if game_focused() {
        for (i, p) in PERKS.iter().enumerate().take(4) {
            k[i] = unsafe { GetAsyncKeyState(p.key_vk) } as u16 & 0x8000 != 0;
        }
    }
    k
}

fn run() {
    START.get_or_init(Instant::now);
    let _ = std::fs::create_dir_all(data_dir());
    if let Ok(f) = std::fs::File::create(data_dir().join("eldenops.log")) {
        *LOG.lock().unwrap() = Some(f);
    }
    log(format!("Elden Ops {} loaded from {:?}; BO2 folder {:?}", env!("CARGO_PKG_VERSION"), MODULE.get(), setting("ELDENOPS_BO2")));
    std::panic::set_hook(Box::new(|i| log(format!("panic: {i}"))));
    crate::melty_test::start();

    // Touch nothing while Elden Ring boots: wait for its window (field note: early singleton scans
    // and early DirectX hooks are what break FromSoftware games at startup).
    let t0 = Instant::now();
    while !game_window_visible() {
        std::thread::sleep(Duration::from_millis(250));
    }
    log(format!("boot: game window visible after {:.1}s", t0.elapsed().as_secs_f32()));
    std::thread::sleep(Duration::from_secs(3));

    let audio = crate::audio::start(|m| log(m));

    std::thread::spawn(|| {
        // Hook the swap chain only once the game has been drawing for a while.
        std::thread::sleep(Duration::from_secs(7));
        log("boot: hooking DirectX 12 for the HUD");
        let hooks = hudhook::Hudhook::builder().with::<hudhook::hooks::dx12::ImguiDx12Hooks>(crate::hud::Overlay::default()).build();
        match hooks.apply() {
            Ok(()) => log("hud: DirectX 12 overlay hooked"),
            Err(e) => log(format!("hud: hook failed {e:?}")),
        }
    });

    log("boot: waiting for the game's task system");
    let cs_task = match CSTaskImp::wait_for_instance(Duration::MAX) {
        Ok(t) => t,
        Err(e) => {
            log(format!("CSTask not found: {e:?}"));
            return;
        }
    };
    log("game task system ready");
    let state = Mutex::new((Game::new(std::process::id().wrapping_mul(2654435761)), crate::er::ErWorld::new(), Instant::now(), 0usize));
    cs_task.run_recurring(
        move |_: &FD4TaskData| {
            let Ok(mut g) = state.lock() else { return };
            let (game, world, last, logged) = &mut *g;
            let dt = last.elapsed().as_secs_f32();
            *last = Instant::now();
            let keys = perk_keys();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| game.tick(dt, world, &audio, keys)));
            if r.is_err() {
                log("tick panicked; skipping frame");
            }
            for m in &game.log[*logged..] {
                log(m);
            }
            *logged = game.log.len();
            if let Ok(mut h) = crate::hud::HUD.lock() {
                *h = Some(game.hud());
            }
        },
        CSTaskGroupIndex::FrameBegin,
    );
}

/// # Safety
/// Called by the Windows loader.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(hmodule: usize, reason: u32, _: usize) -> i32 {
    if reason == 1 {
        let mut buf = [0u16; 1024];
        let n = unsafe { GetModuleFileNameW(hmodule as _, buf.as_mut_ptr(), buf.len() as u32) } as usize;
        if n > 0 {
            let _ = MODULE.set(PathBuf::from(String::from_utf16_lossy(&buf[..n])));
        }
        std::thread::spawn(|| {
            if std::panic::catch_unwind(run).is_err() {
                log("boot: stopped after a panic (the game keeps running without Elden Ops)");
            }
        });
    }
    1
}
