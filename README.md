# RetroDrive Studio — Source + Full Design Documentation

**Start with [`docs/00-START-HERE.md`](docs/00-START-HERE.md).** This package combines the prior v0.1 desktop scaffold with a comprehensive design handoff.

# RetroDrive Studio

**an earlier publisher name** — early Windows desktop application scaffold for a full retro gaming management suite.

## What works in this starter
- Desktop navigation with Dashboard, Game Library, Drive Manager, Emulation Systems, Settings.
- Read-only detection of mounted volumes and capacity information.
- Import / scan an existing ROM directory *without moving, modifying, or deleting its contents*.
- SQLite catalog persisted in the user's app data directory.
- Search indexed games and mark missing/disconnected files offline.

## Not yet implemented
This is an **early development scaffold**, not a drive installer. No imaging, formatting, partition creation, ROM deployment, metadata scraping, BIOS repair, controller mapping, or automatic updates are implemented. The platform labels are a roadmap, not functional installers.

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
1. Create an empty **private** repository named `retrodrive-studio` at github.com/new. Do not auto-create a README, license, or `.gitignore` because this starter already includes them.
2. Extract the ZIP, open its `retrodrive-studio-starter` folder in VS Code, then open the terminal.
3. Run:
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
- The initial database stores one row per detected game file, not yet normalized canonical game identities.
- Platform inference uses file extensions and folder names; ambiguous disc and archive files are marked `Unclassified`.
- Scan is synchronous and may temporarily block interaction on very large collections; a cancellable background job is planned.
- No dependency/build verification was performed in the authoring environment. Test on a Windows development machine and log compatibility adjustments as issues.
