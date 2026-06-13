# ToBe

A privacy-focused YouTube client for desktop. Browse, search, and watch YouTube videos without ads or tracking — no Google account needed.

## Features

- **Subscription feed** — latest videos from all your subscribed channels in one place
- **Channel browsing** — view channel videos, live streams, and info
- **Video playback** — with resolution switching and autoplay
- **Search & subscribe** — find and follow channels
- **Privacy** — uses InnerTube API directly (no YouTube OAuth, no tracking scripts)
- **Light/dark theme**

## How it works

ToBe connects to YouTube through InnerTube (the same API YouTube's own web client uses) and optionally through Invidious instances. No Google API keys, no ads, no tracking.

Video data is cached locally. Channel avatars and metadata are fetched on demand and stored for offline display.

## Prerequisites

- **Rust** (install via [rustup](https://rustup.rs/) or your package manager)
- **Tauri v2 system dependencies** — on Debian/Ubuntu:

```
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Other distros: see [Tauri's official guide](https://v2.tauri.app/start/prerequisites/).

- **yt-dlp** (recommended for video streaming fallback):

```
sudo apt install yt-dlp
```

## Install & Build

```
git clone https://github.com/sergiugit/tobe.git
cd tobe/src-tauri
cargo build --release
cp target/release/tobe ../tobe
```

## Run

From the project root:

```
cd tobe
./tobe
```

To see debug output:

```
./tobe 2>&1 | tee /tmp/tobe.log
```

## Tech stack

- **Frontend** — Vanilla JavaScript, ES modules
- **Backend** — Rust, Tauri v2
- **API** — YouTube InnerTube, yt-dlp fallback
- **Video sources** — Invidious-compatible instances (configurable)

## License

MIT
