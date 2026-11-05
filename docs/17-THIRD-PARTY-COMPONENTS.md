# 17 | Third-Party Component Manifest (LEGAL-001)

Inventory of everything RetroDrive ships, fetches or works with, using the fields from docs/16 §11. This is an engineering record for counsel review, not a legal conclusion. Update it whenever a component is added or considered.

_Last updated: 2026-11-05 (RetroBat license, OneSauce)._

## How this is kept current

- **Software dependencies:** `python scripts/check-licenses.py` lists every Rust crate compiled into the Windows build (normal and build dependencies, not test-only ones) and every production npm package. It checks each license against an allowlist and **fails CI** on anything not allowed (LEGAL-014).
- **Full generated list:** `python scripts/check-licenses.py --csv docs/third-party/dependencies.csv` writes [`third-party/dependencies.csv`](third-party/dependencies.csv) (name, version, license, verdict, source).
- **Everything else** (platforms, assets, content) is recorded by hand below.

## 1. Software compiled into RetroDrive

Measured 2026-11-04: **280 shipped dependencies**, 275 under permissive licenses, 5 needing notices, 0 denied.

| Component | Version | License | Distribution mode | Commercial rights | Obligations |
|---|---|---|---|---|---|
| Tauri (framework, `tauri`, `tauri-build`, plugins) | 2.12.x | MIT OR Apache-2.0 | Bundled (compiled in) | Permissive | Include license notices |
| `@tauri-apps/api`, `@tauri-apps/plugin-dialog` | 2.12.4 / 2.8.4 | Apache-2.0 OR MIT | Bundled (frontend) | Permissive | Notices |
| React, React DOM, scheduler, loose-envify, js-tokens | 18.3.1 … | MIT | Bundled (frontend) | Permissive | Notices |
| lucide-react (icons) | 0.468.0 | ISC | Bundled (frontend) | Permissive | Notices |
| rusqlite / libsqlite3-sys | 0.32.1 / 0.30.1 | MIT | Bundled | Permissive | Notices |
| SQLite (compiled in by `libsqlite3-sys` "bundled") | 3.x | Public domain | Bundled | Unrestricted | None required |
| walkdir, sysinfo, serde, serde_json, uuid, windows-sys | see CSV | MIT / Apache-2.0 / Unlicense | Bundled | Permissive | Notices |
| calamine (reads the user's `.xlsx` catalog) | 0.38.0 | MIT | Bundled | Permissive | Notices |
| csv (reads `.csv` catalogs) | 1.x | Unlicense OR MIT | Bundled | Permissive | Notices |
| sha2 (SHA-256 for copy verification, RDS-010) | 0.10.9 | MIT OR Apache-2.0 | Bundled; already shipped through Tauri before it became a direct dependency (count unchanged) | Permissive | Notices |
| **cssparser, cssparser-macros, selectors, dtoa-short** | 0.37.0, 0.7.1, 0.38.0, 0.3.5 | **MPL-2.0** | Bundled, unmodified, pulled in by `tauri-utils` → `dom_query` | Allowed | **Ship the MPL-2.0 notice and say where the source of these files is available (crates.io).** Modified MPL files would have to be published; we don't modify them. |
| **option-ext** | 0.2.0 | **MPL-2.0** | Bundled, unmodified, via `tauri-build` → `dirs` | Allowed | Same as above |
| Other transitive crates | see CSV | Permissive (MIT, Apache-2.0, BSD, Zlib, Unicode-3.0, Unlicense, CC0, 0BSD) | Bundled | Permissive | Notices |

**Open item (LEGAL-009):** the installer doesn't yet include a third-party notices file. Generate one from the CSV and include it before any public release.

## 2. Software used but not shipped

| Component | Relationship | Notes |
|---|---|---|
| Microsoft Edge WebView2 Runtime | Required at run time; preinstalled on Windows 11 | Not bundled. If a bootstrapper is added to the installer later, review Microsoft's redistribution terms. |
| Rust toolchain, Node.js, Vite, TypeScript, Tauri CLI | Build-time only | Not shipped. |
| Microsoft Visual C++ runtime | The build links against the MSVC runtime | Review whether the installer needs the VC++ redistributable and its terms before public release. |

## 3. Third-party platforms RetroDrive works with

| Platform | What RetroDrive does today | Shipped or downloaded? | Status |
|---|---|---|---|
| RetroBat | Reads a folder the user already installed (version from `system\version.info`); checks layout; plans; copies the user's own games into `roms\` after confirmation (RDS-010). License per its installer (2026-11-05): non-commercial; team code LGPL-3.0; project content CC BY-NC-SA 4.0; selling pre-installed machines or drives is not allowed (docs/16 §5) | **No.** Nothing bundled, downloaded or modified. | Allowed as "manage a user-installed copy" (docs/16 §5). Downloading or installing RetroBat is **blocked on LEGAL-002/003**. |
| Batocera | Nothing yet (RDS-008 planned, read-only) | No | Images and installs are **blocked on LEGAL-004**. |
| Emulators and Libretro cores | None | No | Each must be audited separately before any download feature (LEGAL-005). |
| OneSauce (RetroFE build for AtGames Legends Ultimate) | Nothing. Considered 2026-11-05; a user's drive was inspected read-only (docs/16 §6) | No | **Blocked on LEGAL-015.** The build ships BIOS files; RetroDrive must never copy them. |
| ES-DE | None | No | Candidate frontend; check the exact version and license first (docs/16 §6). |

RetroDrive uses names like "RetroBat", "NES", "SNES", "PlayStation" and "Genesis" only to describe compatibility. No third-party logos are used (docs/16 §8).

## 4. RetroDrive's own assets

| Asset | Source | Notes / open questions |
|---|---|---|
| RetroDrive logo (`branding/retrodrive-logo-full.png`, `src/assets/retrodrive-logo.png`) | Generated with ChatGPT and supplied by the project owner (2026-11-04) | **For counsel (LEGAL-008/012):** confirm the image-generation service's terms on output ownership and commercial use, whether a purely AI-generated image can be protected by copyright, and trademark clearance for the name and logo. The logo contains no third-party marks. |
| App icon (`branding/retrodrive-icon.png`, `src-tauri/icons/*`) | Cropped from the RetroDrive logo (ADR-017) | Same as the logo. |
| UI icons | lucide-react (ISC) | Covered in §1. |
| Fonts | None bundled; the system's Segoe UI / Inter fallbacks | None. |

## 5. Content

| Content | Status |
|---|---|
| Game ROMs or disc images | **None shipped or downloaded.** Test fixtures are generated at test time (empty or zero-filled files with game-like names) and deleted afterwards. |
| BIOS or firmware | None shipped or downloaded. |
| Game artwork, screenshots, manuals or metadata | None shipped or fetched; providers are blocked on LEGAL-007. |
