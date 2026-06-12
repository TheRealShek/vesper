# Vesper

*A keyboard-first local media gallery for Linux.*

Vesper is a lightning-fast, simple media viewer. It indexes your local images and videos, using your existing directory structure to automatically derive tags. It is strictly read-only and designed to provide a seamless, premium desktop experience without modifying your files.

---

### Core Principles

- **Zero Configuration Tagging:** Your folder names automatically become your tags.
- **High Performance:** Smoothly scroll through 50,000+ files without any lag.
- **Keyboard-Driven:** Navigate, search, and view media entirely via keyboard shortcuts.
- **Locally Managed:** 100% offline. No cloud sync, no accounts, and strictly read-only.
- **Native Experience:** Built with GTK4 and Libadwaita for a premium, native Linux interface.

### Technology Stack

| Component | Technology |
| :--- | :--- |
| **Language** | Rust |
| **GUI Toolkit** | GTK4 & Libadwaita |
| **Database** | SQLite |

### Installation & Execution

**1. System Dependencies**

Ensure you have Rust, Cargo, GTK4, and SQLite development tools installed.

*For Fedora:*
```bash
sudo dnf install gtk4-devel libadwaita-devel pkg-config sqlite-devel
```

*For Ubuntu/Debian:*
```bash
sudo apt install libgtk-4-dev libadwaita-1-dev pkg-config libsqlite3-dev
```

**2. Build and Run**

```bash
cargo run --release
```

### Source Architecture

| Directory | Purpose |
| :--- | :--- |
| `src/main.rs` | Core application logic and startup initialization. |
| `src/ui/` | GTK4 user interface and virtualized grid components. |
| `src/index/` | High-performance background filesystem crawler. |
| `src/db/` | SQLite layer for state and metadata caching. |
| `docs/` | Comprehensive design and product specifications. |
