# 03 | Information Architecture and UX Specification

## Navigation
Dashboard · My Library · Drives · Systems · Deployments · Artwork Studio · Profiles · Maintenance · Settings. Advanced sections may be marked 'Coming later' rather than faking functionality.

## Dashboard
Cards for indexed games, available game files, managed installations and active jobs. List storage locations (mounted/offline), actual capacity from OS, supported-platform status, warnings and recent operations. Never invent a device health score; compute and explain discrete checks or say unknown.

## My Library
Search by title and platform, filter by source, status, genre/favorites and duplicates. Grid/list toggle; game detail with identity, file versions, disc set, checksum, sources, deployments, artwork, metadata provenance, and readiness assessment. Actions: rescan, add to collection, plan deployment, correct match. Maintain game even if all copies offline.

**Implemented 2026-11-05 (Plex-style browsing):** the Game Library opens on **Systems**, one card per platform with game count and total size (Unclassified last); clicking a card opens **Games** filtered to that system. Games show as a **grid** (cover tiles; a coloured placeholder with the title's initials until box art exists) or a **list** (the table). System cards are in **alphabetical order** (Unclassified and BIOS last) with an **A–Z bar** that jumps to the first system for a letter (letters without systems are dimmed); the Games view has an **A–Z bar** (All, #, A–Z) that shows only titles starting with that letter (# = digits and symbols), using the title index (owner request, 2026-11-05). The chosen tab and view are remembered per computer (browser storage; falls back to defaults if unavailable).

**Implemented 2026-11-05 (collections, step 7):** a **Collections** tab next to Systems and Games, with a card per collection (Open, Rename, Delete; delete asks twice and says the games stay in the library) and a "New collection" box. Game cards have **Add to collection**; inside a collection they also have **Remove from collection**. **Add all shown to a collection…** adds every game matching the current filters. The add window lists collections and can create one on the spot.

**Implemented 2026-11-06 (Backups):** each saved installation has a **Backups** button listing the files replaced there (game, path, when, where the original is and its size) and the backup folder's total size. **Undo replace** asks twice ("Click again to undo"); rows that can't be undone say why (drive not connected, original missing, file changed). Undone rows show where RetroDrive's copy went. **Open backup folder** opens File Explorer; the screen says RetroDrive never deletes backups.

**Implemented 2026-11-06 (Emulator settings, RetroBat):** an **Emulator settings** button on RetroBat installations opens a table of systems (those with games or existing settings; "Show all" lists every system) with Emulator (emulator: core), Shader, Bezel, Aspect ratio and Integer scaling, each with AUTO. **Review changes (N)** lists exact before → after values; **Apply** asks twice and is disabled while RetroBat runs (with a *Check again* button). **Changes made** lists past changes with **Undo** (asks twice) or why undo isn't possible.

**Implemented 2026-11-06 (Themes, RetroBat):** a **Themes** button on RetroBat installations shows a card per theme (color swatches, size and files, credits, *In use*), with **Remix** and **Use in RetroBat**. The remix editor has a name, a color picker per color setting (alpha kept; reset per color), *Choose your picture…* per picture setting, *Replace with my font…* per font file, and a live approximate preview (system carousel and game list in the chosen colors, picture and font). Saving says how much is copied. *Use in RetroBat* shows the change (theme before → after), applies on a second click while RetroBat is closed, and is undone from Emulator settings → Changes made.

## Drives
Inventory: manufacturer/model, bus type when known, capacity, filesystem, free space, label, stable identity, mount point, safety classification (system/unknown/removable). Selected drive details: partitions, managed installations, activity, backup state. No prominent 'Erase' control in default view. Clearly distinguish physical disks from mounted volumes.

## Systems
Catalog of Batocera, RetroBat, and later platforms. Cards: bootable OS vs frontend vs Windows portable application; supported hardware; version/source; setup vs existing-install recognition. Show prerequisites, external links, disk implications, licensing and hardware constraints. Use official download sites, never assume all systems flash like ISOs.

## Deployment Wizard
1. Source: games, one collection, or entire filtered selection. *(Implemented: "What to copy" in the plan chooses platforms or one collection, 2026-11-05.)*
2. Destination: existing supported installation or authorized local folder.
3. Analyze: capacity, formats, BIOS requirements (informational), paths, duplicates.
4. Preview: precise adds/replaces/skips, bytes, file conflicts and backup actions.
5. Confirm: typed target name for destructive overwrite cases.
6. Progress: throughput, bytes, current file, pause/cancel, errors.
7. Summary: copied/skipped/failed, checksums, generated manifest, next steps.

## Artwork Studio
Provider configuration and rate-limit notice, match confidence, side-by-side suggested matches, editable artwork and metadata, batch status and manual override. Respect attribution and media licensing.

## Profiles
Profile library: Classic Consoles, Four-Player Arcade, Vertical Arcade, Travel USB; show scope, supported adapters and compatibility. Profiles must not silently overwrite system configs.

## Maintenance
Separated checks: drive reachability, missing references, file hash mismatch, installation layout, missing user BIOS, config changes, update availability. All repair actions show what changes, how to back up, and how to undo.

## Global interaction rules
- No dangerous operation from a single click.
- Always show source and destination together when copying.
- Never describe offline files as deleted.
- Explicitly identify unsupported/unknown statuses.
- Progress must be cancellable where technically safe; explain when cancellation is deferred until an atomic boundary.
- Offer basic and advanced modes, both using the same safety layer.
- Prefer neutral progress/health language to arbitrary scores.

## Desktop visual direction
Dark, polished retro-tech palette; readable contrast; restrained arcade accent colors, clear status icons and generous spacing. Avoid neon text on dark backgrounds without accessibility checks. No copyrighted console art in shipped demo unless properly licensed.

## Screen states to implement for every area
Empty, loading, populated, offline, error, warning, job-running, interrupted/recoverable. Show human-readable remediation and detailed logs on demand.
