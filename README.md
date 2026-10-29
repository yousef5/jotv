<p align="center">
  <img src="src-tauri/icons/icon.png" width="120" alt="JoTV Logo">
</p>

<h1 align="center">JoTV</h1>

<p align="center">
  <strong>Modern IPTV Player for Desktop</strong><br>
  Fast, beautiful, and powerful. Built with Tauri, SvelteKit, and MPV.
</p>

<p align="center">
  <a href="#features">Features</a> &bull;
  <a href="#installation">Installation</a> &bull;
  <a href="#building-from-source">Build</a> &bull;
  <a href="#tech-stack">Tech Stack</a> &bull;
  <a href="#license">License</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20Windows%20%7C%20macOS-blue" alt="Platform">
  <img src="https://img.shields.io/badge/version-0.3.1-green" alt="Version">
  <img src="https://img.shields.io/badge/license-MIT-orange" alt="License">
  <img src="https://img.shields.io/badge/tauri-v2-purple" alt="Tauri v2">
</p>

---

## Features

### Playlist Management
- **Xtream Codes API** support with full account info (status, expiry, connections)
- **M3U/M3U8** playlist import from URL or local file
- **Parallel import** - downloads all channel data simultaneously for maximum speed
- **Refresh** playlists to get the latest channels from the server
- **66,000+ channels** handled smoothly with paginated loading

### Live TV
- **MPV-powered playback** - rock-solid streaming with zero cuts
  - 300MB buffer cache, 5 minutes read-ahead
  - Auto-reconnect on network drops (unlimited retries)
  - Hardware-accelerated decoding (VA-API, VDPAU)
  - Instant channel switching via IPC
- **3-column layout**: Playlists > Groups > Channels
- **Smart search** with Arabic text normalization and fuzzy matching (Ctrl+K)
- Content organized by **Live / VOD / Series** tabs

### VOD & Series
- **Movie posters** in thumbnail grid view
- **Series detail page** with backdrop, poster, plot, cast, season tabs
- **Episode list** with one-click play and download
- **Recently Added** section showing newest content from the server
- Playback via **MPV** (direct `.ts` streams) or **VLC** for file formats

### Favorites
- Organize favorites into **categories**: News, Sports, Movies, Kids, Music, Documentary, Entertainment
- **Quick add** with category picker from any channel list
- **3-column favorites page** matching the playlists layout
- MPV playback directly from favorites

### Downloads
- **Real download engine** with streaming writes (not buffered in memory)
- **Pause/Resume** support with HTTP Range headers
- **Max speed** - TCP_NODELAY, 1MB write buffer, 2-hour timeout
- Smart filename: `Movie Name.mp4`, `Series Name - S01E03.mkv`
- Automatic URL format fallback for series episodes

### Dashboard
- **Stats overview**: total channels, favorites, playlists, downloads
- **Recently watched** channels
- **New Movies & Series** - horizontal scrollable poster cards from server
- **Favorite chips** for quick access
- **Playlist cards** with account status, expiry countdown, connection info

### Search
- **Fuzzy matching** - finds "beIN" even if you type "bein" or "bin"
- **Arabic normalization** - normalizes alef variants, taa marbuta, diacritics
- **Ranked results** - exact match > starts-with > contains > group match
- **Ctrl+K** keyboard shortcut to focus search
- Searches by **channel name and group name**

---

## Installation

Get the latest version from the **[Releases page](https://github.com/yousef5/jotv/releases/latest)**. Pick the file for your system:

| System | File | How |
|---|---|---|
| **Windows 10/11** | `JoTV_<version>_x64-setup.exe` (or `.msi`) | Double-click, follow the installer. Everything JoTV needs (mpv, ffmpeg, yt-dlp) is inside. |
| **CachyOS / Arch / Manjaro** | `jotv-<version>-1-x86_64.pkg.tar.zst` | `sudo pacman -U jotv-*.pkg.tar.zst` |
| **Ubuntu / Debian / Mint** | `JoTV_<version>_amd64.deb` | `sudo apt install ./JoTV_*_amd64.deb` |
| **Fedora / RHEL / openSUSE** | `JoTV-<version>-1.x86_64.rpm` | `sudo dnf install ./JoTV-*.rpm` |
| **Any Linux** | `JoTV_<version>_amd64.AppImage` | `chmod +x JoTV_*.AppImage && ./JoTV_*.AppImage` |
| **macOS** | `JoTV_<version>_universal.dmg` | Open it, drag JoTV to Applications. The first time, right-click the app and choose **Open** (it is not notarized). |

### What each system needs

- **Windows**: nothing else.
- **Linux**: the package manager installs `mpv` and `ffmpeg` for you. `yt-dlp` is optional (Reels and Social downloads): `sudo pacman -S yt-dlp` / `sudo apt install yt-dlp` / `sudo dnf install yt-dlp`. On **Wayland** JoTV runs through XWayland (`xorg-xwayland`), which most desktops already have.
- **macOS**: `brew install mpv ffmpeg yt-dlp`.
- JoTV checks all of this for you: on first start (and in **Settings → System**) it shows what is missing and the exact command to install it for your distribution.

### First start

1. Open JoTV and choose **Add playlist**.
2. Pick **Xtream Codes** (server URL + username + password from your provider) or **M3U** (a link or a file).
3. Wait for the first import; big playlists (60,000+ items) take a few seconds.
4. Optional: in **Settings** add your free [TMDB](https://www.themoviedb.org/settings/api) key for posters, backdrops and cast.

### Updating

Install the new file over the old one (same command as above). Your playlists, favorites and history are kept.

### Uninstalling

Windows: *Settings → Apps → JoTV → Uninstall*. Arch: `sudo pacman -R jotv`. Debian/Ubuntu: `sudo apt remove jotv`. Fedora: `sudo dnf remove jotv`.

### Android

JoTV also has an Android version (phones and tablets): **JoTV Mobile**, in the private repository `yousef5/jotv-mobile`.

---

## Building from Source

### Prerequisites

- [Node.js](https://nodejs.org/) >= 18
- [pnpm](https://pnpm.io/) >= 8
- [Rust](https://rustup.rs/) >= 1.77
- [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)

#### System Dependencies

**Arch / CachyOS:**
```bash
sudo pacman -S webkit2gtk-4.1 base-devel curl wget openssl gtk3 librsvg mpv
```

**Ubuntu / Debian:**
```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev mpv
```

### Build Steps

```bash
# Clone the repository
git clone https://github.com/yousef5/jotv.git
cd jotv

# Install frontend dependencies
pnpm install

# Run in development mode
pnpm tauri dev

# Build for production
pnpm tauri build
```

Production builds are output to `src-tauri/target/release/bundle/`.

---

## Tech Stack

| Layer | Technology |
|-------|-----------|
| **Framework** | [Tauri v2](https://v2.tauri.app/) |
| **Frontend** | [SvelteKit](https://kit.svelte.dev/) + [Svelte 5](https://svelte.dev/) Runes |
| **Styling** | CSS Variables + [Tailwind CSS v3](https://tailwindcss.com/) |
| **Backend** | [Rust](https://www.rust-lang.org/) |
| **Database** | [SQLite](https://www.sqlite.org/) (rusqlite) |
| **Live Video** | [MPV](https://mpv.io/) via JSON IPC |
| **VOD Video** | [HLS.js](https://github.com/video-dev/hls.js) / [mpegts.js](https://github.com/nicknisi/mpegts.js) |
| **HTTP** | [reqwest](https://docs.rs/reqwest/) |
| **Async Runtime** | [Tokio](https://tokio.rs/) |

### Project Structure

```
jotv/
  src/                          # SvelteKit frontend
    routes/                     # Pages (Dashboard, Playlists, Favorites, Downloads, Settings)
    lib/
      components/               # Svelte components (Player, Sidebar, StatCard, etc.)
      stores/                   # Reactive stores (playlists, favorites, player state)
      tauri.ts                  # TypeScript bindings for all Rust commands
  src-tauri/                    # Rust backend
    src/
      commands/                 # Tauri commands
        channels.rs             # Channel queries, search, groups
        playlists.rs            # Import, refresh, export
        favorites.rs            # Categorized favorites
        downloads.rs            # Download engine with pause/resume
        mpv_player.rs           # MPV process management via IPC
        player.rs               # External player detection
      db/                       # Database (schema, models, migrations)
      parsers/                  # M3U parser + Xtream API parser
    migrations/                 # SQLite schema
```

---

## MPV Streaming Configuration

JoTV uses MPV as the video engine for live streams with these optimizations:

| Setting | Value | Purpose |
|---------|-------|---------|
| Cache | 300MB | Large demuxer buffer |
| Read-ahead | 2 minutes | Pre-fetches stream data |
| Cache duration | 5 minutes | Total cached ahead |
| Reconnect | Unlimited | Auto-reconnect on any failure |
| Reconnect timeout | 1 hour | Total retry time before giving up |
| Hardware decode | auto-safe | VA-API / VDPAU |
| Stream format | `.ts` direct | Bypasses CDN for reliability |
| Frame sync | display-resample | Smooth frame display |
| Audio gaps | Silence fill | No audio cuts on network hiccups |

---

## Contributing

Contributions are welcome!

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Commit your changes
4. Push to the branch (`git push origin feature/my-feature`)
5. Open a Pull Request

---

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

---

<p align="center">
  Built with Rust, Svelte, and MPV<br>
  <strong>JoTV</strong> - Your IPTV, Your Way
</p>
