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
Paginate, stream, background hash and art; index titles, source statuses and hashes; support millions of file paths without loading the whole catalog into React memory. Use removable-device edge-case tests.
