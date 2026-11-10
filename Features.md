# RetroDrive — Features

**Theme Studio layout (2026-11-10, ADR-109):** the canvas, layers and properties now fit on one screen; screens are tabs across the top.


**Theme Studio tool levels (2026-11-10, ADR-108):** choose **Simple**, **Standard** or **Pro** tools in the editor; the theme is the same in each. New themes start from a **Ready layout** with the game's title, year and description already placed.


**Theme Studio in plain words (2026-11-10, ADR-107):** Theme Studio speaks plainly (Save version, Save a copy, Add picture, Made for: any system) and follows the app's look (Classic, Galactic, Bridge, Cartoon, Versus). Coming next, per the [roadmap](docs/theme-studio/roadmap.md): skill levels, your art and videos everywhere, saving for RetroBat, and an optional AI assistant.


**Native comparison preparation:** an opt-in test now generates original compiler-produced themes, inert game markers and a disposable home configuration without launching RetroBat. [Preparation and pending native checks](docs/theme-studio/native-comparison.md). Native behavior and launch isolation remain unverified.


**Candidate package assembly:** a read-only catalog snapshot now resolves used managed PNGs, checks exact length/hash/signature and assembles bounded deterministic XML/assets/credits/manifest files in memory. Native verification, full licensing/privacy audit and user-facing export remain disabled. [Details](docs/theme-studio/retrobat-export-plan.md#managed-asset-package-spike).


**Experimental RetroBat compiler:** a pure in-memory candidate compiler now produces bounded XML and used-PNG declarations for a strict subset. It does not enable export, resolve asset bytes or write target files. See [scope and evidence](docs/theme-studio/retrobat-export-plan.md#compiler-spike-implementation).


**RetroBat export groundwork:** [pinned evidence and minimal exporter plan](docs/theme-studio/retrobat-export-plan.md) now defines the pure compiler, exact installed-engine capture and native acceptance gates. This is planning only; no exporter or installation support is enabled.


**VIS-004 partial typography:** text layers now support left/center/right alignment within their existing rectangle, including sample-bound multiline captions. Unknown values stay preserved read-only; source edits use existing history and draft/checkpoint persistence. Wrapping, vertical alignment, custom fonts and native engine output remain unimplemented.


**Theme Studio sample previews prepared:** synthetic normal/long/Unicode/missing text metadata plus placeholder mode, and typed text-source selection for title/description/year/players/rating/system name. Sample choices are session-only; binding edits use existing history/save. No emulator or personal library is read. Native acceptance remains pending.

**Theme Studio image controls prepared:** Fit/Fill, non-destructive percentage cropping/reset and asset dimensions/format/size/license/attribution/hash details. Existing images default to Fit/full image. Pixel-level browser checks pass; native visual/save/reopen confirmation remains pending.

**CI test reliability:** the copier confirmation test now gives each file pair a separate modification second and pins its confirmation to that pair. This removes a runner-timing assumption without changing production copying behavior.

**Saved PNG preview fix prepared:** shared in-flight requests prevent StrictMode setup/cleanup from racing the native image-worker guard on reopen. Bounded busy retries and state-based preview publication restore saved image layers without touching stored source/bytes. Native owner recheck remains pending.

**Theme Studio PNG workflow prepared:** user file picker with license/attribution, normalized immutable SQLite blobs, PNG image layers and authorized hash-checked previews. Images use normal move/resize/align/lock/hide/history services. Catalog v29 is additive; native import/save/reopen validation remains pending.

**Theme Studio image-import boundary prepared:** bounded native PNG decode/normalization, metadata stripping, canonical byte hashing and portable image path generation. This is codec-only infrastructure; file import/storage, asset UI and image rendering are not implemented yet.

**Theme Studio layer state prepared:** layer-list Lock/Unlock and Hide/Show controls persist optional editor flags with undo/redo and existing draft/checkpoint services. Locked/hidden layers reject content edits; hidden layers remain listed for recovery. Browser checks pass; native validation remains pending.

**Theme Studio grid/snapping prepared:** separate grid visibility and optional snapping controls, 8/16/32/64 logical-pixel spacing, pointer move/resize snapping, Alt bypass and canvas edge/center guides. Controls are session-only and preserve the universal source format. Native QA remains pending.

**Theme Studio resize regression fix:** handle pointer-down now owns focus and internal focus changes no longer cancel gestures. Workspace-width container breakpoints keep the canvas larger in normal windows. Real headless Edge input regression checks pass; native Tauri recheck remains pending.

**Theme Studio resize:** selected leaf text/panel layers now have a bottom-right resize handle and handle-focused arrow/Shift-arrow sizing. Rotated local-axis math and existing undo/save services are reused. Native pointer/keyboard QA remains pending; export stays disabled.

**Theme Studio alignment:** selected leaf text/panel layers can align left/right/top/bottom or center on either canvas axis, using rotated rectangular bounds and existing undo/save services. Native visual checks remain pending; export stays disabled.

**Theme Studio transforms:** text/panel inspectors now author rotation and numeric opacity with undo/redo. Unknown opacity expressions remain read-only. Native visual verification remains pending; export stays disabled.

**Theme Studio layer ordering:** selected text/panel layers can move backward/forward or to the back/front in flat supported scenes, with undo/redo and existing draft/checkpoint persistence. Nested/unknown scenes remain read-only for ordering. Native visual QA is pending; export remains disabled.

**Theme Studio first canvas increment:** add/select top-level text and panels, pointer/keyboard movement, numeric sizing/unit conversion, basic text/color/font properties and bounded session undo/redo. Editor preview remains approximate; nested/unknown content stays read-only/preserved. No native export, asset import or installation. [Behavior and manual checks](docs/theme-studio/canvas.md).

**Theme Studio project UX cleanup:** scoped dark controls replace native white inputs/buttons; Save As has a separate named confirmation form, disables original-property editing while open and announces copy creation. Suggested names use numbered copy suffixes, including legacy copy-copy names; identities/storage behavior are unchanged. Owner confirmed creation, checkpoint/draft recovery/discard, native close after restart, Save As creation and copy independence during disposable-catalog QA. Native visual recheck of this styling update remains pending.

**Theme Studio native-close fix (2026-11-08):** stop re-requesting close from its own handler; await persistence and let the SDK complete the original close with explicit main-window destroy permission. Save failures still keep the app open. SDK/permission regression tested; actual owner close/reopen recheck pending. Existing checkpoint/recovery/discard behavior was confirmed by the owner.

**Theme Studio PR 4: capability foundation implemented.** Read-only target profiles, exact theme-adapter contracts, five diagnostic outcomes, source-preserving feature reports and workspace compatibility panel. Production registry remains empty: every real target is unverified; no exporter/installer is enabled. [Evidence matrix](docs/theme-studio/target-matrix.md).

**Theme Studio PR 3: partial workspace implemented.** Separate project browser and no-target wizard; metadata editing, scene/layer overview, future canvas/inspector/timeline regions, autosave/checkpoints, recovery and guarded navigation. No drag-and-drop, media editing, native preview/export or installation. [Workspace behavior and verification limits](docs/theme-studio/workspace.md).

**Theme Studio PR 2 (2026-11-08): backend persistence implemented.** Additive catalog v27 stores universal checkpoints/drafts separately with revision conflicts, recovery/discard, rename and source duplication. Background IPC and typed frontend wrappers are available; workspace/wizard and UI autosave remain PR 3. [Storage contract](docs/theme-studio/persistence.md). Existing Customize, design records and target-write code remain unchanged.

**Theme Studio (2026-11-08, Sprint 1 PR 1): partial foundation only.** Portable source types, validation and fixtures exist: no-target projects, scene/node graphs, typed bindings, asset declarations and future-field preservation. [Contract](docs/theme-studio/project-format.md) and [delivery tracking](docs/theme-studio/requirements.md). Workspace, SQLite project storage, visual editor, compiler, packages and deployment remain planned. Customize is unchanged.

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
| RetroDrive branding: art in the sidebar, "RetroDrive" window title and installer name | ✅ | ADR-017. Only the RetroDrive name is used. Since 2026-11-05 the bottom-left of the side menu shows the owner's neon "cartridges streaming onto a drive" art; the RetroDrive logo stays in `branding/`. |
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
| A–Z order and quick-access bars | ✅ | Systems are sorted A–Z with a letter bar to jump; games have an A–Z bar (# for numbers and symbols). |
| Grid or list view | ✅ | Grid shows cover tiles (coloured placeholders until box art); list is the detailed table. Your choice is remembered. |
| Reference catalog import (names, details, genres) | ✅ | Settings → Reference catalog: import your `.xlsx`/`.csv` and an optional genre file. Games are matched by exact No-Intro/MAME name, MAME clone set, then title per system; cards and the list show catalog titles, year and genre. Art columns are never read. On your library: 47,840 of 116,405 games matched (Arcade 93%). |
| Catalog import waits for scans and copies | ✅ | Its buttons are greyed out during a scan or copy, and scans wait for an import (instead of a "database is locked" error). |
| Search and sort by catalog titles | ✅ | Search and sort use catalog titles (schema v14). |
| Arcade routing by source folder (`mame`/`fbneo`) | 📋 | Your library already separates them. |
| Box art from your own images | ✅ | Found automatically during scans (your `gamelist.xml` from Skraper/ScreenScraper, or images named like the game in `media`, `images`, `boxart`, `covers`…), or pick one with **Add art** on a game card. Shown from where it is; never copied or shipped. On your NAS's NES folder: 1,520 of 1,520 images linked. |
| Scan summary counts images separately | ✅ | "N images found, N games linked to art" instead of "non-game files ignored". |
| Game details from gamelist.xml (descriptions, players, rating) | ✅ | Read from the game lists already in your library folders when you scan: cards show "1–2 players · ★ 4.5" and the description on hover; filter by **Players** (2+, 3+, 4+) and save it as a smart collection (e.g. Couch co-op). Copies carry them to your systems' game lists where those lack them. |
| Scraper (e.g. ScreenScraper) | ❌ | Not planned: RetroDrive is free and doesn't fetch online metadata or art (owner decision, 2026-11-06). |
| Search and sort by catalog title | ✅ | Matched games are listed, sorted and A–Z filtered by their catalog title (e.g. *'88 Games* for `konami88.zip`); search finds file titles and catalog titles. Library pages are also much faster on big libraries (e.g. the A–Z bar went from about 3 s to under 0.25 s on 145,000 games). |
| Import (scan) a user-selected folder | ✅ | Runs as a background job; the window stays responsive. Read only: tests confirm source files are unchanged. 40,000 files took about 3 s in a dev build. |
| 49 systems recognized, incl. your hack folders | ✅ | C64, Amiga, Amstrad, Atari, MSX, X68000, PC Engine, Neo Geo, WonderSwan and many more; `nesh`/`snesh`/`genh`/`megadrive-japan` join their systems. On your library: Unclassified drops from 60,558 to 4 after a rescan. RetroBat folders mapped for all of them. |
| BIOS files kept out of the game list | ✅ | `bios` folders and `[BIOS] …` names become **BIOS** (never placed or copied as games). The **BIOS checker** in Customize shows which ones your systems need. |
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
| Collections | ✅ | Step 7. Your own named game lists: Collections tab, Add to collection on each game, Add all shown (current filters, up to 10,000), rename, remove, delete (asks twice). Lists only: games and files are never touched. |
| Favorites | ✅ | Star (☆) any game to add it to **Favorites**, a built-in collection listed first; copy it with Plan like any collection. Your favorites also show as favorites on your systems: the **Game art and details** step (in Customize, or the tick box when copying) marks them in each system's game list, without changing a favorite you set or removed in the system's own menu. |
| Art in the list view | ✅ | The Game Library's list view shows each game's cover as a small thumbnail (or a colored initials tile), with the ★ Favorites toggle, light gun and vertical marks, and players/rating, like the grid. |
| Collection order | ✅ | Move collections up and down (↑ ↓ on each card); Favorites always comes first, and Plan lists them in the same order. |
| Smart collections | ✅ | Pick filters in the Game Library (platform, **genre**, **years**, light gun, vertical, search) and **Save as smart collection**: it fills itself from your library, now and as you add games, and works in Plan and Sync. |
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
| Catalog export / delete controls | ✅ | ADR-097, ADR-098. **Settings → Library catalog**: **Export game list** saves a CSV to Documents (opens in Excel: title, platform, year, genre, players, file, size, drive, path, status, collections). **Library folders** lists every scanned folder with **Remove from catalog…** (e.g. a drive you no longer use): it shows how many games go first, and the next scan doesn't bring them back. **Clean up missing entries** forgets files a scan found gone. **Start over → Reset the catalog…** empties the library (scanned folders, games, collections, art picks, Leave-out choices) after saving a full copy; your systems, copy history, designs, Theme Studio projects and reference catalog stay. **Put back** returns to any saved catalog (the current one is saved first). Only RetroDrive's list changes: files on your drives are never touched. |

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
| Choose which platforms to deploy | ✅ | Tick or untick each RetroBat system folder; the plan updates. Or choose **What to copy: Collection** to copy just one collection's games. |
| Destination safety checks | ✅ | Blocks files too big for FAT32, names that clash only by upper/lower case, links or junctions on the way, folders in the way, and paths outside the installation. Warns about paths over 260 characters. |
| RetroBat folder mapping (verified against the RetroBat wiki) | ✅ | nes, fds (Famicom Disk `.fds`), snes, megadrive, mastersystem, gamegear, psx, ps2, gb, gbc, gba, n64. Multi-disc games get their own subfolder so playlists keep working. |
| Arcade placement (MAME or FinalBurn Neo) | ✅ | Per installation: *Arcade games go to* MAME (`roms\mame`) or FinalBurn Neo (`roms\fbneo`); not chosen by default, so nothing is guessed. Arcade zips keep their exact names. CHD disks aren't placed yet. |
| RetroBat version shown | ✅ | Read from `system\version.info` (e.g. 8.3.1.1-stable-win64). |
| Copy progress where you clicked | ✅ | Progress and Cancel replace the Copy button, and the result scrolls into view (owner feedback). |
| Name clashes show their source files | ✅ | "Needs your attention" shows which library file each row is. |
| Leave games out of the library | ✅ | **Leave out** on any game card or list row; Status: *Left out of library* shows them, with **Bring back**. Left-out games are never copied. The files stay on your drives. |
| Clean up duplicates | ✅ | Keeps one copy of each set of identical files and leaves the rest out, after a preview with examples. Nothing is deleted. |
| Conflict and clash choices | ✅ | In "Needs your attention": keep the existing file (default), **keep both** (saved as `Game (2).zip`), or **replace** (the original is moved to `.retrodrive\backup` on the drive first and put back if anything goes wrong). For two different library versions: copy neither, **use this version**, or keep both. Arcade sets are never renamed. Safety review: docs/18 §7. |
| Faster copies of many small files | ✅ | Four games are copied at once (each with every safety check), and the journal is lighter: 57 → about 370 small files per second in the benchmark (6.5×). Files of 32 MB or more are still written one at a time, so USB hard drives aren't overloaded. |
| Undo a replace (Replaced files) | ✅ | Each installation has **Replaced files** (called Backups before 2026-11-09): the files you replaced, with their originals in `.retrodrive\backup`. **Undo replace** (asks twice) puts the original back and moves RetroDrive's copy into the backup folder; it's refused if the file on the drive changed since the copy (checked by SHA-256). **Open backup folder** shows it in File Explorer. Nothing is ever deleted; delete old backups yourself if you need the space. |
| File-type check per target | 🟡 | RDS-018. **RetroBat:** Plan warns when RetroBat won't show some games because it doesn't list their file type for that system (from its own es_systems.cfg), e.g. ".7z NES games"; they're still copied. Other systems keep their lists where Windows can't read them, so they aren't checked yet. |
| OneSauce / RetroFE (AtGames Legends Ultimate) | ⛔ | RDS-020, blocked on LEGAL-015 (docs/16 §6). |
| Emulator settings (RetroBat) | ✅ | RDS-021 version 1. **Emulator settings** on a RetroBat installation: per system, choose the emulator and core, shader, bezel, aspect ratio and integer scaling, or AUTO. Choices come from your own RetroBat. **Review changes** shows exact before → after values; **Apply** (asks twice) works only while RetroBat is closed, keeps the previous settings file in `.retrodrive\backup` and leaves every other setting untouched; **Undo** puts it back. |
| Settings for one game | ✅ | **Batocera, Knulli and ROCKNIX:** in their settings cards, pick a system and then a **Game** to give just that game its own aspect ratio, shader, smoothing, rewind and so on (the same lines the device's own game menu uses). **RetroBat:** a **Settings for one game** card in Customize: pick a system and a game, then its own emulator and core, shader, bezel, aspect ratio or integer scaling. **Recalbox:** the same **Game** choice in its settings card; a game's settings go in a small `<game>.recalbox.conf` file next to it (Recalbox's own way), created on the first change; games that already have one are marked with a dot. |
| More emulator settings | 📋 | Controls and other advanced options (later versions of RDS-021). |
| Theme builder: remix (RetroBat) | ✅ | **Themes** on a RetroBat installation lists its themes (colors, size, credits). **Remix** makes your own version: change any of the theme's colors, its background picture and fonts (your own files), with an approximate live preview; it's saved as a new theme folder and the original is never changed. **Use in RetroBat** switches the theme (reviewed, backed up, undoable). Theme credits and licenses stay with every remix. |
| Customize page | ✅ | A **Customize** item in the menu (owner request 2026-11-06): pick the installation, then **Look** (themes, boot video, menu music), **Behavior** (interface mode such as kiosk or kid, fullscreen, random theme, hide Quit, VSync, frame rate, Windows language, hide Windows while playing) and **Emulators** (per-system settings). One **Changes made** list with Undo covers every change. Themes and Emulator settings moved here from the installation card. |
| Boot video and menu music | ✅ | Choose the boot video or random, turn it off, and set how it plays (wait for the end, skip with a gamepad, delay). **Add my own video…** (.mp4) and **Add my own music…** (.ogg/.mp3) copy your files into RetroBat as new files (nothing is replaced, checked by SHA-256); a new video is then offered as the boot video for review. |
| Designs (Customize) | ✅ | Customizing works through **designs** (owner request 2026-11-06): **New design** (or **New from this RetroBat** to start from its current setup) opens the **design studio** with Theme (keep, use another, or remix with live preview), Boot video (options, your own video), Music (menu music on/off, **music per system**, theme music, song titles and how long, volume, stop in screensaver, lower for game videos, your own tracks for all systems or one system), Behavior and Emulators tabs. Nothing touches RetroBat while designing; drafts are saved as you go. **Finish design** or **Abandon design**. **My designs** lists them; **Apply to RetroBat…** shows every change (theme to create, files to add, each setting before → after), applies only while RetroBat is closed, and **Undo** reverses it in one step. Designs are kept in RetroDrive, so they can be reused on other drives. |
| Choose which songs play | ✅ | In a design's Music tab, **Which tracks play → Play only the tracks I tick** lists every track (including system folders and tracks the design adds); untick the ones you don't want. Applying writes RetroBat's own favorites playlist and turns on *Play only songs from your favorites playlist*. Nothing is deleted; undo puts things back. |
| Game art and details (RetroBat, Batocera, RetroPie, ES-DE, Recalbox, ROCKNIX) | ✅ | Batocera too since 2026-11-07 (same game lists; a running Batocera shows them after Start → Game Settings → Update Gamelists). Your own box art, logos (marquees) and catalog details (title, year, genre, developer, publisher) show in RetroBat's menus for the games RetroDrive copied. Tick **Also add your box art, logos and catalog details** when confirming a copy, or use **Customize → Game art and details → Check what can be added** for games already on the drive. Images are copied as new files into each system's `images` folder; RetroBat's game lists only get what they're missing (RetroBat's own art and names are kept). Needs RetroBat closed; undo under *Applied designs*. Logos need a rescan of the ROM folders to be picked up. |
| Theme builder: new themes | 📋 | Next: build a theme from RetroDrive's own original layouts (RDS-026). |
| RetroPie | ✅ | RDS-023. Recognizes RetroPie's `\\RETROPIE\roms` network share or the `retropie` folder on a USB stick, plans with RetroPie's own folder names, **copies games** the same safe way as RetroBat, has **Sync**, lets you pick each system's **default emulator** (from the ones installed on that Pi, over the network), and adds your **art and details** to game lists already in its rom folders (creating a list is optional and off by default, because RetroPie would then stop showing art it scraped itself). Restart RetroPie to see changes. Not yet tried on a real RetroPie. |
| ES-DE | ✅ | RDS-025. Recognizes ES-DE's ROMs folder, including a fresh portable ES-DE (or points you to it from the program, ROMs_ALL or settings folder), plans with ES-DE's folder names, **copies games** the same safe way as RetroBat, and has **Sync**. ES-DE finds new games when it starts. Adds your **art and details**: details into ES-DE's own game lists, box art and logos into its media folder named after each game (pictures ES-DE already has are kept; close ES-DE first). |
| BIOS files | ✅ | Many systems need BIOS files to start games. RetroDrive now copies the ones in your library's `bios` folder (with their subfolders) into RetroBat's, Batocera's or a RetroPie USB stick's BIOS folder: tick **bios** in Plan, and Sync keeps it up to date. Files already there are never replaced. Re-import your library once so all BIOS files are found. For RetroPie over the network (its separate bios share) and ES-DE (RetroArch's system folder), use **BIOS files** in Customize. |
| Game controller setup | 🟡 | RDS-028. **Controllers** page: plug a controller into this PC and RetroDrive shows an outline of the style it thinks it is (Xbox-style, PlayStation-style, arcade stick, …). Choose **Yes, that's it**, or **No, choose another** and pick from the list of styles, each with a picture. RetroDrive remembers your answer. Details say whether RetroBat/ES-DE (SDL) and RetroArch already know it. Look up any of 1,405 controllers by name or USB ID. Credits SDL GameControllerDB and RetroArch joypad autoconfig. Once confirmed, the card shows each of your systems: ✓ works already, ＋ RetroDrive can set it up (RetroBat, RetroPie, ES-DE's RetroArch), or the steps to do it in the system's own menu (Batocera, Recalbox, controllers with no ready-made setup). **Set up on …** adds the setup files; nothing already there is changed, and it can be undone. Xbox-style controllers already work, so nothing is written for them. Plan and Sync offer **Also set up your controller** when confirming a copy, and remind you if no controller is set up in RetroDrive yet. **Light guns:** RetroDrive recognizes a light gun plugged into your PC (Sinden, GUN4IR, AimTrak, Blamcon, Retro Shooter and more), or you choose yours, and shows how to set it up on RetroBat, Batocera, RetroPie, ES-DE and Recalbox (which guns Recalbox supports, and RetroArch's steps on RetroPie and ES-DE), including how far to sit for your screen size. Recalbox's own light gun group can be switched on in Customize → Game groups. The **Game Library** marks light gun games and can show only those (nearly 800 known games, from Batocera's list), so you can make a collection of them and copy it with Plan. **Sinden on RetroBat:** download the Sinden software yourself, then RetroDrive copies it into RetroBat's tools folder (new files only, nothing replaced). |
| Recalbox | ✅ | RDS-025. Recognizes Recalbox's `\\RECALBOX\share` network share, the `recalbox` folder on an external drive, or its SD card's share partition; plans with Recalbox's folder names; **copies games and BIOS files** the same safe way as RetroBat; has **Sync**, and adds your **art and details** to its game lists (update the game lists in Recalbox or restart it to see them). Not yet tried on a real Recalbox. |
| More platforms (connectors) | 🟡 | **ROCKNIX:** recognized (`\\ROCKNIX\games-roms` or its roms folder), checked, planned and **copied** with ROCKNIX's folder names (BIOS too), kept up to date by Sync; every file verified, nothing overwritten or deleted. **Game art, details and ★ favorites** too (Customize, or the tick box when copying). **Settings** (aspect ratio, shader off, smoothing, rewind, auto save, integer scaling; all systems or one) in Customize when ROCKNIX is added over the network. **Knulli:** works as Batocera (same files and folders) and is named as Knulli. **Lakka:** add `\\LAKKA\ROMs`; games are copied, then added to Lakka's playlists (so they show in its menu) with your PNG box art; BIOS files go to its System share from Customize. On first start Lakka asks which core to use. |
| Batocera | ✅ | RDS-008 (recognize, plan) and RDS-022 (copy). Recognizes Batocera's game folder (`\\BATOCERA\share` over the network, or the `batocera` folder on an external game drive), explains what to pick if you choose the boot partition or the drive root, checks the layout and plans where games would go (CPS games follow the arcade choice). Copies games too (see Copy to Batocera). |
| Copy to Batocera | ✅ | RDS-022. Copies games to Batocera the same way as RetroBat (checked after writing, never overwrites or deletes): to a Batocera game drive plugged into your PC, or over the network to `\\BATOCERA\share` while Batocera is running. The first network copy leaves a small id on the share so a different Batocera with the same name is never mistaken for it. Network copies are new; not yet tried on a real Batocera. |
| Batocera Sync | ✅ | The Batocera card has **Sync**, working like RetroBat's: it keeps the platforms you copied up to date. |
| Batocera settings and theme | ✅ | RDS-024. In **Customize**, change Batocera's settings for all systems or one system (aspect ratio, shader set, decorations, smoothing, integer scaling, rewind, auto save/load, menu music) using Batocera's own choices, and switch its theme (built-in Carbon or themes you added). Reviewed before saving, backed up, undoable. Emulator and core choices aren't offered yet. |
| Vertical games and game groups | ✅ | The **Game Library** marks **vertical** arcade games (made for a portrait screen; 3,481 known, from Batocera's list) and can show only those, like light gun games. In **Customize**, RetroBat and Batocera can show **Light gun games** and **Vertical arcade games** as their own groups in their menus (their built-in groups, switched on for you; reviewed, undoable). For a vertical screen: Batocera's **Screen rotation** is in its settings; for RetroBat, RetroDrive points you to Windows' Display orientation. **RetroPie and ES-DE:** RetroDrive creates **Light Gun Games** and **Vertical Games** collections and fills them with the matching games it copied there, so you don't pick them by hand; *Add newly copied games* tops them up later. Nothing in a collection is ever removed. |
| Recalbox settings and theme | ✅ | In **Customize**, change Recalbox's settings for all systems or one system (aspect ratio, shader set, smoothing, rewind, auto save/load), plus integer scaling, frame rate, press twice to quit and the **theme**. Changes are saved in `system
| ROCKNIX settings | ✅ | In **Customize**, for a ROCKNIX added over the network (`\\ROCKNIX\games-roms`): aspect ratio, shader set (off, or keep the device's), smoothing, rewind, auto save/load and integer scaling, for all systems or one. Review first; saved in ROCKNIX's own settings file, backed up and undoable. Not for a ROCKNIX card in the PC (Windows can't read that part of the card). |
ecalbox.conf` the way Recalbox's own menu saves them, after you review them, and can be undone. |
| Install a system (helper) | ✅ | RDS-027. **Install a system** on Emulation Systems walks you through installing any supported system (ADR-103). **Batocera, RetroPie, Recalbox, ROCKNIX, Lakka** go on a card or USB stick: open the official download page, choose the file you downloaded (RetroDrive checks it and tells you what it's for), optionally check its checksum, then RetroDrive opens Raspberry Pi Imager or balenaEtcher to write it. **RetroBat and ES-DE** (Windows): open the official download page, install or unpack it yourself, then **Find it**. RetroDrive never writes to the card and never downloads, ships or installs a system. |
| Copy games to RetroBat (verified) | ✅ | RDS-010. After you confirm, copies the plan in the background with progress and Cancel. Re-checks the plan, the drive's identity and free space first. Each file goes to a temporary name, is checked with SHA-256 by reading it back from the drive, and only then gets its real name. **Never overwrites or deletes** your files (a file that appears mid-copy is kept). Safety review: docs/18. |
| Same game under another name isn't copied twice | ✅ | The copy plan recognises a game already on the drive (or another version in the same plan) under a different file name, e.g. `10-Yard_Fight(J).zip` next to `10-Yard Fight (Japan).zip`, and doesn't copy it. A list **Already there under another name** shows each one with **Don't copy / Copy anyway**. When several versions are new, the preferred one is copied (good dump, USA/World, Europe, Japan). Measured on the owner's NES library: 1,187 such games. |
| PSP | ✅ | `psp` / `pspminis` folders are PSP and go to `roms\psp` (they were mistaken for PS2). Rescan those folders after updating. |
| Sync | ✅ | **Plan** is where you choose what goes on a drive (it starts with nothing ticked). **Sync** keeps exactly what you copied up to date: the platforms and collections you copied (including games added to them later), never the rest of your library. It re-checks those games (unchanged files aren't re-read), copies new ones, shows games changed on the drive (restore only if you choose; the drive's file is backed up first), and adds your art and details. *Stop syncing something* takes a platform off the list without touching the drive. One summary before it starts. Games you deleted from the drive are **asked about** (*Ask me later / Copy it again / Keep it off this drive*), never copied back silently. Never overwrites or deletes. |
| Choices out of the way | ✅ | Games that need a choice (two versions in your library, a different file on the drive, removed from the drive) are skipped and everything else goes ahead. One line under the summary says how many and links to **Review choices**, which has tabs, a **For all** option (e.g. copy every version from "roms II") and short rows. Your choices are remembered for next time, except *Replace*. Start is right under the summary; no scrolling. |
| Existing files checked by content | ✅ | "Already there" files are compared byte for byte before being skipped; if they differ, the existing file is kept and reported. |
| Cancel and crash recovery | ✅ | Cancel discards only the file in progress. After a crash or unplugged drive, RetroDrive removes its own unfinished temporary files the next time that same drive is connected. Partly copied multi-disc games keep their verified discs; copying again finishes them. |
| Copy record (manifest) | ✅ | Every copy is recorded in the catalog and as `.retrodrive\deployments\<id>.json` in the RetroBat folder. Verified checksums are saved to the catalog. |
| Conflict choices (replace with backup, side-by-side rename) | ✅ | RDS-017: choose to keep, replace (original backed up) or copy both (docs/18 §7). |
| Guided installs and imaging | ⛔ | RDS-012 to RDS-014, gated. |

## Later modules

| Feature | Status | Requirement |
|---|---|---|
| Artwork and metadata scraping with attribution | ⛔ | FR-06, dropped 2026-11-06: RetroDrive is free and won't fetch online art or metadata (ADR-033). |
| BIOS checker (user-supplied files only) | ✅ | FR-07. In **Customize**: which systems in your library need a BIOS, whether you have it (library and that system), with regional alternatives counted. Names from libretro-core-info (MIT, credited); RetroDrive never provides BIOS files. Checks names, not checksums. |
| Controller profiles | 📋 | FR-08 |
| Setup wizards | ✅ | ADR-100, ADR-101. **Dashboard → Quick actions**: **Set up everything** walks you from your games to a ready system, every step inside the wizard: **Your games** → **Find your system** (RetroDrive looks on your drives and for Batocera, Recalbox, RetroPie, ROCKNIX and Lakka on your network; you pick one, or Browse…) → **What to copy** (tick platforms or pick a collection; shows what's copied, what's already there and whether it fits; asks MAME or FinalBurn Neo for arcade games) → **BIOS** (what's missing) → **Copy** (checked, never overwrites, with progress) → **Back up and check** (first backup and the health check). Each task also has its own card: **Add your games**, **Find or add a system**, **Copy games to a system**, **Check BIOS files**, **Back up a system** (the system step is skipped when you have only one). All optional: offered once on first start, with *No thanks, I'll do it myself*; Exit any time. **Controllers and light guns** is its own wizard (ADR-102): *Controller or light gun?* Controller: RetroDrive recognises it, you confirm, it sets it up on your systems. Light gun: find it (plugged in, or pick it), set it up (steps per system, Sinden helper), then the light gun games: one click makes a **Light gun games** collection that fills itself, and **Copy them to a system**. Also **Step by step** on the Controllers page. |
| App looks | ✅ | ADR-104, ADR-105. **Look** switch on the Dashboard: **Classic** (the original), **Galactic** (space opera, the empire's side: a black retro vector display with amber lines and corners, glowing red readouts, indicator lamps, sci-fi headings), **Bridge** (starship console: orange, lilac and sky colour bars, pill buttons, condensed capitals), **Cartoon** (light and sunny: sky and clouds, yellow buttons, thick ink outlines, comic shadows, a bubbly font) and **Versus** (arcade fighter: fire glow, lifebar strips, slanted italic capitals). Original designs, not any film's or series'. Layout never changes; remembered on this PC. |
| Backup and restore of configs and saves | ✅ | FR-09, ADR-090. **Backup** (step 1) in **Emulation Systems** for every system: RetroBat, ES-DE, Batocera, Recalbox, RetroPie, ROCKNIX and Lakka (each system's **Backup** button): **Back up now** copies the system's saves, settings and game lists to a new folder on your PC (`Documents\RetroDrive Backups`, or a folder you choose in **Settings → Backups**). Nothing on the system is changed, and each file is checked after copying. Where RetroPie and ROCKNIX keep saves next to the games, only the save files are taken, never the games. Settings on another network share (RetroPie's configs, ROCKNIX's config, Lakka's shares) are included when RetroDrive can reach them. Optionally include music and boot videos (RetroBat) or downloaded art (ES-DE). See when the last backup was made, open any backup, and **Check** that it's still intact. Games, BIOS, themes and emulators aren't included. Not while RetroBat or ES-DE is running. **Restore** (step 2): **Restore…** on any backup shows what it would do first: missing files are added, identical ones skipped, and for files that differ you choose to keep the system's copies (the default; they may be newer) or use the backup's (the system's copies are moved to `.retrodrive\backup`, never deleted). Pick which groups (e.g. only saves). A backup can also go onto another system of the same kind. A damaged or edited backup restores nothing. Every restore can be **undone**; files you've played since are kept. The preview is quick: files unchanged since the backup are recognised by size and time instead of being read (backups made from 2026-11-09 on; ADR-099). **Save as .zip** (ADR-096) packs any complete backup into one checked `.zip` next to it, to keep on a USB stick or move to another PC; **Add a backup from a .zip** brings one back as a backup you can restore, marked **added from a .zip** in the list (ADR-106) (only the backed-up saves and settings are unpacked; anything else in the zip is ignored). |
| Maintenance checks and repair plans | ✅ | FR-10, ADR-093. **Check health** on each system in **Emulation Systems** looks at six things and changes nothing: **space** (free space, and whether your next Sync fits), **games** (copied before but gone, different from your library, can't be copied, new in your library), **folders** (the system's layout checks), **BIOS**, **backup** (none yet, or older than 30 days) and **leftovers** (unfinished copies, and how much RetroDrive's own backup folder holds). Each finding has a button for the feature that fixes it: Sync, Plan deployment, Back up, the BIOS checker in Customize, or Open backup folder. |
| Arcade Builder | 📋 | FR-11 |
| Optional AI troubleshooting (opt-in, redacted) | 📋 | FR-12 |
| More platforms (Knulli/ROCKNIX, Lakka, …) | ✅ | FR-13. ES-DE, Recalbox, ROCKNIX, Knulli and Lakka are done. |

## Legal and compliance (docs/16)

| Item | Status | Notes |
|---|---|---|
| No games, BIOS, firmware or artwork shipped or downloaded | ✅ | Checked in docs/17 §5. |
| RetroBat support limited to the user's own installation | ✅ | RetroDrive works only with a RetroBat you installed: it checks the folder and copies games into it after you confirm. Downloading or installing RetroBat is blocked on LEGAL-002/003. |
| Personal-use statement (Settings, README) | 🟡 | Draft: RetroDrive is for your own games, for personal use; not for preparing drives or machines for others or for sale (ADR-023). |
| Independence statement in the app (Settings) and README | 🟡 | Draft wording from docs/16 §8, pending counsel review. |
| Third-party component manifest (LEGAL-001) | 🟡 | Software dependencies inventoried (298; 5 MPL-2.0 via Tauri, credited with their source location in the notices file). Platforms and assets are listed by hand. |
| CI license gate (LEGAL-014) | 🟡 | `scripts/check-licenses.py` fails CI on copyleft or unknown licenses; release sign-off still to be defined. |
| Third-party notices file in the installer (LEGAL-009) | ✅ | `THIRD_PARTY_NOTICES.txt` (298 open-source components with their licenses, and where to get the source of the 5 MPL-2.0 ones) and `THIRD_PARTY_CONTROLLERS.txt` (data credits) are installed next to RetroDrive; **Settings → About and legal** opens them (**Third-party notices**, **Data credits**). Generated from the dependency list; CI fails if it falls out of date. |
| Privacy policy (LEGAL-009) | 🟡 | Draft in `docs/legal/PRIVACY.md` (2026-11-09), to be reviewed before the first public release; contact through GitHub Issues, no personal details published. No EULA: RetroDrive is open source (Apache-2.0). The privacy policy matches the app: no account, no telemetry, no network calls of its own; everything stays on your PC and your own drives. |
| Trademark clearance for "RetroDrive", and logo provenance (LEGAL-008) | 📋 | Owner action; the logo is AI-generated (docs/17 §4). |
| RetroBat written permission (LEGAL-003) | ✅ | Not needed for what RetroDrive does (owner decision 2026-11-09, ADR-087): RetroDrive is free and doesn't download, install, bundle or package RetroBat; it only works with a RetroBat the user installed. Would only come back if RetroDrive ever downloaded or installed RetroBat itself (RDS-012, out of scope). |

## Engineering

| Feature | Status | Notes |
|---|---|---|
| Rust unit tests (77): migrations, classifier, paths, long paths, locked files, scanner, disc grouping, cancellation, batching, job log, drive identity, search, filters, duplicates, paging, adapter contract, RetroBat detection and placement, installations and preview | ✅ | `cd src-tauri && cargo test` |
| 100k-file performance benchmark | ✅ | `cargo test --release benchmark_100k -- --ignored --nocapture`; results in docs/15. |
| TypeScript type-check and frontend build | ✅ | `npm run build` |
| GitHub Actions CI on Windows (tests, clippy, build) | ✅ | Runs on every push to `main` and every pull request. |
| Pinned dependency lockfiles | ✅ | `package-lock.json`, `src-tauri/Cargo.lock`. |
| Content Security Policy | ✅ | On since 2026-11-09 (ADR-089): the app's pages may load only RetroDrive's own code, styles and pictures (including your library art) and talk only to RetroDrive itself; no outside connections, frames, plugins or forms. Tested in a release build (library, art, Theme Studio canvas) and in development. |
| Project license | ✅ | **Apache License 2.0** (ADR-094): free and open source; anyone may use, change and share it, keeping the notices. `LICENSE` and `NOTICE` ship with the app, the installer shows the license, and **Settings → About and legal** has a **License** button. Copyright "The RetroDrive contributors"; contact through GitHub Issues. |
