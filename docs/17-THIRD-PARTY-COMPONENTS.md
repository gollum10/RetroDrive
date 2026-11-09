# 17 | Third-Party Component Manifest (LEGAL-001)

**Theme Studio browser regression harness:** no npm dependency or bundled browser added. Optional development command uses an independently installed Chromium/Edge executable and Node 22+ WebSocket support with existing Vite; no browser redistributed, downloaded or invoked at runtime.

**Canvas increment:** no new dependencies, fonts, game art or other media. Canvas uses React/native SVG and the system sans-serif font; generated JSON fixtures are original structural test data. No external resources are loaded by preview.

**Theme Studio PR 4:** no new dependencies or copied frontend themes/assets. Target-profile fixtures are original synthetic test data; historically observed application-release strings are not a bundled frontend or verified theme support.

**Theme Studio PR 3:** no new dependencies/assets; workspace/lifecycle tests reuse TypeScript, Node assertions and React DOM server rendering. No copied theme, game art or media is included.

**Theme Studio PR 2 (2026-11-08):** no new dependencies or assets. Persistence reuses rusqlite/SQLite JSON functions, serde/serde_json, UUID and existing Tauri background tasks; tests reuse tempfile.

**Theme Studio PR 1 (2026-11-08):** no new dependencies, copied third-party themes, game art, fonts or media assets. Blank/starter/future JSON fixtures are original structural test data. Contract tests use existing TypeScript and Node built-ins; Rust reuses serde/serde_json. No FFmpeg/ZIP/canvas package is introduced.

Inventory of everything RetroDrive ships, fetches or works with, using the fields from docs/16 §11. This is an engineering record for counsel review, not a legal conclusion. Update it whenever a component is added or considered.

_Last updated: 2026-11-05 (RetroBat license, OneSauce)._

## How this is kept current

- **Software dependencies:** `python scripts/check-licenses.py` lists every Rust crate compiled into the Windows build (normal and build dependencies, not test-only ones) and every production npm package. It checks each license against an allowlist and **fails CI** on anything not allowed (LEGAL-014).
- **Full generated list:** `python scripts/check-licenses.py --csv docs/third-party/dependencies.csv` writes [`third-party/dependencies.csv`](third-party/dependencies.csv) (name, version, license, verdict, source).
- **Everything else** (platforms, assets, content) is recorded by hand below.

## 1. Software compiled into RetroDrive

Measured 2026-11-09: **298 shipped dependencies**, 293 under permissive licenses, 5 needing notices (MPL-2.0), 0 denied. (2026-11-04: 280.)

| Component | Version | License | Distribution mode | Commercial rights | Obligations |
|---|---|---|---|---|---|
| Tauri (framework, `tauri`, `tauri-build`, plugins) | 2.12.x | MIT OR Apache-2.0 | Bundled (compiled in) | Permissive | Include license notices |
| `@tauri-apps/api`, `@tauri-apps/plugin-dialog` | 2.12.4 / 2.8.4 | Apache-2.0 OR MIT | Bundled (frontend) | Permissive | Notices |
| React, React DOM, scheduler, loose-envify, js-tokens | 18.3.1 … | MIT | Bundled (frontend) | Permissive | Notices |
| lucide-react (icons) | 0.468.0 | ISC | Bundled (frontend) | Permissive | Notices |
| rusqlite / libsqlite3-sys | 0.32.1 / 0.30.1 | MIT | Bundled | Permissive | Notices |
| SQLite (compiled in by `libsqlite3-sys` "bundled") | 3.x | Public domain | Bundled | Unrestricted | None required |
| walkdir, sysinfo, serde, serde_json, uuid, windows-sys (features: storage, foundation, ToolHelp process list since 2026-11-06) | see CSV | MIT / Apache-2.0 / Unlicense | Bundled | Permissive | Notices |
| quick-xml (reads `gamelist.xml`; already shipped via calamine) | 0.42.0 | MIT | Bundled | Permissive | Notices |
| calamine (reads the user's `.xlsx` catalog) | 0.38.0 | MIT | Bundled | Permissive | Notices |
| csv (reads `.csv` catalogs) | 1.x | Unlicense OR MIT | Bundled | Permissive | Notices |
| sha2 (SHA-256 for copy verification, RDS-010) | 0.10.9 | MIT OR Apache-2.0 | Bundled; already shipped through Tauri before it became a direct dependency (count unchanged) | Permissive | Notices |
| **cssparser, cssparser-macros, selectors, dtoa-short** | 0.37.0, 0.7.1, 0.38.0, 0.3.5 | **MPL-2.0** | Bundled, unmodified, pulled in by `tauri-utils` → `dom_query` | Allowed | **Ship the MPL-2.0 notice and say where the source of these files is available (crates.io).** Modified MPL files would have to be published; we don't modify them. |
| **option-ext** | 0.2.0 | **MPL-2.0** | Bundled, unmodified, via `tauri-build` → `dirs` | Allowed | Same as above |
| Other transitive crates | see CSV | Permissive (MIT, Apache-2.0, BSD, Zlib, Unicode-3.0, Unlicense, CC0, 0BSD) | Bundled | Permissive | Notices |

**Third-party notices (LEGAL-009, done 2026-11-09):** `python scripts/third-party-notices.py` writes `src-tauri/resources/THIRD_PARTY_NOTICES.txt` from the same list as the license gate: every component with its version and license, the source location of each MPL-2.0 component, and the license/notice files from each package (identical texts printed once with every component using them; for the 6 packages that ship no license file, the standard text appears for another component with the same license). The installer ships it next to `RetroDrive.exe` together with `THIRD_PARTY_CONTROLLERS.txt` (Tauri `bundle.resources`); Settings → About and legal opens either in Notepad (**Third-party notices**, **Data credits**). CI runs `--check` so the file can't fall behind the dependencies. Still open under LEGAL-009: counsel review of the drafts in [docs/legal](legal/) (EULA with warranty and liability wording, privacy policy).

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
| Batocera | Nothing. RDS-008 (2026-11-06) only reads a user's existing Batocera game folder; folder names were checked against Batocera's public docs and source, and no Batocera code or files are included | No | Images and installs are **blocked on LEGAL-004**. |
| Emulators and Libretro cores | None | No | Each must be audited separately before any download feature (LEGAL-005). |
| OneSauce (RetroFE build for AtGames Legends Ultimate) | Nothing. Considered 2026-11-05; a user's drive was inspected read-only (docs/16 §6) | No | **Blocked on LEGAL-015.** The build ships BIOS files; RetroDrive must never copy them. |
| ES-DE | None | No | Candidate frontend; check the exact version and license first (docs/16 §6). |

RetroDrive uses names like "RetroBat", "NES", "SNES", "PlayStation" and "Genesis" only to describe compatibility. No third-party logos are used (docs/16 §8).

## 4. RetroDrive's own assets

| Asset | Source | Notes / open questions |
|---|---|---|
| RetroDrive logo (`branding/retrodrive-logo-full.png`; `src/assets/retrodrive-logo.png` is no longer shown in the app) | Generated with ChatGPT and supplied by the project owner (2026-11-04) | **For counsel (LEGAL-008/012):** confirm the image-generation service's terms on output ownership and commercial use, whether a purely AI-generated image can be protected by copyright, and trademark clearance for the name and logo. The logo contains no third-party marks. |
| Side menu art (`branding/retrodrive-sidebar-art-full.png`, `src/assets/retrodrive-sidebar-art.png`) | Supplied by the project owner (2026-11-05); replaces the logo in the bottom-left of the side menu | Same open questions as the logo (provenance, generation-service terms, LEGAL-008/012). Contains no third-party marks or logos; the cartridge shapes are generic. |
| App icon (`branding/retrodrive-icon.png`, `src-tauri/icons/*`) | Cropped from the RetroDrive logo (ADR-017) | Same as the logo. |
| UI icons | lucide-react (ISC) | Covered in §1. |
| Fonts | None bundled; the system's Segoe UI / Inter fallbacks | None. |

## 5. Content

| Content | Status |
|---|---|
| Game ROMs or disc images | **None shipped or downloaded.** Test fixtures are generated at test time (empty or zero-filled files with game-like names) and deleted afterwards. |
| BIOS or firmware | None shipped or downloaded. The BIOS checker ships only a list of file names and descriptions (`resources/bios_requirements.tsv`) curated from libretro-core-info (MIT, commit eac3a278), credited with its licence text in `THIRD_PARTY_CONTROLLERS.txt`. |
| Controller data | **Shipped, with credit and licence texts on the Controllers page** (`src-tauri/resources/THIRD_PARTY_CONTROLLERS.txt`). The owner's controller list (`controllers.tsv`) is compiled from SDL GameControllerDB (zlib, commit 07af1d50) and RetroArch joypad autoconfig (MIT, commit 2e3b8a27). Step 2 also ships SDL GameControllerDB's Windows lines (`sdl_windows.txt`, 869) and RetroArch's udev, xinput, dinput and sdl2 setup files (`retroarch_joypads.txt`, 791 files) from the same commits, so setups work offline. Controller drawings are original line art (no photos or logos). Light guns (ADR-066) add the list of light gun games from Batocera EmulationStation's `resources/gamesdb.xml` (MIT, commit ccbcf40e; `gun_games.tsv`, 791 games: system, id and name only), credited on the same page. Vertical games (ADR-069) add the vertical arcade list from the same project's `resources/arcaderoms.xml` (MIT, commit b0ed9f56; `vertical_games.tsv`, 3,481 romset ids and names). |
| RetroBat themes (e.g. Carbon, CC BY-NC-SA) | Nothing shipped. The theme builder copies a theme already on the user's drive into a new folder for personal use, keeps its credits and license, and adds a note. Users choose their own pictures and fonts. | No | Remixes stay on the user's drive. |
| Game artwork, screenshots, manuals or metadata | None shipped or fetched; online providers are not planned (ADR-033). Images already on the user's drives are linked and displayed locally, never copied (ADR-025). |
