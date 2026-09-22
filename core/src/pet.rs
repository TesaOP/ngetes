//! pet.rs — Peluncur jendela overlay Desktop Pet, port `src/server/pet.ts`.
//! Web tak bisa menembus batas browser, jadi pet jalan di jendela terpisah
//! always-on-top + transparan. Urutan peluncur:
//!   1. Shell Tauri (live2d-shell.exe "pet" url) — transparan + klik-tembus.
//!   2. Chrome/Edge --app (opaque, always-on-top via PowerShell) — fallback.

use std::path::Path;
use std::process::Child;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

struct PetState {
    proc: Option<Child>,
    pid: Option<u32>,
    helper_pid: Option<u32>,
    click_through: bool,
    shell: Option<&'static str>, // "tauri" | "browser"
}

impl Default for PetState {
    fn default() -> Self {
        PetState { proc: None, pid: None, helper_pid: None, click_through: false, shell: None }
    }
}

fn state() -> &'static Mutex<PetState> {
    static S: OnceLock<Mutex<PetState>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(PetState::default()))
}

fn shell_candidates(root: &Path) -> Vec<std::path::PathBuf> {
    vec![
        root.join("live2d-shell.exe"),
        root.join("agent-shell").join("target").join("release").join("live2d-shell.exe"),
    ]
}

fn find_shell_exe(root: &Path) -> Option<std::path::PathBuf> {
    shell_candidates(root).into_iter().find(|c| c.exists())
}

fn taskkill(pid: u32) {
    let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
}

/// GET /api/pet/state — status jendela pet.
pub fn status() -> Value {
    let mut s = state().lock().unwrap();
    // Proses bisa mati sendiri (user tutup) → sinkronkan.
    let dead = s.proc.as_mut().map(|p| matches!(p.try_wait(), Ok(Some(_)))).unwrap_or(false);
    if dead {
        s.proc = None;
        s.pid = None;
        s.helper_pid = None;
        s.click_through = false;
        s.shell = None;
    }
    json!({
        "running": s.proc.is_some() || s.pid.is_some(),
        "clickThrough": s.click_through,
        "shell": s.shell,
    })
}

/// POST /api/pet/clickthrough {on} — set flag klik-tembus (dibaca shell/pet.html).
pub fn set_click_through(on: bool) -> Value {
    let mut s = state().lock().unwrap();
    s.click_through = on;
    json!({ "ok": true, "clickThrough": on })
}

/// POST /api/pet/close — tutup jendela pet.
pub fn close() -> Value {
    let mut s = state().lock().unwrap();
    if let Some(mut p) = s.proc.take() {
        let _ = p.kill();
    }
    if let Some(h) = s.helper_pid.take() {
        taskkill(h);
    }
    if let Some(pid) = s.pid.take() {
        taskkill(pid);
    }
    s.click_through = false;
    s.shell = None;
    json!({ "ok": true })
}

/// POST /api/pet/launch — luncurkan jendela pet (port = server). {ok, how} / {ok:false, error}.
pub fn launch(root: &Path, port: u16) -> Value {
    close();
    let url = format!("http://127.0.0.1:{port}/pet.html");

    // Shell 1: Tauri (transparan + klik-tembus).
    if let Some(shell) = find_shell_exe(root) {
        match std::process::Command::new(&shell).args(["pet", &url]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
            Ok(child) => {
                let mut s = state().lock().unwrap();
                s.pid = Some(child.id());
                s.proc = Some(child);
                s.shell = Some("tauri");
                return json!({ "ok": true, "how": "tauri-shell (transparan + klik-tembus)" });
            }
            Err(e) => {
                eprintln!("[pet] shell Tauri gagal dijalankan: {e}");
            }
        }
    }

    // Shell 2: Chrome/Edge --app (fallback nol-build).
    let exe = match crate::browser::find_chromium() {
        Some(e) => e,
        None => {
            return json!({ "ok": false, "error": "Shell belum dibangun (build agent-shell) dan Chrome/Edge tidak ditemukan" });
        }
    };
    match std::process::Command::new(&exe)
        .args([
            &format!("--app={url}"),
            "--window-size=420,640",
            "--window-position=40,40",
            "--autoplay-policy=no-user-gesture-required",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => {
            let pid = child.id();
            {
                let mut s = state().lock().unwrap();
                s.pid = Some(pid);
                s.proc = Some(child);
                s.shell = Some("browser");
            }
            // Windows: paksa always-on-top via PowerShell SetWindowPos (async, best-effort).
            if cfg!(windows) {
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(2500));
                    let ps = format!(
                        "Add-Type -Name W -Namespace P -MemberDefinition '[DllImport(\"user32.dll\")] public static extern bool SetWindowPos(IntPtr h,IntPtr a,int x,int y,int cx,int cy,uint f);[DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr h);';$p=Get-Process -Id {pid} -ErrorAction Stop;$h=$p.MainWindowHandle;if($h -ne 0){{[P.W]::SetWindowPos($h,-1,0,0,0,0,0x0041);[P.W]::SetForegroundWindow($h)}}"
                    );
                    if let Ok(c) = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &ps]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
                        state().lock().unwrap().helper_pid = Some(c.id());
                    }
                });
            }
            json!({ "ok": true, "how": "app-window (Chrome/Edge --app + always-on-top)" })
        }
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_through_dan_status_awal() {
        // status awal: tidak running.
        let st = status();
        assert_eq!(st["running"], false);
        let r = set_click_through(true);
        assert_eq!(r["clickThrough"], true);
        assert_eq!(status()["clickThrough"], true);
        // reset supaya test lain bersih
        set_click_through(false);
    }
}
