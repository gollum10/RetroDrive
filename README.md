# RetroDrive

A free Windows app for people who keep their own retro game collection.

![RetroDrive logo](branding/retrodrive-logo-full.png)

## Download

**[Download RetroDrive for Windows](https://github.com/gollum10/RetroDrive/releases)**: the newest version is at the top of the Releases page. Get `RetroDrive_<version>_x64-setup.exe`, then run it. RetroDrive is free, open source (Apache-2.0), and runs on Windows 10 and 11 (64-bit).

- **It's a beta.** The current release is 0.1.0. Please report problems on [GitHub Issues](https://github.com/gollum10/RetroDrive/issues).
- **"Windows protected your PC":** the installer isn't code-signed yet, so Windows SmartScreen may warn you. Choose **More info → Run anyway**. Each release lists SHA-256 checksums so you can check your download.
- **Bring your own games.** RetroDrive doesn't include or download games, BIOS files, firmware or game art.

## What it does
RetroDrive catalogs your games, copies them safely to the systems you play on (RetroBat, Batocera, RetroPie, ES-DE, Recalbox, ROCKNIX, Knulli, Lakka), keeps them in sync, backs up your saves, sets up controllers and light guns, and lets you design your own themes in Theme Studio, with optional AI helpers.

- It never overwrites or deletes your files. Anything it replaces is backed up first and can be undone.
- There's no account and no telemetry.
- It doesn't format, image or partition disks.

The full, maintained list of what's ready, what has limits and what's planned is in **[FEATURES.md](FEATURES.md)**. What changed and when: **[CHANGELOG.md](CHANGELOG.md)**. How it's designed: **[docs/00-START-HERE.md](docs/00-START-HERE.md)**.

## Build from source
Most people should use the [download](#download). To build it yourself:

### Requirements (Windows)
- Node.js LTS and npm
- Rust toolchain (rustup, stable MSVC target)
- Microsoft C++ Build Tools (Desktop development with C++)
- WebView2 runtime (typically included in Windows 11)
- Git and Visual Studio Code (recommended)

See Tauri's official Windows prerequisites: https://v2.tauri.app/start/prerequisites/

### Run locally
Clone the repo outside OneDrive (sync can lock build files):
```powershell
git clone https://github.com/gollum10/RetroDrive.git C:\dev\RetroDrive
cd C:\dev\RetroDrive
npm install
npm run tauri dev
```

The first Rust build downloads many dependencies. To build the installer yourself:
```powershell
npm run tauri build
```

`npm run dev` alone opens the UI in a browser, but native commands **do not** work without Tauri.

Contributions are welcome: see [docs/12-DEVELOPMENT-GUIDE.md](docs/12-DEVELOPMENT-GUIDE.md) and [AGENTS.md](AGENTS.md).

## Legal
RetroDrive doesn't include or download games, BIOS or firmware; it organizes and copies files you choose, for your personal use. It isn't for preparing drives or machines for others or for sale. See [docs/16-LEGAL-AND-COMPLIANCE.md](docs/16-LEGAL-AND-COMPLIANCE.md) for the legal assessment and [docs/17-THIRD-PARTY-COMPONENTS.md](docs/17-THIRD-PARTY-COMPONENTS.md) for the component inventory.

> RetroDrive is an independent retro gaming management application. All third-party product names and trademarks belong to their respective owners. Compatibility references do not imply sponsorship, affiliation, or endorsement. *(Draft wording, pending counsel review.)*

## Design rules
- Always use a preview and explicit confirmation before potentially destructive disk operations.
- Start with a hybrid catalog: game identity, physical files, and deployment manifests will eventually be separate entities.
- Do not bundle copyrighted ROMs or BIOS firmware.
- Store user library data locally; future sync is opt-in.
- Platform adapters must follow upstream platform layout, terms, and licensing.

## Current simplifications
- **Game identity:** each game is one file or one disc set. Matching against metadata databases is planned; duplicates are flagged by name and size, not yet by hash.
- **Platform guesses** come from folder names and extensions; ambiguous archive and disc files are marked `Unclassified`.
- **Verified 2026-11-04** on Windows 11 with Node 24, Rust 1.99.1 and VS 2026 C++ tools: `cargo test` (168 passing; 374 as of 2026-11-10), `npm run build`, and `npm run tauri dev`, including 40,000-file scans, cancel, crash recovery, drive recognition, search, paging, duplicates, disc grouping and the RetroBat placement preview, plus (2026-11-05) the RDS-009 deployment plan and the RDS-010 verified copy (copy, stale-plan refusal and cancel tested in the app; see docs/18).

## License

RetroDrive is free and open source under the [Apache License 2.0](LICENSE) (see also
[NOTICE](NOTICE)). Copyright 2026 The RetroDrive contributors. The open-source components
it's built with keep their own licenses, listed in
[`THIRD_PARTY_NOTICES.txt`](src-tauri/resources/THIRD_PARTY_NOTICES.txt); data files are
credited in [`THIRD_PARTY_CONTROLLERS.txt`](src-tauri/resources/THIRD_PARTY_CONTROLLERS.txt).
Privacy: [docs/legal/PRIVACY.md](docs/legal/PRIVACY.md). Questions and problems:
[GitHub Issues](https://github.com/gollum10/RetroDrive/issues).
