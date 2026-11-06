# RetroDrive — Features

The single list of what the app **does today** and what is **planned**. Update this file in the same change as the code. A feature is only listed as Shipped once it exists in source, has automated tests where it can, and has been run on Windows.

**Status key:** ✅ Shipped · 🟡 Partial (works, with limits noted) · 📋 Planned · ⛔ Out of scope until a safety review

Requirement IDs (FR/NFR) refer to [docs/02-SCOPE-AND-REQUIREMENTS.md](docs/02-SCOPE-AND-REQUIREMENTS.md); RDS IDs refer to [docs/11-ROADMAP-AND-BACKLOG.md](docs/11-ROADMAP-AND-BACKLOG.md).

_Last updated: 2026-11-05 (arcade choice, RetroBat version, real-world test)_

---

## App shell (FR-01)

| Feature | Status | Notes |
|---|---|---|
| Windows desktop app (Tauri v2 + React) | ✅ | Built and launched on Windows 11 with Rust 1.99.1 / VS 2026 C++ tools. |
| Sidebar navigation: Dashboard, Game Library, Drive Manager, Emulation Systems, Settings | ✅ | Screen names differ from docs/03 (My Library, Drives, Systems); to be aligned. |
| Dashboard counters: cataloged files, platforms, detected volumes, available / offline / missing | ✅ | Counts come from indexed catalog queries; no invented health scores. |
| Dashboard duplicate hint with a "Review them" link | ✅ | Opens the library filtered to possible duplicates. |
| RetroDrive branding: art in the sidebar, "RetroDrive" window title and installer name | ✅ | ADR-017. an earlier publisher name removed from the app. Since 2026-11-05 the bottom-left of the side menu shows the owner's neon "cartridges streaming onto a drive" art; the RetroDrive logo stays in `branding/`. |
| RetroDrive app icon (taskbar, window, installer, sidebar header) | ✅ | The cartridge emblem from the logo on a rounded navy square, generated from `branding/retrodrive-icon.png` with `npx tauri icon`. Replaces the starter's placeholder icons. |
| Accurate status banner | ✅ | Separates "running in browser" from real catalog errors. |
| Live scan progress panel with Cancel button | ✅ | RDS-002. Shows items checked, game files found, new files, unreadable count and the current file. The bar is indeterminate because the total isn't known in advance. |
| Recent activity on the Dashboard | ✅ | Last 5 jobs with state (Completed / Cancelled / Failed / Interrupted), folder, file count, error and time. |
| Pause / resume jobs | 📋 | Not planned for scans (just cancel and rescan). Will come with deployment jobs. |
| Deployments, Artwork Studio, Profiles, Maintenance screens | 📋 | M2–M4. |

## Drive Manager (FR-02)

| Feature | Status | Notes |
|---|---|---|
| List mounted volumes with capacity, free space, removable flag | ✅ | Read only (sysinfo). |
| Volume type recorded in the catalog (local / removable / network) | ✅ | Network = UNC path; removable comes from the OS. |
| Stable drive identity (survives drive-letter changes) | ✅ | RDS-003. Each volume is fingerprinted by its serial number and size, read from Windows. A drive that comes back under a new letter is matched to its existing records, and the scan says "Recognized this drive from before (it was at E:\)". |
| Different drive using a known letter | ✅ | Gets its own records. The old drive's games show **Offline**, not Online. |
| Known drives list (Drive Manager) | ✅ | Every scanned drive, plugged in or not. Shows label, connection status (Connected / Not connected / letter now used by a different drive), file and missing counts, filesystem, size, volume ID and last scan. |
| `subst` drive letters | ✅ | Resolved to the real folder underneath, so an alias letter never creates duplicate records. |
| Confirmation prompt when a drive match is ambiguous | 📋 | Not needed yet: copies are refused when the drive doesn't match its fingerprint. Copies re-check the drive's fingerprint before starting and before every game (RDS-010, QA-015). |
| Physical disk vs volume model, filesystem type, system-disk flag | 📋 | docs/06. |
| Format, partition, image, clone | ⛔ | Needs the elevated-helper safety review (RDS-014). |

## Master Library (FR-03)

| Feature | Status | Notes |
|---|---|---|
| Browse by system (Plex-style) | ✅ | Systems page: a card per platform with game count and size; click to see that system's games. |
| Grid or list view | ✅ | Grid shows cover tiles (coloured placeholders until box art); list is the detailed table. Your choice is remembered. |
| Reference catalog import (names, details, genres) | ✅ | Settings → Reference catalog: import your `.xlsx`/`.csv` and an optional genre file. Games are matched by exact No-Intro/MAME name, MAME clone set, then title per system; cards and the list show catalog titles, year and genre. Art columns are never read. On your library: 47,840 of 116,405 games matched (Arcade 93%). |
| Catalog import waits for scans and copies | ✅ | Its buttons are greyed out during a scan or copy, and scans wait for an import (instead of a "database is locked" error). |
| Search and sort by catalog titles | 📋 | Search still uses file titles today. |
| Arcade routing by source folder (`mame`/`fbneo`) | 📋 | Your library already separates them. |
| Box art from your own images | ✅ | Found automatically during scans (your `gamelist.xml` from Skraper/ScreenScraper, or images named like the game in `media`, `images`, `boxart`, `covers`…), or pick one with **Add art** on a game card. Shown from where it is; never copied or shipped. On your NAS's NES folder: 1,520 of 1,520 images linked. |
| Scan summary counts images separately | ✅ | "N images found, N games linked to art" instead of "non-game files ignored". |
| Game details from gamelist.xml (descriptions, players, rating) | 📋 | Follow-up. |
| Scraper (e.g. ScreenScraper) | ⛔ | Blocked on LEGAL-007. |
| Import (scan) a user-selected folder | ✅ | Runs as a background job; the window stays responsive. Read only: tests confirm source files are unchanged. 40,000 files took about 3 s in a dev build. |
| 49 systems recognized, incl. your hack folders | ✅ | C64, Amiga, Amstrad, Atari, MSX, X68000, PC Engine, Neo Geo, WonderSwan and many more; `nesh`/`snesh`/`genh`/`megadrive-japan` join their systems. On your library: Unclassified drops from 60,558 to 4 after a rescan. RetroBat folders mapped for all of them. |
| BIOS files kept out of the game list | ✅ | `bios` folders and `[BIOS] …` names become **BIOS** (never placed or copied as games). A BIOS checker is planned (M4). |
| Platform detection from folder names | ✅ | Uses the nearest platform folder (e.g. `roms/psx/Game (USA)/game.cue`). The scanned folder's own name counts. |
| Non-game files ignored inside platform folders | ✅ | `readme.txt`, `.srm` saves, `.state`, `gamelist.xml`, images and similar are skipped. |
| Extension-only detection outside platform folders | ✅ | `.zip`, `.7z`, `.chd`, `.iso`, `.cue`, `.m3u` are recorded as **Unclassified**. |
| Multi-file games counted once | ✅ | RDS-005. A `.cue` sheet's tracks (`.bin` and others) and an `.m3u` playlist's discs are grouped into one game, shown with a "2 discs" badge and the game's **total** size. Discs keep their order. |
| Incomplete games flagged | ✅ | When a track or disc a game needs is missing, the game shows **Incomplete** and the scan lists a warning naming the missing file. |
| Unsafe references refused | ✅ | Cue sheets and playlists that point outside the scanned folder are ignored with a warning. Files over 1 MB are not parsed. |
| Removed playlists handled | ✅ | If an `.m3u` is deleted, its discs become separate games again on the next complete scan. A cancelled or partly unreadable scan never splits a game. |
| Errors vs warnings in scan reports | ✅ | Errors (something couldn't be read) make a scan incomplete and skip missing-file detection. Warnings (links, broken references) don't. |
| Long Windows paths (over 260 characters) | ✅ | Indexed, and stored once per volume (a duplicate-record bug was found and fixed, QA-008). |
| Files locked by another program | ✅ | ROMs are still indexed from their directory entry; an unreadable cue sheet is reported as an error (QA-009). |
| Supported systems | ✅ | NES, SNES, Genesis, Arcade, PlayStation, PlayStation 2, GBA, Game Boy, Game Boy Color, N64, Master System, Game Gear. |
| Scan report | ✅ | Counts of indexed, new, ignored, missing and unreadable items, plus an expandable issues list (first 200). |
| Unreadable files reported, scan continues | ✅ | QA-009 (now with a locked-file test). |
| Symbolic links and junctions not followed | ✅ | Reported as skipped (NFR-02, QA-014). |
| Rescan without duplicates | ✅ | One row per (volume, relative path). |
| Missing-file detection | ✅ | A file is marked **Missing** only after an error-free scan of its folder. Rows are never deleted. Restoring the file sets it back to present. |
| Offline drives | ✅ | When the volume is unreachable, or its letter now holds a different volume, its files show **Offline**; the catalog keeps them. |
| Clean Windows paths | ✅ | No `\\?\` prefixes are stored or shown. Paths are stored per volume plus a relative path. |
| Manual title corrections kept on rescan | ✅ | Rescans don't overwrite titles. No title-editing UI yet. |
| Search by title or platform | ✅ | RDS-004. Full-text trigram index (SQLite FTS5) matches anywhere in a title, case-insensitively. Searches under 3 characters fall back to a plain scan. Typing is debounced. Special characters are searched literally. |
| Paged library (100 per page) | ✅ | The window never loads the whole catalog. Previous/Next with "1–100 of N". Filtered counts stop at 10,000 ("10,000+"). |
| Filters: platform, status (Online / Offline / Missing), drive | ✅ | Can be combined with search; "Clear filters" resets them. |
| Sort by title, platform or largest first | ✅ | Uses an index, so pages stay fast at 100k+ files. |
| Readable paths | ✅ | Long paths keep the drive and last folders (`C:\…\roms\snes\Game.sfc`); the full path shows on hover. |
| Duplicate candidates | ✅ | RDS-004. Same platform, same file name (ignoring case) and same non-zero size. Shown with a ×N badge and a "Possible duplicates only" filter. Informational only: nothing is ever deleted. |
| Hash-verified duplicates | 📋 | Needs background hashing; `sha256` / `crc32` columns are reserved. |
| Collections and favorites | 📋 | M2. |
| Scan progress and cancellation | ✅ | RDS-002. Cancelling keeps files found so far and skips missing-file detection, so it can't mark files missing (QA-002). |
| One scan at a time | ✅ | Starting a second scan is refused with a clear message. |
| Scans survive crashes safely | ✅ | Work is saved in batches (500 files or 1 s). A scan cut off by a crash or forced close is marked **Interrupted** on the next launch; files already saved are kept. |

## Catalog database

| Feature | Status | Notes |
|---|---|---|
| Normalized hybrid schema v1: storage locations, indexed roots, games, releases, files, file locations | ✅ | RDS-001. See [docs/05-DATA-MODEL.md](docs/05-DATA-MODEL.md). |
| Versioned migrations (`PRAGMA user_version`) | ✅ | Run at startup in one transaction. |
| Automatic backup before upgrading an existing catalog | ✅ | `retrodrive.sqlite.v<N>-backup-<unix time>` next to the catalog. |
| v0.1 catalog upgrade without data loss | ✅ | Row counts are verified, otherwise rolled back (QA-017). |
| Refuses a catalog from a newer app version, untouched | ✅ | |
| Job log (schema v2): `jobs` and `job_events` | ✅ | Every scan records start, end state, counts, errors and each scan issue. |
| Saved installations (schema v6) | ✅ | `installations` table: adapter, contract version, fingerprinted storage location and relative path. |
| Multi-file game records (schema v5) | ✅ | `game_files.file_role` (primary / disc / track), `parent_file_id`, `payload_bytes`, `missing_parts`. Upgrades from every earlier schema are tested. |
| Search and query indexes (schema v4) | ✅ | `games_fts` trigram index kept in sync by triggers; indexes for duplicate keys, platform+title, storage+status. |
| Volume fingerprints (schema v3) | ✅ | `storage_locations` rebuilt: unique `stable_id`, non-unique `root_path`, plus label, filesystem, serial, GUID and size. References are checked before commit. |
| Catalog export / delete controls | 📋 | docs/10 privacy defaults. |

## Platforms (FR-04) and deployment (FR-05)

| Feature | Status | Notes |
|---|---|---|
| `PlatformAdapter` contract v1 (read-only) | ✅ | RDS-006. Detect, layout checks and placement plans; adapters can't install or write. A mock adapter test proves the contract works for any implementation. |
| Emulation Systems page: platform list with each adapter's abilities | ✅ | Shown from the adapters themselves, including what isn't possible yet. |
| Add an existing RetroBat installation | ✅ | RDS-007. Pick a folder; RetroDrive shows the evidence (RetroBat.exe, emulationstation, retrobat.ini, roms) and layout checks before saving. Unrecognized folders are refused, with a hint when a folder looks partly like RetroBat. |
| RetroBat layout checks | ✅ | roms, bios, emulators and emulationstation folders, plus the install location (the wiki advises against spaces, accents and drive roots). The version is shown as unknown because RetroBat doesn't document a version file. |
| Saved installations survive drive-letter changes | ✅ | Stored against the drive's volume ID, like scanned folders. |
| Deployment plan (replaces the placement preview) | ✅ | RDS-009. For each saved installation: which `roms\<system>` folder each game would go to, and what would happen to it: copy, already there (same name and size), already in place (you scanned the RetroBat folder), duplicate (copied once), conflict (a different file is there and is kept) or blocked, with the reason. Copies and creates nothing (verified: the installation folder is unchanged). |
| Drive space check | ✅ | Live free space on the installation's drive, a safety reserve (1 GB or 2% of the drive, whichever is bigger) and room for the largest file, rounded to the drive's cluster size. Says plainly whether it fits. |
| Choose which platforms to deploy | ✅ | Tick or untick each RetroBat system folder; the plan updates. Collections come later. |
| Destination safety checks | ✅ | Blocks files too big for FAT32, names that clash only by upper/lower case, links or junctions on the way, folders in the way, and paths outside the installation. Warns about paths over 260 characters. |
| RetroBat folder mapping (verified against the RetroBat wiki) | ✅ | nes, fds (Famicom Disk `.fds`), snes, megadrive, mastersystem, gamegear, psx, ps2, gb, gbc, gba, n64. Multi-disc games get their own subfolder so playlists keep working. |
| Arcade placement (MAME or FinalBurn Neo) | ✅ | Per installation: *Arcade games go to* MAME (`roms\mame`) or FinalBurn Neo (`roms\fbneo`); not chosen by default, so nothing is guessed. Arcade zips keep their exact names. CHD disks aren't placed yet. |
| RetroBat version shown | ✅ | Read from `system\version.info` (e.g. 8.3.1.1-stable-win64). |
| Copy progress where you clicked | ✅ | Progress and Cancel replace the Copy button, and the result scrolls into view (owner feedback). |
| Name clashes show their source files | ✅ | "Needs your attention" shows which library file each row is. |
| Conflict and clash choices | 📋 | RDS-017: pick one version or keep both; replace only with backup and confirmation. |
| File-type check per target | 📋 | RDS-018: warn when a target won't show a file type (e.g. zipped NES games on OneSauce). |
| OneSauce / RetroFE (AtGames Legends Ultimate) | ⛔ | RDS-020, blocked on LEGAL-015 (docs/16 §6). |
| Emulator customizer | 📋 | RDS-021: emulators, display, controls; backup and undo; own safety review. |
| Batocera adapter | 📋 | RDS-008. |
| Copy games to RetroBat (verified) | ✅ | RDS-010. After you confirm, copies the plan in the background with progress and Cancel. Re-checks the plan, the drive's identity and free space first. Each file goes to a temporary name, is checked with SHA-256 by reading it back from the drive, and only then gets its real name. **Never overwrites or deletes** your files (a file that appears mid-copy is kept). Safety review: docs/18. |
| Existing files checked by content | ✅ | "Already there" files are compared byte for byte before being skipped; if they differ, the existing file is kept and reported. |
| Cancel and crash recovery | ✅ | Cancel discards only the file in progress. After a crash or unplugged drive, RetroDrive removes its own unfinished temporary files the next time that same drive is connected. Partly copied multi-disc games keep their verified discs; copying again finishes them. |
| Copy record (manifest) | ✅ | Every copy is recorded in the catalog and as `.retrodrive\deployments\<id>.json` in the RetroBat folder. Verified checksums are saved to the catalog. |
| Conflict choices (replace with backup, side-by-side rename) | 📋 | Today conflicts are always kept (docs/08). |
| Guided installs and imaging | ⛔ | RDS-012 to RDS-014, gated. |

## Later modules

| Feature | Status | Requirement |
|---|---|---|
| Artwork and metadata scraping with attribution | 📋 | FR-06 |
| BIOS checker (user-supplied files only) | 📋 | FR-07 |
| Controller profiles | 📋 | FR-08 |
| Backup and restore of configs and saves | 📋 | FR-09 |
| Maintenance checks and repair plans | 📋 | FR-10 |
| Arcade Builder | 📋 | FR-11 |
| Optional AI troubleshooting (opt-in, redacted) | 📋 | FR-12 |
| More platforms (Recalbox, Lakka, ES-DE, …) | 📋 | FR-13 |

## Legal and compliance (docs/16)

| Item | Status | Notes |
|---|---|---|
| No games, BIOS, firmware or artwork shipped or downloaded | ✅ | Checked in docs/17 §5. |
| RetroBat support limited to the user's own installation | ✅ | RetroDrive works only with a RetroBat you installed: it checks the folder and copies games into it after you confirm. Downloading or installing RetroBat is blocked on LEGAL-002/003. |
| Personal-use statement (Settings, README) | 🟡 | Draft: RetroDrive is for your own games, for personal use; not for preparing drives or machines for others or for sale (ADR-023). |
| Independence statement in the app (Settings) and README | 🟡 | Draft wording from docs/16 §8, pending counsel review. |
| Third-party component manifest (LEGAL-001) | 🟡 | Software dependencies inventoried (280; 5 MPL-2.0 via Tauri need notices). Platforms and assets are listed by hand. |
| CI license gate (LEGAL-014) | 🟡 | `scripts/check-licenses.py` fails CI on copyleft or unknown licenses; release sign-off still to be defined. |
| Third-party notices file in the installer (LEGAL-009) | 📋 | Needed before public release. |
| Trademark clearance for "RetroDrive", and logo provenance (LEGAL-008) | 📋 | Owner action; the logo is AI-generated (docs/17 §4). |
| RetroBat written permission (LEGAL-003) | 📋 | Owner action; blocks RDS-012. |

## Engineering

| Feature | Status | Notes |
|---|---|---|
| Rust unit tests (77): migrations, classifier, paths, long paths, locked files, scanner, disc grouping, cancellation, batching, job log, drive identity, search, filters, duplicates, paging, adapter contract, RetroBat detection and placement, installations and preview | ✅ | `cd src-tauri && cargo test` |
| 100k-file performance benchmark | ✅ | `cargo test --release benchmark_100k -- --ignored --nocapture`; results in docs/15. |
| TypeScript type-check and frontend build | ✅ | `npm run build` |
| GitHub Actions CI on Windows (tests, clippy, build) | ✅ | Runs on every push to `main` and every pull request. |
| Pinned dependency lockfiles | ✅ | `package-lock.json`, `src-tauri/Cargo.lock`. |
| Content Security Policy | 📋 | Still `null` in `tauri.conf.json`; needs testing in dev and release builds. |
| Project license | 📋 | Open question #9 in docs/13. |
