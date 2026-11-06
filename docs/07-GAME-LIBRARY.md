# 07 | Master Game Library and Metadata

## Library scope
The catalog is a cross-drive inventory, not a requirement to consolidate media. Sources: local folders, external SSD/HDD/USB, mounted network shares and validated emulation installations. The catalog remains searchable when sources disconnect.

## Library actions
- Add/remove *indexed roots* without deleting files.
- Index paths, size, modification time, format and location.
- Filter by platform, genre, source availability, collection, region/revision, missing artwork and duplicates.
- Add to Favorites, custom collections and curated cabinet sets.
- Inspect and manually correct matches.
- Queue hashes, rechecks and artwork separately from initial fast scans.

## Recognition
Use platform folder semantics, file extension and optional metadata; record inferred confidence and avoid false certainty. Generic files (`.zip`, `.chd`, `.iso`) are ambiguous without context. Support multidisc assets (`.cue/.bin`, `.m3u`) and avoid counting a playlist and its disc files as independent games. Arcade ROM sets may depend on parent/clone archives and specific emulator core versions, so don't 'clean up duplicates' blindly.

### More systems, hack folders and BIOS (2026-11-05)
- **49 systems** are recognized (was 12), including Amstrad CPC/GX4000, Atari 800/ST/2600/5200/7800/Lynx/Jaguar, Commodore 64, Amiga/Amiga 1200/CD32, Sharp X68000, MSX/MSX2, ZX81/ZX Spectrum, PC Engine/CD, SuperGrafx, Nintendo DS, Famicom Disk System, Neo Geo/CD/Pocket/Pocket Color, Intellivision, SG-1000, Odyssey 2, WonderSwan/Color, ColecoVision, Vectrex, Game & Watch, Sega 32X/CD/Saturn, Pokemon Mini, Virtual Boy, Watara Supervision, CPS1/2/3, Dreamcast, Naomi, Atomiswave and 3DO. Folder names include both RetroBat's (`msx1`, `lynx`, `wswanc`, `megacd`…) and other common spellings (`msx`, `atarilynx`, `wonderswancolor`, `segacd`, `sgfx`, `sg-1000`, `neocd`, `coleco`, `odyssey`).
- **Hack folders** count as their system: `nesh`→NES, `snesh`→SNES, `genh` and `megadrive-japan`→Genesis, `gbh`/`gbch`/`gbah`→Game Boy/Color/Advance, `gamegearh`→Game Gear.
- Extensions shared by several computers (`.dsk`, `.tap`, `.rom`, `.cas`, `.hdf`) count only inside a matching system folder; unique ones (`.d64`, `.adf`, `.st`, `.lnx`, `.pce`, `.nds`…) count anywhere.
- **BIOS:** files in a folder named `bios`, or named like `[BIOS] …`, get the system **BIOS**. They are kept out of game lists' system counts (shown last on the Systems page as "files"), and are never placed or copied as games.
- **Rescans move games** whose primary file is now recognized differently (e.g. a `c64` folder that used to be Unclassified) to the new system; search follows. Legacy v0.1 rows keep theirs.
- **Owner's library check (2026-11-05, snapshot):** of 60,558 Unclassified games, 60,554 are now recognized; 4 remain (the `ports` folder). No file became "not a game", so rescans won't mark anything missing.

### Implemented recognition rules (2026-11-04)
`src-tauri/src/classify.rs`:
- **Platform folders:** the nearest folder named after a platform decides, searching from the file's parent up to and including the scanned folder. Inside that folder, only that platform's extensions (plus `.zip` / `.7z` for cartridge systems) count as games. Anything else, such as saves, states, readme files or gamelists, is ignored.
- **`.bin`:** counts only inside Genesis folders. In disc-system folders it is a data track of a `.cue`, and is skipped.
- **Without a platform folder:** an extension that belongs to exactly one platform decides it. `.zip`, `.7z`, `.chd`, `.iso`, `.cue` and `.m3u` are recorded as `Unclassified` with source `ambiguous`.
- **Multi-file games (RDS-005):** after the walk, every `.cue` sheet (`FILE "…" BINARY` lines, quoted or not, any case) and `.m3u` playlist (one path per line, `#` comments) is read.
  - **Grouping:** the referenced files join the referencing file's game as `track` or `disc` (in playlist order). Tracks the walk skipped as unrecognized (e.g. `.bin`) are recorded through the reference.
  - **Safety:** references are resolved relative to the sheet, must stay inside the scanned folder (otherwise ignored with a warning), and files over 1 MB aren't parsed. A missing reference is a warning that increments `missing_parts`, and the game shows **Incomplete**.
  - **Unreadable sheets:** a cue sheet or playlist that can't be read (e.g. locked) is an **error**: the scan is incomplete, so nothing is marked missing or split.
  - **Deleted playlists:** after a complete scan, discs whose playlist no longer references them become separate games again, and their tracks move with them.
  - **Known limitations:** a track dropped from an edited cue sheet is marked missing rather than reclassified, and a manual title edit on a disc is lost when it is grouped.

### Playlists with a leading slash (2026-11-05)
Some playlists (the owner's X68000 sets) list disks as `/Game/disk.dim`, meaning "inside this playlist's folder". A reference starting with a single `/` or `\` (not a network path or drive letter) is first tried relative to the playlist's folder and used **only if that file exists there**; it is still checked against the scanned folder. Before this, those references were refused as outside the folder (122 warnings on the owner's `roms` rescan) and the disks weren't grouped.

### Implemented rescan rules
`src-tauri/src/scanner.rs`:
- **Links:** symbolic links and junctions are never followed, and are reported as skipped.
- **Read errors:** entries that can't be read are reported, and the scan continues with the rest.
- **Missing files:** a previously indexed file under the scanned folder is marked `missing` only if the scan finished with **no** read errors and was **not cancelled**. Catalog rows are never deleted, and a file that reappears becomes `present` again.
- **Partial rescans:** scanning a subfolder only affects files under that subfolder.
- **Offline volumes:** if a file's volume root isn't reachable, *or* that drive letter now holds a volume with a different fingerprint, the file is shown as `offline`. That check overrides `present` / `missing`.
- **Reconnecting under a new letter:** a rescan matches the drive by fingerprint and updates the location's root, so files are neither duplicated nor marked missing. The scan summary reports `remounted_from`.
- **Manual titles:** existing titles are never overwritten by a rescan, so manual corrections survive.
- **Batched saving:** files are committed in batches of 500 or every second. A cancelled or crashed scan keeps what it had already saved; the next full scan completes the picture.

### Leaving files out, and duplicate cleanup (step 5a, 2026-11-05, `duplicates.rs`)
- **Leave out** (any game) marks its primary file `excluded_at` in the catalog. Left-out files are hidden from the library, its counts, the Systems page and duplicate badges, and are **never planned or copied**. They stay on disk untouched and can be brought back (Status: *Left out of library*). Rescans keep the mark.
- **Clean up duplicates** keeps one copy of each duplicate group (same system hint, name ignoring case, size) and leaves the others out (`excluded_reason = 'duplicate'`). The copy kept is present on a connected drive, then has the shortest path, then sorts first; the preview shows examples before anything changes. Refused while a scan or copy runs.
- Same-name files with different sizes are not duplicates; the user can resolve such a clash by leaving one version out (or, in step 5b, keeping both).
- Owner decision (2026-11-05): users decide per duplicate (keep or leave out) so drives don't fill with copies; kept duplicates that clash get `(2)` added when copied (step 5b).

### Implemented duplicate candidates (RDS-004)
- **Rule:** files with the same platform, file name (ignoring case) and non-zero size are flagged as *possible duplicates*: a ×N badge, a filter, and a count on the Dashboard.
- **Categories covered:** this catches category 1 below when two copies are byte-identical, without proving it. It never merges or deletes anything.
- **Not flagged:** the same title across platforms (category 3), and different dumps with different sizes.
- **Next step:** hash-verified grouping needs background hashing (planned).

### Implemented browsing (2026-11-05)
- **Shown title (2026-11-06, schema v14):** each game has a `display_title`: the reference catalog's title when matched (e.g. *'88 Games* for `konami88.zip`), otherwise the file-based title. The title sort, the platform sort's second key and the A–Z bar use it, so the list reads in the order it's shown; search matches file titles, catalog titles and platforms.
- `catalog::system_summary`: games (multi-disc games count once) and total size per platform for the Systems page. It runs only when that page is shown, because summing sizes walks every game; `library_stats` (run on every refresh) stays a plain count.
- Grid and list views use the same paged query (`query_games`, 100 per page).

## Collections (step 7, implemented 2026-11-05, `src-tauri/src/collections.rs`)
- A collection is the user's own named list of games (e.g. "Couch co-op"). It is a **catalog list only**: creating, renaming, emptying or deleting one never touches games or files.
- **Add** one game from its card, or **Add all shown to a collection…** (every game matching the current search and filters, up to 10,000 at a time). A new collection can be created from the same window. Adding a game twice keeps one entry.
- **Collections tab** in the Game Library: a card per collection with its game count and size (left-out games aren't counted); Open shows its games with the usual search, filters and views, where each card offers **Remove from collection**.
- **Copying a collection:** the deployment plan's **What to copy** choice limits the plan to one collection's games (the platform ticks then apply within it). The collection is part of the plan fingerprint, so changing it requires a fresh plan before copying.
- Library queries take `collection_id`; the shared filter builder (`catalog::filter_sql`) is used both for paging and for `matching_file_ids` ("add all shown").
- Not yet: favourites as a built-in collection, ordering within a collection, smart (rule-based) collections.

## Duplicate categories
1. Exact file: identical cryptographic hash.
2. Same release/edition: different dump or compression; manual validation before merge.
3. Same title across platforms: **not** duplicate for deletion.
4. Arcade parent/clone dependency: not safe to infer redundancy from names.

## Game media: box art (implemented 2026-11-05, `src-tauri/src/media.rs`)
RetroDrive **never ships, downloads or copies art**. It links games to images the user already has; only each image's location is stored (`game_media`).
- **During every scan:** image files (`png`, `jpg`, `jpeg`, `webp`, `gif`, `bmp`) are counted as *images found*, not as ignored files. Then, for games reached by the scan: (1) each `gamelist.xml` (EmulationStation format, e.g. written by Skraper/ScreenScraper) links `<path>` to its `<image>` and `<thumbnail>` (`<marquee>` logos aren't used as covers); (2) games without a gamelist entry are matched to images next to the ROM by name: `media/box2dfront|boxart|covers/<name>`, `boxart/`, `covers/`, `media/images|screenshot|screenshots/<name>`, `images/<name>-image`, `images/<name>`, `images/<name>-thumb`, or `<name>.<ext>` beside the ROM.
- **Only images seen inside the scanned folder are linked**; gamelist paths that climb above it are refused. Scanned links are refreshed by each scan; the user's own picks are never touched.
- **Add art by hand:** a game card's **Add art / Change art** picks any image; **Remove my art** forgets the pick (the image file is never touched). The user's pick always wins, then box art, thumbnail, image.
- **Display:** the UI loads images through the `rdmedia` protocol, which serves **only** the best image recorded for the requested game file, only image file types, at most 25 MB, and only when its drive is connected (fingerprint checked). It never serves arbitrary paths. No thumbnails or copies are cached.
- **Measured on the owner's NAS (2026-11-05, separate test catalog):** `roms II\nes`: 1,592 games, 1,520 images found, 1,520 games linked; only 6 files left as non-game.

## Reference catalog (implemented 2026-11-05, `src-tauri/src/reference.rs`)
The user can import their own game catalog (`.xlsx`/`.csv`) in **Settings → Reference catalog**.
- **Columns** are recognized by header name (Title, Platform/System/Console, Year, Genre, Publisher, Developer, Source ID = No-Intro/MAME name, Source full name, Variants, Regions, Game ID). **Art columns ("Art…", image, thumbnail) are never read.** Rows without a title or system are skipped and counted.
- **One catalog at a time:** importing a new one replaces the old one. A file with only *Game ID* and *Genre* columns is a **genre supplement** that fills empty genres (existing genres are kept).
- **Matching** (per library game, by its primary file name without extension): (1) exact No-Intro/MAME name or full name, preferring the game's own system (an Unclassified game accepts a unique match on any system); (2) for arcade entries, a listed MAME clone/regional set; (3) the same title on the same system, ignoring bracket tags, punctuation and "Title, The" order. Ambiguous titles (e.g. two *Monopoly* entries on Genesis) are not matched. Games found by later scans are matched automatically.
- **Display:** name and clone-set matches show the catalog title (clone sets add their set name, e.g. *10-Yard Fight (10yard85)*); title matches keep the file's title. Year and genre appear on cards and in list columns; publisher in the card tooltip.
- **Separation:** reference data lives in its own tables; the library's own titles are never changed, and removing the catalog removes only the links.
- **Measured on the owner's library (2026-11-05):** 63,793 entries imported in about 20 s; 47,840 of 116,405 games matched (24,156 by name, 4,539 by clone set, 19,145 by title): Arcade 93%, NES 80%, Genesis 79%, SNES 72%, Unclassified 4%. The 17,232-row genre supplement added 17,232 genres.
- **One thing at a time:** importing or removing the catalog is refused (and its buttons are greyed out) while a scan or copy runs, and scans/copies are refused while an import runs, because both rewrite catalog tables (found by the owner, 2026-11-05: an import during a scan failed with "database is locked").
- **Not yet:** searching by catalog titles (search still uses file titles), sorting by catalog title, and editing matches by hand.

## Metadata and scraping
**Owner decision (2026-11-06):** RetroDrive is **free**, and it has **no scraper or online metadata/artwork service** (no ScreenScraper, TheGamesDB, IGDB, MobyGames). The owner doesn't want RetroDrive handling copyrighted material. Box art comes only from the user's own images; game details only from a catalog file the user imports. What remains possible: reading the user's own EmulationStation `gamelist.xml` (already used for box art), the user's reference catalog, and a manual editor for corrections.

## BIOS checker
Use externally sourced compatibility requirement data and user-supplied BIOS files only. Match expected hashes where authorized and published; never bundle protected firmware. Distinguish missing requirement from emulator-specific optional BIOS. No false promise that presence alone guarantees working games.

## Offline and rescan rules
Storage `offline` is not `deleted`. On reconnect, verify a stable identity before matching; rerun incremental scan. Only mark a previously indexed file absent after a complete scan of that root succeeds. Archive directory disappearance should not trigger automatic file deletion anywhere.

## Scaling
*Implemented (RDS-004):* 100-row pages, indexed search and sorts, capped filtered counts; see docs/15 for measured timings.

Paginate, stream, background hash and art; index titles, source statuses and hashes; support millions of file paths without loading the whole catalog into React memory. Use removable-device edge-case tests.
