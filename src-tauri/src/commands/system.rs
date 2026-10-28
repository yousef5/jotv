//! What this computer has of the programs JoTV needs, for the setup checklist
//! on the first screen and in Settings.

use crate::tools::{program, NoConsole};
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    /// Package-ish id the frontend maps to install commands: mpv, ffmpeg, ffprobe, yt-dlp, xwayland
    pub id: String,
    pub found: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    /// Shipped inside JoTV (Windows) rather than installed on the system
    pub bundled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemStatus {
    /// linux, windows or macos
    pub os: String,
    /// `ID` then `ID_LIKE` from /etc/os-release, e.g. ["cachyos", "arch"]
    pub distro: Vec<String>,
    pub distro_name: Option<String>,
    pub tools: Vec<ToolStatus>,
}

/// Runs `name <flag>` and pulls a version out of its first line.
fn check(id: &str, flag: &str) -> ToolStatus {
    let exe = program(id);
    let bundled = exe.is_absolute();
    let output = Command::new(&exe).no_console().arg(flag).output().ok().filter(|o| o.status.success());
    let version = output.as_ref().and_then(|o| {
        let text = String::from_utf8_lossy(&o.stdout);
        version_in(text.lines().next().unwrap_or(""))
    });
    ToolStatus {
        id: id.to_string(),
        found: output.is_some(),
        version,
        path: output.is_some().then(|| find_on_path(&exe).unwrap_or(exe).to_string_lossy().into_owned()),
        bundled,
    }
}

/// "mpv v0.39.0-dirty Copyright…" → "0.39.0", "ffmpeg version n7.1 …" → "7.1",
/// "2025.10.22" (yt-dlp) → "2025.10.22"
fn version_in(line: &str) -> Option<String> {
    line.split_whitespace()
        .map(|w| w.trim_start_matches(['v', 'n']))
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()) && w.contains('.'))
        .map(|w| w.split(['-', '+', '~']).next().unwrap_or(w).to_string())
}

fn find_on_path(exe: &PathBuf) -> Option<PathBuf> {
    if exe.is_absolute() {
        return Some(exe.clone());
    }
    let dirs = std::env::var_os("PATH")?;
    std::env::split_paths(&dirs).map(|d| d.join(exe)).find(|p| p.is_file())
}

#[cfg(target_os = "linux")]
fn os_release() -> (Vec<String>, Option<String>) {
    let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
            .map(|v| v.trim_matches('"').to_string())
    };
    let mut ids: Vec<String> = field("ID").into_iter().collect();
    if let Some(like) = field("ID_LIKE") {
        ids.extend(like.split_whitespace().map(str::to_string));
    }
    (ids, field("PRETTY_NAME").or_else(|| field("NAME")))
}

/// In-app video needs an X11 display: a plain X11 session, or XWayland on Wayland.
#[cfg(target_os = "linux")]
fn x11_status() -> ToolStatus {
    let display = std::env::var("DISPLAY").ok().filter(|d| !d.is_empty());
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    ToolStatus {
        id: if wayland { "xwayland" } else { "x11" }.to_string(),
        found: display.is_some(),
        version: None,
        path: display,
        bundled: false,
    }
}

#[tauri::command]
pub async fn system_check() -> Result<SystemStatus, String> {
    tauri::async_runtime::spawn_blocking(|| {
        #[allow(unused_mut)] // only Linux adds to it
        let mut tools = vec![
            check("mpv", "--version"),
            check("ffmpeg", "-version"),
            check("ffprobe", "-version"),
            check("yt-dlp", "--version"),
        ];
        #[cfg(target_os = "linux")]
        tools.push(x11_status());

        #[cfg(target_os = "linux")]
        let (distro, distro_name) = os_release();
        #[cfg(not(target_os = "linux"))]
        let (distro, distro_name) = (Vec::new(), None);

        SystemStatus { os: std::env::consts::OS.to_string(), distro, distro_name, tools }
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::version_in;

    #[test]
    fn reads_versions() {
        assert_eq!(version_in("mpv v0.39.0-dirty Copyright © 2000-2024 mpv/MPlayer/mplayer2 projects").as_deref(), Some("0.39.0"));
        assert_eq!(version_in("ffmpeg version n7.1 Copyright (c) 2000-2024").as_deref(), Some("7.1"));
        assert_eq!(version_in("ffprobe version 7.1.1 Copyright").as_deref(), Some("7.1.1"));
        assert_eq!(version_in("2025.10.22").as_deref(), Some("2025.10.22"));
        assert_eq!(version_in("no version here"), None);
    }
}
