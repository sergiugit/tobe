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

## Build from source

Requires Rust and the Tauri v2 prerequisites for your platform:

```
cd src-tauri
cargo build --release
cp target/release/tobe ../tobe
```

Run from the project root:

```
./tobe
```

## Tech stack

- **Frontend** — Vanilla JavaScript, ES modules
- **Backend** — Rust, Tauri v2
- **API** — YouTube InnerTube, yt-dlp fallback
- **Video sources** — Invidious-compatible instances (configurable)

## License

MIT
