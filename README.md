# Vesper

Vesper is a keyboard-first local media gallery for Linux. It indexes local images and videos, deriving tags dynamically from your directory structure. 

## Features

- **Virtualized Grid**: Renders large media collections (up to 50,000+ files) without performance degradation.
- **Dynamic Tagging**: Automatically derives tags from folder hierarchies (read-only; does not modify your filesystem).
- **Keyboard-first Navigation**: Navigate, search, select, and view media using keyboard shortcuts.
- **Overlay Viewer**: High-performance image zoom and pan; video playback with auto-looping.

## Stack

- **Language**: Rust
- **GUI Toolkit**: GTK4 & libadwaita
- **Database**: SQLite (via `rusqlite`)

## Building and Running

### Prerequisites

You need Rust, Cargo, GTK4, and libadwaita development libraries installed on your Linux system.
On Fedora:
```bash
sudo dnf install gtk4-devel libadwaita-devel pkg-config sqlite-devel
```
On Ubuntu/Debian:
```bash
sudo apt install libgtk-4-dev libadwaita-1-dev pkg-config libsqlite3-dev
```

### Run

```bash
cargo run
```

## Project Structure

- `src/main.rs`: Entry point and application startup.
- `src/ui/`: GTK4 layout, custom widgets (virtualized grid, search, viewer).
- `src/db/`: Local SQLite database for caching and indexing state.
- `src/index/`: Filesystem crawler and ignore engine.
- `docs/`: Design documents and specifications.
