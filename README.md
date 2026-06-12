# Vesper
A keyboard-first local media gallery for Linux.

Vesper indexes your local images and videos, automatically deriving tags from your existing folders. 100% offline, read-only, and blazingly fast.

### Features
- **Zero-config:** Folder names become tags.
- **Fast:** Smoothly scroll 50,000+ files. 
- **Keyboard-Driven:** Navigate and view media without a mouse.
- **Native:** Built with Rust, GTK4 & Libadwaita.

### Install
Download `vesper.flatpak` from the [Releases](../../releases) page.
```bash
flatpak install vesper.flatpak
```

### Build from source
Ensure you have Rust and Cargo installed.
```bash
# Fedora
sudo dnf install gtk4-devel libadwaita-devel pkg-config sqlite-devel

# Ubuntu/Debian 
sudo apt install libgtk-4-dev libadwaita-1-dev pkg-config libsqlite3-dev

cargo run --release
```
