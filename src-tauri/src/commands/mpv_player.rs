use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

pub struct MpvState {
    pub process: Mutex<Option<Child>>,
    pub socket_path: Mutex<String>,
    /// Native window MPV is embedded in (`--wid`), or None for its own window
    pub wid: Mutex<Option<u64>>,
    /// Live TV (badge, jump-to-live) vs movie/episode playback
    pub live: Mutex<bool>,
}

impl MpvState {
    pub fn new() -> Self {
        // MPV's control channel: a Unix socket, or a named pipe on Windows
        #[cfg(windows)]
        let socket = format!(r"\\.\pipe\jotv-mpv-{}", std::process::id());
        #[cfg(not(windows))]
        let socket = format!("/tmp/jotv-mpv-{}.sock", std::process::id());
        std::fs::remove_file(&socket).ok();
        // Sockets left by earlier runs whose app process is gone
        #[cfg(target_os = "linux")]
        if let Ok(entries) = std::fs::read_dir("/tmp") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let pid = name.strip_prefix("jotv-mpv-").and_then(|r| r.strip_suffix(".sock"));
                if let Some(pid) = pid {
                    if !std::path::Path::new(&format!("/proc/{}", pid)).exists() {
                        std::fs::remove_file(entry.path()).ok();
                    }
                }
            }
        }
        MpvState {
            process: Mutex::new(None),
            socket_path: Mutex::new(socket),
            wid: Mutex::new(None),
            live: Mutex::new(true),
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

/// Live playback status for the channel currently in MPV.
/// `state` is one of: "live", "buffering", "nosignal", "paused", "offline".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpvStatus {
    pub state: String,
    pub cache_seconds: f64,
    /// Seconds downloaded but not yet shown: how far playback trails the live edge
    pub behind_seconds: f64,
    pub volume: f64,
    pub muted: bool,
    pub time_pos: f64,
    /// 0 for live streams
    pub duration: f64,
    pub eof: bool,
}

/// Result of a lightweight connectivity test against a stream URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub ok: bool,
    pub status: Option<u16>,
    pub message: String,
}

#[cfg(unix)]
fn connect_ipc(path: &str) -> std::io::Result<std::os::unix::net::UnixStream> {
    let stream = std::os::unix::net::UnixStream::connect(path)?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(2))).ok();
    Ok(stream)
}

/// Windows: MPV's `--input-ipc-server` is a named pipe, opened like a file.
#[cfg(windows)]
fn connect_ipc(path: &str) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new().read(true).write(true).open(path)
}

fn send_mpv_command(socket_path: &str, command: &[&str]) -> Result<String, String> {
    let args: Vec<String> = command.iter().map(|s| format!("\"{}\"", s)).collect();
    let json = format!("{{ \"command\": [{}] }}\n", args.join(", "));

    let mut stream = connect_ipc(socket_path).map_err(|e| format!("Cannot connect to MPV: {}", e))?;

    stream.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to send command: {}", e))?;

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).ok();
    Ok(response)
}

/// Query a single MPV property over IPC and return its `data` value.
/// Returns None if MPV is unreachable or the property is unavailable.
fn get_mpv_property(socket_path: &str, name: &str) -> Option<serde_json::Value> {
    let raw = send_mpv_command(socket_path, &["get_property", name]).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(raw.trim()).ok()?;
    if parsed.get("error").and_then(|e| e.as_str()) != Some("success") {
        return None;
    }
    parsed.get("data").cloned()
}

/// MPV script that draws a YouTube-style "● LIVE" badge on the video. It lives
/// inside MPV because overlays created over IPC vanish when the connection closes.
const LIVE_BADGE_LUA: &str = r#"
local BEHIND_LIMIT = 10
local LIVE = mp.get_opt("jotv-live") ~= "no"
local badge = mp.create_osd_overlay("ass-events")
local vol = mp.create_osd_overlay("ass-events")
badge.res_y, vol.res_y = 720, 720
local badge_rect = nil      -- {x, y, w, h} in 720p overlay space, for clicks
local started_at, jumped = nil, false
-- Set by the app while it recovers a dropped stream
local reconnecting = false
local vol_timer = nil

local function pill(w, h)
  return string.format("m 10 0 l %d 0 b %d 0 %d 4 %d 10 l %d %d b %d %d %d %d %d %d l 10 %d b 4 %d 0 %d 0 %d l 0 10 b 0 4 4 0 10 0",
    w - 10, w - 4, w, w, w, h - 10, w, h - 4, w - 4, h, w - 10, h, h, h, h - 4, h - 10)
end

local function res_x()
  local ow, oh = mp.get_property_number("osd-width", 0), mp.get_property_number("osd-height", 0)
  if ow <= 0 or oh <= 0 then return nil end
  return math.floor(720 * ow / oh), 720 / oh
end

-- Newest downloaded moment minus what's on screen = how far behind live we are
local function behind()
  local edge, pos = mp.get_property_number("demuxer-cache-time"), mp.get_property_number("time-pos")
  if not edge or not pos then return 0 end
  return math.max(0, math.floor(edge - pos))
end

-- Jump to the newest part already downloaded (no new request to the server)
local function go_live()
  local edge = mp.get_property_number("demuxer-cache-time")
  if edge then mp.commandv("seek", tostring(edge - 2), "absolute") end
end

local function draw()
  if not LIVE then return end
  local rx, scale = res_x()
  if not rx then return end
  badge.res_x = rx

  local lag = behind()
  local playing = not mp.get_property_bool("core-idle", true)
  local caching = mp.get_property_bool("paused-for-cache", false)

  -- Servers often burst old video on connect; start at the live edge instead
  if playing and not caching and not jumped and started_at and mp.get_time() - started_at < 20 and lag > BEHIND_LIMIT then
    jumped = true
    go_live()
    return
  end

  local dot, label
  if reconnecting then
    dot, label = "&H2BB8F5&", "RECONNECTING"
  elseif caching then
    dot, label = "&H2BB8F5&", "BUFFERING"
  elseif playing then
    if lag > BEHIND_LIMIT then
      dot, label = "&H1F14E5&", string.format("BEHIND LIVE %ds  ›", lag)
    else
      dot, label = "&H5EC522&", "LIVE"
    end
  elseif mp.get_property_bool("pause", false) then
    dot, label = "&HC8C8C8&", "PAUSED"
  elseif reconnecting then
    dot, label = "&H2BB8F5&", "RECONNECTING"
  else
    badge:remove()
    badge_rect = nil
    return
  end

  -- Sit inside the picture, not on letterbox bars
  local ml = (mp.get_property_number("osd-dimensions/ml", 0) or 0) * scale
  local mb = (mp.get_property_number("osd-dimensions/mb", 0) or 0) * scale
  local w, h = 50 + #label * 13, 40
  local x, y = math.floor(24 + ml), math.floor(720 - 24 - h - mb)
  badge_rect = { x = x, y = y, w = w, h = h }
  badge.data = string.format(
    "{\\an7\\pos(%d,%d)\\bord0\\shad0\\1c&H000000&\\1a&H50&\\p1}%s{\\p0}\n" ..
    "{\\an4\\pos(%d,%d)\\bord0\\shad0\\fnInter\\b1\\fs30\\1c%s}●{\\1c&HFFFFFF&\\fs23\\fsp1.5} %s",
    x, y, pill(w, h), x + 14, y + h / 2, dot, label)
  badge:update()
end

local function show_volume()
  local rx = res_x()
  if not rx then return end
  vol.res_x = rx
  local muted = mp.get_property_bool("mute", false)
  local v = math.floor(mp.get_property_number("volume", 100) + 0.5)
  local label = muted and "MUTED" or string.format("VOLUME %d%%", v)
  local w, h = 40 + #label * 13, 50
  local x, y = math.floor(rx - 24 - w), 24
  local fill = muted and 0 or math.min(1, v / 100)
  vol.data = string.format(
    "{\\an7\\pos(%d,%d)\\bord0\\shad0\\1c&H000000&\\1a&H50&\\p1}%s{\\p0}\n" ..
    "{\\an4\\pos(%d,%d)\\bord0\\shad0\\fnInter\\b1\\fs23\\fsp1.5\\1c&HFFFFFF&}%s\n" ..
    "{\\an7\\pos(%d,%d)\\bord0\\shad0\\1c&HFFFFFF&\\p1}m 0 0 l %d 0 %d 3 0 3{\\p0}",
    x, y, pill(w, h), x + 20, y + 20, label, x + 20, y + h - 13, math.floor((w - 40) * fill), math.floor((w - 40) * fill))
  vol:update()
  if vol_timer then vol_timer:kill() end
  vol_timer = mp.add_timeout(1.5, function() vol:remove() end)
end

mp.register_event("file-loaded", function()
  started_at, jumped = mp.get_time(), false
end)

for _, p in ipairs({ "core-idle", "paused-for-cache", "pause", "osd-dimensions" }) do
  mp.observe_property(p, nil, draw)
end
-- The delay changes continuously; refresh it once a second
mp.add_periodic_timer(1, draw)

local first = true
local function volume_changed()
  if first then first = false return end   -- skip the initial value on startup
  show_volume()
end
mp.observe_property("volume", nil, volume_changed)
mp.observe_property("mute", nil, volume_changed)

mp.register_script_message("jotv-go-live", go_live)
mp.register_script_message("jotv-reconnecting", function(on)
  reconnecting = on == "yes"
  draw()
end)

-- Click the badge while behind to jump to live
mp.add_forced_key_binding("MBTN_LEFT", "jotv-click", function()
  if not badge_rect or behind() <= BEHIND_LIMIT then return end
  local _, scale = res_x()
  local mx, my = mp.get_mouse_pos()
  if not scale or not mx then return end
  mx, my = mx * scale, my * scale
  if mx >= badge_rect.x and mx <= badge_rect.x + badge_rect.w and my >= badge_rect.y and my <= badge_rect.y + badge_rect.h then
    go_live()
  end
end)

-- Mouse wheel over the video changes the volume
mp.add_forced_key_binding("WHEEL_UP", "jotv-vol-up", function() mp.commandv("add", "volume", "5") end)
mp.add_forced_key_binding("WHEEL_DOWN", "jotv-vol-down", function() mp.commandv("add", "volume", "-5") end)
"#;

/// Writes the badge script into the app's own data dir. Not the shared temp dir:
/// a predictable name there could be pre-created by another user, and MPV would
/// run whatever Lua it found.
fn live_badge_script(app: &AppHandle) -> Option<String> {
    let dir = app.path().app_local_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("live-badge.lua");
    std::fs::write(&path, LIVE_BADGE_LUA).ok()?;
    Some(path.to_string_lossy().into_owned())
}

/// Start MPV with a stream URL, controlled over IPC. With `wid` it renders into
/// that native window inside the app; otherwise it opens its own window.
#[tauri::command]
pub async fn mpv_play(
    app: AppHandle,
    url: String,
    title: String,
    wid: Option<u64>,
    volume: Option<i64>,
    muted: Option<bool>,
    live: Option<bool>,
    start: Option<f64>,
    alang: Option<String>,
    slang: Option<String>,
    mpv: State<'_, MpvState>,
) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    let live = live.unwrap_or(true);

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

    // Use the URL exactly as stored. Auto-converting .m3u8→.ts breaks servers
    // that only serve HLS, and triggers extra requests that contribute to rate limits.
    let stream_url = url.clone();

    // In-app: no window chrome, no mpv key/mouse bindings (the app owns input)
    let wid_arg = wid.map(|id| format!("--wid={}", id));
    let window_args: Vec<&str> = match &wid_arg {
        Some(arg) => vec![
            arg,
            "--force-window=yes",
            "--input-default-bindings=no",
            "--input-vo-keyboard=no",
            "--cursor-autohide=no",
            // Movies get MPV's seek bar, hidden until the app goes fullscreen
            if live { "--osc=no" } else { "--osc=yes" },
        ],
        None => vec![
            "--force-window=yes",
            "--geometry=960x540",
            "--autofit-larger=90%x90%",
            "--ontop",
        ],
    };

    let badge_arg = live_badge_script(&app).map(|path| format!("--script={}", path));

    // Launch MPV — ultimate IPTV settings for zero cuts
    let mut cmd = Command::new("mpv");
    // Die with the app: otherwise a crash, kill or dev rebuild leaves MPV
    // decoding (and playing audio) with no window to show it in.
    #[cfg(target_os = "linux")]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    }
    if let Some(arg) = &badge_arg {
        cmd.arg(arg);
    }
    if let Some(v) = volume {
        cmd.arg(format!("--volume={}", v.clamp(0, 130)));
    }
    if muted == Some(true) {
        cmd.arg("--mute=yes");
    }
    cmd.arg(format!(
        "--script-opts=jotv-live={}{}",
        if live { "yes" } else { "no" },
        if !live && wid.is_some() { ",osc-visibility=never" } else { "" },
    ));
    if let Some(secs) = start.filter(|s| *s > 0.0) {
        cmd.arg(format!("--start={:.1}", secs));
    }
    // Preferred audio/subtitle languages (remembered from the track picker)
    let lang_ok = |l: &&String| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == ',' || c == '-');
    if let Some(l) = alang.as_ref().filter(lang_ok) {
        cmd.arg(format!("--alang={}", l));
    }
    match slang.as_deref() {
        Some("no") => { cmd.arg("--sid=no"); }
        Some(_) => if let Some(l) = slang.as_ref().filter(lang_ok) { cmd.arg(format!("--slang={}", l)); },
        None => {}
    }
    if wid.is_some() {
        // --wid is X11-only; on a Wayland session MPV would otherwise pick its
        // Wayland output, ignore --wid and open its own window.
        cmd.env_remove("WAYLAND_DISPLAY");
    }
    let child = cmd
        .args([
            &stream_url,
            &format!("--input-ipc-server={}", socket),
            &format!("--title=JoTV - {}", title),

            // ═══ CORE: raw stream, no youtube-dl ═══
            "--ytdl=no",
            "--hls-bitrate=max",
            // Start HLS at the newest segment (default is 3 back: ~20-30s behind live)
            "--demuxer-lavf-o-append=live_start_index=-1",

            // ═══ CACHE: enough to survive jitter, not so much it desyncs ═══
            // 5-min cache caused initial-load lag and made stream-end/seek behave
            // weirdly. 30s is plenty for IPTV jitter.
            "--cache=yes",
            "--demuxer-max-bytes=64M",
            "--demuxer-max-back-bytes=32M",
            "--cache-secs=30",
            "--demuxer-readahead-secs=20",
            "--cache-pause-initial=yes",
            "--cache-pause-wait=1",              // resume after 1s buffered (was 3)

            // ═══ NETWORK: reconnect on transient failures only ═══
            // Don't reconnect on 4xx — Xtream uses 461/463 for IP rate-limit;
            // hammering only extends the block. Limit retries so MPV gives up
            // and surfaces the error instead of pretending to play forever.
            "--network-timeout=60",
            "--stream-lavf-o-append=reconnect=1",
            "--stream-lavf-o-append=reconnect_streamed=1",
            "--stream-lavf-o-append=reconnect_on_network_error=1",
            "--stream-lavf-o-append=reconnect_on_http_error=5xx",
            "--stream-lavf-o-append=reconnect_delay_max=10",
            "--stream-lavf-o-append=reconnect_max_retries=8",
            "--stream-lavf-o-append=timeout=15000000",
            "--stream-lavf-o-append=rw_timeout=15000000",

            // ═══ DEMUXER: handle broken/stuttery streams ═══
            "--demuxer=lavf",
            "--demuxer-lavf-analyzeduration=10",
            "--demuxer-lavf-probesize=5000000",
            "--stream-buffer-size=2M",            // 2MB stream read buffer
            "--hr-seek=yes",
            "--demuxer-seekable-cache=yes",      // lets "Go live" jump within the cache

            // ═══ PERFORMANCE: hardware decode + simple sync ═══
            // Use default audio-based video sync — display-resample +
            // interpolation can stutter on variable-frame-rate IPTV.
            "--hwdec=auto-safe",
            "--vo=gpu",
            "--gpu-api=opengl",
            // Don't wait for the compositor to show each frame: Hyprland (and
            // other Wayland compositors) never show a window on a hidden
            // workspace, so a vsync'd swap blocks and the stream stalls behind
            // live while you're in another app
            "--opengl-swapinterval=0",
            "--framedrop=decoder",
            "--vd-lavc-threads=0",

            // ═══ ERROR RECOVERY ═══
            // No --loop-file for live (caused weird seek behavior on stream end).
            // No --audio-stream-silence / --audio-wait-open (those introduced
            // initial audio offset on some devices).
            // Only un-pause on channel change; "all" also reset the volume and
            // mute the user picked, back to their startup values
            "--reset-on-next-file=pause",
            "--keep-open=yes",
            "--keep-open-pause=no",

            // ═══ UI ═══
            "--msg-level=all=error",              // only show real errors
            "--osd-level=0",
        ])
        .args(&window_args)
        .spawn()
        .map_err(|e| format!("Failed to start MPV: {}", e))?;

    *mpv.process.lock().unwrap() = Some(child);
    *mpv.wid.lock().unwrap() = wid;
    *mpv.live.lock().unwrap() = live;

    Ok(())
}

/// Switch MPV to a new URL without restarting (instant channel switch).
#[tauri::command]
pub async fn mpv_load(
    app: AppHandle,
    url: String,
    title: String,
    wid: Option<u64>,
    volume: Option<i64>,
    muted: Option<bool>,
    live: Option<bool>,
    start: Option<f64>,
    alang: Option<String>,
    slang: Option<String>,
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

    // Switching in-app/window or live/movie mode needs a fresh MPV
    if !alive || *mpv.wid.lock().unwrap() != wid || *mpv.live.lock().unwrap() != live.unwrap_or(true) {
        return mpv_play(app, url, title, wid, volume, muted, live, start, alang, slang, mpv).await;
    }

    // Use .ts for direct server connection (avoids CDN issues)
    let stream_url = if url.contains(".m3u8") {
        url.replace(".m3u8", ".ts")
    } else {
        url.clone()
    };

    // Per-file start position (resume) rides along with loadfile
    let start_opt = start.filter(|s| *s > 0.0).map(|s| format!("start={:.1}", s));
    let load: Vec<&str> = match &start_opt {
        Some(opt) => vec!["loadfile", &stream_url, "replace", "-1", opt],
        None => vec!["loadfile", &stream_url, "replace"],
    };

    // Try IPC — if it fails, MPV is stuck/orphaned, restart it
    match send_mpv_command(&socket, &load) {
        Ok(_) => {
            send_mpv_command(&socket, &["set_property", "title", &format!("JoTV - {}", title)]).ok();
            Ok(())
        }
        Err(_) => {
            // IPC failed — kill and restart
            mpv_play(app, url, title, wid, volume, muted, live, start, alang, slang, mpv).await
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
    *mpv.wid.lock().unwrap() = None;
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
    send_mpv_command(&socket, &["set", "volume", &volume.to_string()])?;
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

/// Report whether the currently-loaded channel is actually connected and decoding.
/// The frontend polls this while a live channel is playing to drive a status badge.
#[tauri::command]
pub async fn mpv_status(mpv: State<'_, MpvState>) -> Result<MpvStatus, String> {
    let process_alive = {
        let mut proc = mpv.process.lock().unwrap();
        if let Some(ref mut p) = *proc {
            p.try_wait().ok().flatten().is_none()
        } else {
            false
        }
    };

    let offline = MpvStatus {
        state: "offline".into(),
        cache_seconds: 0.0,
        behind_seconds: 0.0,
        volume: 100.0,
        muted: false,
        time_pos: 0.0,
        duration: 0.0,
        eof: false,
    };
    if !process_alive {
        return Ok(offline);
    }

    let socket = mpv.socket_path.lock().unwrap().clone();

    // If IPC itself is unreachable, MPV is stuck/gone → offline.
    let idle_active = match get_mpv_property(&socket, "idle-active") {
        Some(v) => v.as_bool().unwrap_or(false),
        None => return Ok(offline),
    };

    let paused_for_cache = get_mpv_property(&socket, "paused-for-cache")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let core_idle = get_mpv_property(&socket, "core-idle")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let user_paused = get_mpv_property(&socket, "pause")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let cache_seconds = get_mpv_property(&socket, "demuxer-cache-time")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let state = if idle_active {
        // Nothing loaded, or the stream ended.
        "nosignal"
    } else if paused_for_cache {
        // Stalled waiting on the network to refill the cache.
        "buffering"
    } else if core_idle {
        if user_paused { "paused" } else { "nosignal" }
    } else {
        // Actively decoding frames.
        "live"
    };

    // Newest downloaded moment minus play position
    let time_pos = get_mpv_property(&socket, "time-pos").and_then(|v| v.as_f64());
    let behind_seconds = match time_pos {
        Some(pos) if cache_seconds > 0.0 => (cache_seconds - pos).max(0.0),
        _ => 0.0,
    };

    let volume = get_mpv_property(&socket, "volume").and_then(|v| v.as_f64()).unwrap_or(100.0);
    let muted = get_mpv_property(&socket, "mute").and_then(|v| v.as_bool()).unwrap_or(false);

    let duration = get_mpv_property(&socket, "duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let eof = get_mpv_property(&socket, "eof-reached").and_then(|v| v.as_bool()).unwrap_or(false);

    Ok(MpvStatus {
        state: state.into(),
        cache_seconds,
        behind_seconds,
        volume,
        muted,
        time_pos: time_pos.unwrap_or(0.0),
        duration,
        eof,
    })
}

/// Lightweight connectivity test for a stream URL, independent of MPV so it never
/// disrupts current playback. Used by the right-click "test channel" action.
#[tauri::command]
pub async fn probe_stream(url: String) -> Result<ProbeResult, String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) JoTV/0.1")
        .connect_timeout(std::time::Duration::from_secs(6))
        .timeout(std::time::Duration::from_secs(9))
        .build()
        .map_err(|e| format!("client: {}", e))?;

    // Range keeps servers that would otherwise stream the whole file honest,
    // and gives us headers fast. Servers that ignore it still just send data.
    let resp = match client
        .get(&url)
        .header(reqwest::header::RANGE, "bytes=0-1023")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let message = if e.is_timeout() {
                "Timed out".to_string()
            } else if e.is_connect() {
                "Cannot connect".to_string()
            } else {
                format!("{}", e)
            };
            return Ok(ProbeResult { ok: false, status: None, message });
        }
    };

    let status = resp.status();
    let code = status.as_u16();

    if !status.is_success() {
        return Ok(ProbeResult {
            ok: false,
            status: Some(code),
            message: format!("HTTP {}", code),
        });
    }

    // Confirm bytes actually flow — a 200 with no readable body is a dead stream.
    // Read only the FIRST chunk: a live .ts body never ends, so we must not wait
    // for the whole thing (`bytes()` would block until the request timeout).
    let mut resp = resp;
    match resp.chunk().await {
        Ok(Some(bytes)) if !bytes.is_empty() => Ok(ProbeResult {
            ok: true,
            status: Some(code),
            message: "OK".into(),
        }),
        Ok(_) => Ok(ProbeResult {
            ok: false,
            status: Some(code),
            message: "No data".into(),
        }),
        Err(e) => Ok(ProbeResult {
            ok: false,
            status: Some(code),
            message: format!("{}", e),
        }),
    }
}

/// Jump to the live edge of what MPV has already downloaded.
#[tauri::command]
pub async fn mpv_go_live(mpv: State<'_, MpvState>) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    send_mpv_command(&socket, &["script-message", "jotv-go-live"])?;
    Ok(())
}

#[tauri::command]
pub async fn mpv_toggle_mute(mpv: State<'_, MpvState>) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    send_mpv_command(&socket, &["cycle", "mute"])?;
    Ok(())
}

/// Seek by `seconds` (relative) or to `seconds` (absolute).
#[tauri::command]
pub async fn mpv_seek(seconds: f64, absolute: bool, mpv: State<'_, MpvState>) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    let amount = format!("{:.2}", seconds);
    send_mpv_command(&socket, &["seek", &amount, if absolute { "absolute" } else { "relative" }])?;
    Ok(())
}

/// Shows MPV's own seek bar on hover (used while the in-app player is fullscreen).
#[tauri::command]
pub async fn mpv_osc(visible: bool, mpv: State<'_, MpvState>) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    send_mpv_command(&socket, &["script-message", "osc-visibility", if visible { "auto" } else { "never" }, "no-osd"])?;
    Ok(())
}

/// An audio or subtitle track of the current file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpvTrack {
    pub id: i64,
    /// "audio" or "sub"
    pub kind: String,
    pub lang: Option<String>,
    pub title: Option<String>,
    pub codec: Option<String>,
    pub channels: Option<i64>,
    pub selected: bool,
    pub external: bool,
}

/// Audio and subtitle tracks of what's playing.
#[tauri::command]
pub async fn mpv_tracks(mpv: State<'_, MpvState>) -> Result<Vec<MpvTrack>, String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    let list = get_mpv_property(&socket, "track-list").ok_or("MPV isn't playing")?;
    let tracks = list
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|t| {
                    let kind = t.get("type")?.as_str()?;
                    if kind != "audio" && kind != "sub" {
                        return None;
                    }
                    let text = |k: &str| t.get(k).and_then(|v| v.as_str()).map(String::from);
                    Some(MpvTrack {
                        id: t.get("id")?.as_i64()?,
                        kind: kind.to_string(),
                        lang: text("lang"),
                        title: text("title"),
                        codec: text("codec"),
                        channels: t.get("demux-channel-count").and_then(|v| v.as_i64()),
                        selected: t.get("selected").and_then(|v| v.as_bool()).unwrap_or(false),
                        external: t.get("external").and_then(|v| v.as_bool()).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(tracks)
}

/// Switch audio/subtitle track (`id` None = subtitles off). Also makes that
/// language the preference for the next file in this MPV.
#[tauri::command]
pub async fn mpv_set_track(
    kind: String,
    id: Option<i64>,
    lang: Option<String>,
    mpv: State<'_, MpvState>,
) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    let (prop, pref) = match kind.as_str() {
        "audio" => ("aid", "alang"),
        "sub" => ("sid", "slang"),
        _ => return Err(format!("Unknown track type: {}", kind)),
    };
    let value = id.map(|i| i.to_string()).unwrap_or_else(|| "no".into());
    send_mpv_command(&socket, &["set", prop, &value])?;
    if let Some(l) = lang.filter(|l| !l.is_empty()) {
        send_mpv_command(&socket, &["set", pref, &l]).ok();
    }
    Ok(())
}

/// Reconnects to `url` as given (no .m3u8→.ts rewrite), at the live edge.
/// Used by the app's stream recovery.
#[tauri::command]
pub async fn mpv_reconnect(url: String, mpv: State<'_, MpvState>) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    send_mpv_command(&socket, &["loadfile", &url, "replace"])?;
    Ok(())
}

/// Live buffering: "stable" waits for a bigger buffer before resuming after a
/// stall (fewer cut/resume cycles on weak streams); "fast" resumes quickly.
#[tauri::command]
pub async fn mpv_set_buffer(mode: String, mpv: State<'_, MpvState>) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    let (wait, readahead) = match mode.as_str() {
        "stable" => ("5", "40"),
        _ => ("1", "20"),
    };
    send_mpv_command(&socket, &["set", "cache-pause-wait", wait])?;
    send_mpv_command(&socket, &["set", "demuxer-readahead-secs", readahead])?;
    Ok(())
}

/// Shows "RECONNECTING" on the video while the app recovers the stream.
#[tauri::command]
pub async fn mpv_badge_reconnecting(on: bool, mpv: State<'_, MpvState>) -> Result<(), String> {
    let socket = mpv.socket_path.lock().unwrap().clone();
    send_mpv_command(&socket, &["script-message", "jotv-reconnecting", if on { "yes" } else { "no" }])?;
    Ok(())
}
