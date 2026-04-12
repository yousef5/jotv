use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::{Child, Command};
use std::sync::Mutex;
use tauri::State;

pub struct MpvState {
    pub process: Mutex<Option<Child>>,
    pub socket_path: Mutex<String>,
}

impl MpvState {
    pub fn new() -> Self {
        let socket = format!("/tmp/jotv-mpv-{}.sock", std::process::id());
        // Kill any stale MPV from previous app runs
        std::fs::remove_file(&socket).ok();
        MpvState {
            process: Mutex::new(None),
            socket_path: Mutex::new(socket),
        }
    }
}

impl Drop for MpvState {
    fn drop(&mut self) {
        if let Ok(mut proc) = self.process.lock() {
            if let Some(mut p) = proc.take() {
                p.kill().ok();
                p.wait().ok();
            }
        }
        if let Ok(socket) = self.socket_path.lock() {
            std::fs::remove_file(socket.as_str()).ok();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpvStatus {
    pub playing: bool,
    pub paused: bool,
    pub title: String,
}

fn send_mpv_command(socket_path: &str, command: &[&str]) -> Result<String, String> {
    let args: Vec<String> = command.iter().map(|s| format!("\"{}\"", s)).collect();
    let json = format!("{{ \"command\": [{}] }}\n", args.join(", "));

    let mut stream = UnixStream::connect(socket_path)
        .map_err(|e| format!("Cannot connect to MPV: {}", e))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2))).ok();

    stream.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to send command: {}", e))?;

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).ok();
    Ok(response)
}

/// Start MPV with a stream URL. MPV runs in its own window with IPC control.
#[tauri::command]
pub async fn mpv_play(
    url: String,
    title: String,
    mpv: State<'_, MpvState>,
) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();

    // Kill existing MPV if running
    {
        let mut proc = mpv.process.lock().unwrap();
        if let Some(mut p) = proc.take() {
            p.kill().ok();
            p.wait().ok();
        }
    }

    // Remove stale socket
    std::fs::remove_file(&socket).ok();

    // Build stream URL — prefer .ts over .m3u8 for direct server connection
    // .m3u8 goes through CDN which can be down; .ts goes directly to main server
    let stream_url = if url.contains(".m3u8") {
        url.replace(".m3u8", ".ts")
    } else {
        url.clone()
    };

    // Launch MPV — ultimate IPTV settings for zero cuts
    let child = Command::new("mpv")
        .args([
            &stream_url,
            &format!("--input-ipc-server={}", socket),
            &format!("--title=JoTV - {}", title),

            // ═══ CORE: raw stream, no youtube-dl ═══
            "--ytdl=no",
            "--hls-bitrate=max",

            // ═══ CACHE: massive buffer — the #1 defense against cuts ═══
            "--cache=yes",
            "--demuxer-max-bytes=300M",          // 300MB demuxer buffer
            "--demuxer-max-back-bytes=100M",     // 100MB back buffer
            "--cache-secs=300",                   // cache 5 MINUTES ahead
            "--demuxer-readahead-secs=120",      // read 2 minutes ahead always
            "--cache-pause-initial=yes",          // pause until buffer has enough data
            "--cache-pause-wait=3",               // resume after 3s of data cached

            // ═══ NETWORK: auto-reconnect on ANY failure — never give up ═══
            "--network-timeout=60",
            "--stream-lavf-o-append=reconnect=1",
            "--stream-lavf-o-append=reconnect_at_eof=1",
            "--stream-lavf-o-append=reconnect_streamed=1",
            "--stream-lavf-o-append=reconnect_on_network_error=1",
            "--stream-lavf-o-append=reconnect_on_http_error=4xx,5xx",
            "--stream-lavf-o-append=reconnect_delay_max=30",
            "--stream-lavf-o-append=reconnect_max_retries=-1",
            "--stream-lavf-o-append=reconnect_delay_total_max=3600",
            "--stream-lavf-o-append=timeout=20000000",
            "--stream-lavf-o-append=rw_timeout=20000000",

            // ═══ DEMUXER: handle broken/stuttery streams ═══
            "--demuxer=lavf",
            "--demuxer-lavf-analyzeduration=10",
            "--demuxer-lavf-probesize=5000000",
            "--stream-buffer-size=2M",            // 2MB stream read buffer
            "--hr-seek=yes",

            // ═══ PERFORMANCE: hardware decode + GPU render ═══
            "--hwdec=auto-safe",
            "--vo=gpu",
            "--gpu-api=opengl",
            "--video-sync=display-resample",
            "--interpolation=yes",
            "--tscale=oversample",
            "--framedrop=decoder",                // drop frames at decoder level if behind
            "--vd-lavc-threads=0",                // auto thread count for decoding

            // ═══ AUDIO: prevent audio desync ═══
            "--audio-stream-silence=yes",
            "--audio-wait-open=1",

            // ═══ ERROR RECOVERY: never stop, never close ═══
            "--reset-on-next-file=all",
            "--keep-open=yes",
            "--keep-open-pause=no",
            "--loop-file=inf",

            // ═══ UI ═══
            "--msg-level=all=error",              // only show real errors
            "--osd-level=0",
            "--force-window=yes",
            "--geometry=960x540",
            "--autofit-larger=90%x90%",
            "--ontop",
        ])
        .spawn()
        .map_err(|e| format!("Failed to start MPV: {}", e))?;

    *mpv.process.lock().unwrap() = Some(child);

    Ok(())
}

/// Switch MPV to a new URL without restarting (instant channel switch).
#[tauri::command]
pub async fn mpv_load(
    url: String,
    title: String,
    mpv: State<'_, MpvState>,
) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();

    // Check if MPV process is still alive
    let alive = {
        let mut proc = mpv.process.lock().unwrap();
        if let Some(ref mut p) = *proc {
            p.try_wait().ok().flatten().is_none()
        } else {
            false
        }
    };

    if !alive {
        return mpv_play(url, title, mpv).await;
    }

    // Use .ts for direct server connection (avoids CDN issues)
    let stream_url = if url.contains(".m3u8") {
        url.replace(".m3u8", ".ts")
    } else {
        url.clone()
    };

    // Try IPC — if it fails, MPV is stuck/orphaned, restart it
    match send_mpv_command(&socket, &["loadfile", &stream_url, "replace"]) {
        Ok(_) => {
            send_mpv_command(&socket, &["set_property", "title", &format!("JoTV - {}", title)]).ok();
            Ok(())
        }
        Err(_) => {
            // IPC failed — kill and restart
            mpv_play(url, title, mpv).await
        }
    }
}

#[tauri::command]
pub async fn mpv_pause(mpv: State<'_, MpvState>) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    send_mpv_command(&socket, &["cycle", "pause"])?;
    Ok(())
}

#[tauri::command]
pub async fn mpv_stop(mpv: State<'_, MpvState>) -> Result<(), String> {
    let mut proc = mpv.process.lock().unwrap();
    if let Some(mut p) = proc.take() {
        p.kill().ok();
        p.wait().ok();
    }
    // Cleanup socket
    let socket = mpv.socket_path.lock().unwrap().clone();
    std::fs::remove_file(&socket).ok();
    Ok(())
}

#[tauri::command]
pub async fn mpv_fullscreen(mpv: State<'_, MpvState>) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    send_mpv_command(&socket, &["cycle", "fullscreen"])?;
    Ok(())
}

#[tauri::command]
pub async fn mpv_volume(volume: i64, mpv: State<'_, MpvState>) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    send_mpv_command(&socket, &["set_property", "volume", &volume.to_string()])?;
    Ok(())
}

#[tauri::command]
pub async fn mpv_is_running(mpv: State<'_, MpvState>) -> Result<bool, String> {
    let process_alive = {
        let mut proc = mpv.process.lock().unwrap();
        if let Some(ref mut p) = *proc {
            p.try_wait().ok().flatten().is_none()
        } else {
            false
        }
    };

    if !process_alive {
        return Ok(false);
    }

    // Also verify IPC socket is responsive
    let socket = mpv.socket_path.lock().unwrap().clone();
    Ok(send_mpv_command(&socket, &["get_property", "pid"]).is_ok())
}
