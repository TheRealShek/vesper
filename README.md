<div align="center">
  <h1>Vesper</h1>
  
  <p>
    <img src="https://img.shields.io/badge/language-Rust-orange.svg?style=flat-square" alt="Rust" />
    <img src="https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square" alt="License" />
    <img src="https://img.shields.io/badge/version-0.1.0-brightgreen.svg?style=flat-square" alt="Version" />
    <img src="https://img.shields.io/github/actions/workflow/status/TheRealShek/vesper/rust.yml?branch=main&style=flat-square" alt="Build" />
    <img src="https://img.shields.io/badge/Flathub-Available-4A90E2.svg?logo=flathub&style=flat-square" alt="Flathub" />
  </p>

  <h3>Fast, offline media browsing for Linux.</h3>


</div>

<br />

* **Zero-config organization:** Your folder structure is the tagging system.
* **Blazingly fast:** Virtualized grid smoothly handles 50,000+ files without stutter.
* **Read-only by design:** Browses your existing files; never edits, moves, or deletes them.
* **Keyboard-driven:** Navigate your entire media collection without touching a mouse.

## Features
- Unified visual grid for images and videos across multiple source directories.
- Real-time filtering by folder-derived tags and fast text search.
- Native, borderless media viewer with zoom, pan, and looping video playback.
- Additive selection and multi-selection with persistent session state.
- Gracefully ignores `.git`, `node_modules`, and respects local `.galleryignore` files.
- Polished Linux desktop integration built on GTK4 and Libadwaita.

## Install

### Flatpak (Recommended)
Download `vesper.flatpak` from the [Releases](../../releases) page.
```bash
flatpak install vesper.flatpak
```

### Build from Source
Ensure you have Rust and Cargo installed, along with GTK4 and SQLite dependencies.
```bash
# Fedora
sudo dnf install gtk4-devel libadwaita-devel pkg-config sqlite-devel

# Ubuntu/Debian
sudo apt install libgtk-4-dev libadwaita-1-dev pkg-config libsqlite3-dev

git clone https://github.com/TheRealShek/vesper.git
cd vesper
cargo run --release
```

## Usage

1. Open Vesper for the first time.
2. Click **Add Source Directory** and select a folder containing your media.
3. Vesper instantly begins indexing in the background. Your folder names become your tags.
4. Use the sidebar to filter by tags, or the top bar to search and sort.
5. Single-click any image or video to open the native viewer.

## Keyboard Shortcuts

| Shortcut | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Move focus between UI regions |
| `Arrow keys` | Navigate grid or change file in viewer |
| `Enter` | Open selected cell in viewer |
| `Escape` | Close viewer, exit selection, or clear search |
| `Ctrl+B` | Toggle sidebar |
| `Ctrl+A` | Select all in current view |
| `Ctrl+Click` / `Shift+Click` | Add cell to selection / Range select |
| `F` | Toggle fullscreen (in viewer) |
| `I` | Toggle info panel (in viewer) |
| `Space` | Toggle video play/pause (in viewer) |

## Architecture / Tech Stack

- **Language:** Rust
- **UI Framework:** GTK4 + Libadwaita (via `gtk4-rs` and `libadwaita-rs`)
- **Database:** SQLite (via `rusqlite`) for fast metadata querying and persistence.
- **Concurrency:** `tokio` for non-blocking I/O, database queries, and thumbnail generation.
- **Filesystem:** `notify` for real-time filesystem watching.

## Status / Roadmap

**Status:** v1 Locked.

Vesper v1 is focused exclusively on creating a smooth, read-only browsing experience. There are no plans to introduce file editing, cloud sync, rating systems, or facial recognition. The application is feature-complete according to its primary scope.
