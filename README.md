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
  <img src="https://img.shields.io/badge/version-0.1.0-green" alt="Version">
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

### Linux (CachyOS / Arch)

```bash
# Download the .pkg.tar.zst from Releases page
sudo pacman -U jotv-0.1.0-1-x86_64.pkg.tar.zst
```

### Ubuntu / Debian

```bash
# Download the .deb from Releases page
sudo dpkg -i jotv_0.1.0_amd64.deb
sudo apt-get install -f  # install dependencies if needed
```

### Windows

Download `JoTV_0.1.0_x64-setup.exe` from the [Releases](../../releases) page.

### macOS

Download `JoTV_0.1.0_universal.dmg` from the [Releases](../../releases) page.

### Requirements

- **MPV** is required for live TV playback
  - **Arch/CachyOS**: `sudo pacman -S mpv`
  - **Ubuntu/Debian**: `sudo apt install mpv`
  - **Windows**: Download from [mpv.io](https://mpv.io/installation/)
  - **macOS**: `brew install mpv`
- **VLC** (optional) - used as fallback for VOD/series file playback

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
