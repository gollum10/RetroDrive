# RetroDrive Studio — Source + Full Design Documentation

**Start with [`docs/00-START-HERE.md`](docs/00-START-HERE.md).** This package combines the prior v0.1 desktop scaffold with a comprehensive design handoff.

# RetroDrive Studio

**an earlier publisher name** — early Windows desktop application scaffold for a full retro gaming management suite.

## What works today
Full, maintained list: **[Features.md](Features.md)**.
- **Navigation:** Dashboard, Game Library, Drive Manager, Emulation Systems, Settings.
- **Drives:** read-only detection of mounted volumes and their capacity.
- **Import:** scan an existing ROM folder in the background *without moving, modifying or deleting anything*. Shows live progress, can be cancelled, and the report lists ignored and unreadable files.
- **Activity:** every scan is logged. The Dashboard shows recent scans, including ones interrupted by a crash.
- **Catalog:** normalized SQLite catalog (schema v2) in the app data folder. Versioned migrations back up the catalog before upgrading it.
- **Library:** search, plus **Online** / **Offline** (drive not connected) / **Missing** (not found by the last complete scan) status for each file.

## Not yet implemented
This is an **early development build**, not a drive installer. No imaging, formatting, partition creation, ROM deployment, metadata scraping, BIOS repair, controller mapping, or automatic updates are implemented. The platform labels are a roadmap, not functional installers.

## Requirements (Windows)
- Node.js LTS and npm
- Rust toolchain (rustup, stable MSVC target)
- Microsoft C++ Build Tools (Desktop development with C++)
- WebView2 runtime (typically included in Windows 11)
- Git and Visual Studio Code (recommended)

See Tauri's official Windows prerequisites: https://v2.tauri.app/start/prerequisites/

## Run locally
```powershell
npm install
npm run tauri dev
```

The first Rust build can download numerous dependencies. To compile a production installer later:
```powershell
npm run tauri build
```

`npm run dev` alone opens the UI in a browser, but native commands **do not** work without Tauri.

## Create your GitHub repo
The project lives at **https://github.com/gollum10/RetroInstaller** (private). Clone it outside OneDrive:
```powershell
git clone https://github.com/gollum10/RetroInstaller.git C:\dev\RetroInstaller
```
Original first-push steps, kept for reference:
```powershell
git init
git add .
git commit -m "Initial RetroDrive Studio scaffold"
git branch -M main
git remote add origin https://github.com/YOUR_USERNAME/retrodrive-studio.git
git push -u origin main
```
Replace `YOUR_USERNAME` with your GitHub account. GitHub may prompt for authentication.

## Design rules
- Always use a preview and explicit confirmation before potentially destructive disk operations.
- Start with a hybrid catalog: game identity, physical files, and deployment manifests will eventually be separate entities.
- Do not bundle copyrighted ROMs or BIOS firmware.
- Store user library data locally; future sync is opt-in.
- Platform adapters must follow upstream platform layout, terms, and licensing.

## Current simplifications
- **One game per file:** each indexed file gets its own game record. Matching, disc grouping and duplicate detection are planned.
- **Platform guesses** come from folder names and extensions; ambiguous archive and disc files are marked `Unclassified`.
- **Verified 2026-11-04** on Windows 11 with Node 24, Rust 1.99.1 and VS 2026 C++ tools: `cargo test` (34 passing), `npm run build`, and `npm run tauri dev`, including a 40,000-file scan, cancel and crash recovery.
