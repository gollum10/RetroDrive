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

### Implemented duplicate candidates (RDS-004)
- **Rule:** files with the same platform, file name (ignoring case) and non-zero size are flagged as *possible duplicates*: a ×N badge, a filter, and a count on the Dashboard.
- **Categories covered:** this catches category 1 below when two copies are byte-identical, without proving it. It never merges or deletes anything.
- **Not flagged:** the same title across platforms (category 3), and different dumps with different sizes.
- **Next step:** hash-verified grouping needs background hashing (planned).

## Duplicate categories
1. Exact file: identical cryptographic hash.
2. Same release/edition: different dump or compression; manual validation before merge.
3. Same title across platforms: **not** duplicate for deletion.
4. Arcade parent/clone dependency: not safe to infer redundancy from names.

## Metadata and scraping
Provide approved providers (e.g. ScreenScraper with compatible account and terms) and import existing EmulationStation gamelist metadata. Store attribution/source ID/confidence; cache respectfully; respect API limits, licensing, permitted commercial usage, and removal requests. Manual editor allows corrections and local image selection.

## BIOS checker
Use externally sourced compatibility requirement data and user-supplied BIOS files only. Match expected hashes where authorized and published; never bundle protected firmware. Distinguish missing requirement from emulator-specific optional BIOS. No false promise that presence alone guarantees working games.

## Offline and rescan rules
Storage `offline` is not `deleted`. On reconnect, verify a stable identity before matching; rerun incremental scan. Only mark a previously indexed file absent after a complete scan of that root succeeds. Archive directory disappearance should not trigger automatic file deletion anywhere.

## Scaling
*Implemented (RDS-004):* 100-row pages, indexed search and sorts, capped filtered counts; see docs/15 for measured timings.

Paginate, stream, background hash and art; index titles, source statuses and hashes; support millions of file paths without loading the whole catalog into React memory. Use removable-device edge-case tests.
