use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalPlayer {
    pub name: String,
    pub path: String,
}

#[tauri::command]
pub async fn launch_external_player(
    player_path: String,
    stream_url: String,
) -> Result<(), String> {
    let mut child = Command::new(&player_path)
        .arg(&stream_url)
        .spawn()
        .map_err(|e| format!("Failed to launch player '{}': {}", player_path, e))?;

    // Reap it when the user closes the player; otherwise it lingers as a zombie
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}

#[tauri::command]
pub async fn detect_external_players() -> Result<Vec<ExternalPlayer>, String> {
    let mut players = Vec::new();

    #[cfg(target_os = "linux")]
    {
        let candidates = [
            ("VLC", &["/usr/bin/vlc", "/snap/bin/vlc", "/usr/local/bin/vlc"][..]),
            ("MPV", &["/usr/bin/mpv", "/usr/local/bin/mpv"][..]),
            ("Celluloid", &["/usr/bin/celluloid"][..]),
        ];

        for (name, paths) in &candidates {
            for path in *paths {
                if Path::new(path).exists() {
                    players.push(ExternalPlayer {
                        name: name.to_string(),
                        path: path.to_string(),
                    });
                    break;
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let candidates = [
            ("VLC", &[
                r"C:\Program Files\VideoLAN\VLC\vlc.exe",
                r"C:\Program Files (x86)\VideoLAN\VLC\vlc.exe",
            ][..]),
            ("MPV", &[
                r"C:\Program Files\mpv\mpv.exe",
                r"C:\Program Files (x86)\mpv\mpv.exe",
            ][..]),
        ];

        for (name, paths) in &candidates {
            for path in *paths {
                if Path::new(path).exists() {
                    players.push(ExternalPlayer {
                        name: name.to_string(),
                        path: path.to_string(),
                    });
                    break;
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            ("VLC", &["/Applications/VLC.app/Contents/MacOS/VLC"][..]),
            ("MPV", &["/usr/local/bin/mpv", "/opt/homebrew/bin/mpv"][..]),
            ("IINA", &["/Applications/IINA.app/Contents/MacOS/IINA"][..]),
        ];

        for (name, paths) in &candidates {
            for path in *paths {
                if Path::new(path).exists() {
                    players.push(ExternalPlayer {
                        name: name.to_string(),
                        path: path.to_string(),
                    });
                    break;
                }
            }
        }
    }

    Ok(players)
}
