# 11 | Development Roadmap and Backlog

## Releases

### M0: Current scaffold (already provided; not yet independently build-tested)
Navigation, basic read-only mounted volume discovery, folder scan into simple SQLite file catalog and offline-file detection. This is not the normalized design and not a deployer.

### M1: Safe library foundation
- RDS-001 Add migration framework and normalized storage/game/file model. **Done 2026-11-04** (branch `feature/RDS-001-normalized-schema`).
- RDS-002 Implement background paginated scanner with cancellation and progress. **Done 2026-11-04** (branch `feature/RDS-002-scan-jobs`): job log (schema v2), live progress, cancel, batched commits and interrupted-job recovery. Paginating the library *list* moves to RDS-004.
- RDS-003 Distinguish offline root versus removed file; stable root identifiers. **Done 2026-11-04** (branch `feature/RDS-003-stable-volume-ids`): volume fingerprints (schema v3), drive-letter changes recognized, reused letters detected, Known drives view.
- RDS-004 Add search/filter and duplicate candidate views. **Done 2026-11-04** (branch `feature/RDS-004-search-filters-duplicates`): FTS5 trigram search, 100-row pages, platform / status / drive filters, sorts, duplicate candidates, 100k benchmark.
- RDS-005 Add tests for paths, Unicode, discs, archive ambiguity, offline drives. **Done 2026-11-04** (branch `feature/RDS-005-disc-grouping`): `.cue`/`.m3u` grouping (schema v5), Incomplete flag, long-path fix (QA-008), locked-file test (QA-009); 62 Rust tests.

**M1 status: complete.** One follow-up remains: first-scan speed with the search index (below).

### M2: First real game deployment
- RDS-006 Define versioned `PlatformAdapter` interface. **Done 2026-11-04**: contract v1 (read-only) with a mock-adapter test.
- RDS-007 RetroBat adapter: identify an existing installation, validate folders and plan placement. **Done 2026-11-04**: detection, layout checks, folder mapping verified against the RetroBat wiki, saved installations (schema v6), Emulation Systems page and placement preview.
- RDS-008 Batocera adapter: inspect accessible userdata layout, report limitations.
- RDS-009 Build capacity and file-conflict preview; preflight all paths. **Done 2026-11-05**: read-only deployment plan with per-file actions, FAT32 limits, link and clash checks, measured capacity with reserve and staging, platform selection and a plan fingerprint (docs/08, ADR-020).
- RDS-010 Build transactional staged copier with hash verify, recovery journal and manifest. **Done 2026-11-05**: staged temp files, SHA-256 with unbuffered read-back, no-replace rename, journal (schema v7), content check for existing files, cancel, crash cleanup, manifest in the catalog and the installation; safety review docs/18. Real-world run 2026-11-05: 3,436 NES games copied to a real RetroBat 8.3.1.1 on a 2 TB exFAT drive, all verified; the unplug test was waived by the owner.
- RDS-011 Add user-confirmed deletion/overwrite policy and safety tests.
- RDS-016 Arcade choice per installation (MAME or FinalBurn Neo). **Done 2026-11-05** (schema v8, adapter contract v2, ADR-022).
- RDS-017 Conflict and clash choices: for a different existing file, or two library files with the same name (25 pairs in the owner's NES library), let the user pick one version or keep both side by side; replace only with a backup and explicit confirmation (docs/08). Needs a safety review.
- RDS-018 Format check per target: warn when a target won't show a file type (e.g. OneSauce's NES collection accepts only `.nes`, while many libraries are zipped). Unzipping would be a new kind of change and needs its own design and safety review.
- RDS-019 Faster copies of many small files (each file is synced and read back; thousands of tiny ROMs take longer than their size suggests) without weakening verification.

### M3: Install and guide
- RDS-012 RetroBat portable installation guided workflow and official download validation. **⛔ Blocked on LEGAL-002/003** (RetroBat's written permission for commercial download/install; docs/16 §5).
- RDS-013 Batocera official image selection and bootable-media preflight design. **⛔ Blocked on LEGAL-004.**
- RDS-014 Elevated imaging helper only after independent destructive-disk review and QA. **⛔ Also needs LEGAL-004 and LEGAL-010.**
- RDS-015 First-boot guide and setup verification.

### M4: Rich management
Metadata/artwork matching and attribution; gamelist imports; BIOS checker; profiles; backup/restore; controller setup and maintenance dashboard.

### M5: Advanced suite
Arcade Builder, additional adapters, cloning/migration, optional AI diagnostics, optional secure remote/NAS optimization.
- RDS-020 **OneSauce / RetroFE adapter** (AtGames Legends Ultimate): read-only first, reading collections and accepted file types from the drive's own `settings.conf`. **⛔ Blocked on LEGAL-015** (docs/16 §6).
- RDS-021 **Emulator customizer:** per-system emulator/core, display (shaders, bezels), controls and per-game overrides, described by each adapter. Must change the frontend's own settings (RetroBat rewrites many emulator configs at launch; verify first), back up before changing anything, show the exact change, support undo, and get its own safety review (like docs/18).
- **Excluded:** preparing drives or machines with RetroBat or games for others or for sale (docs/16 §5).

### Compliance track (runs alongside the milestones; see docs/16 §13 and §18)
- LEGAL-001 inventory: 🟡 software done (docs/17); add each platform or asset as it is considered.
- LEGAL-014 CI license gate: 🟡 `scripts/check-licenses.py` runs in CI; release sign-off still to be defined.
- **Gates:** metadata and artwork (M4) need LEGAL-007; any download of third-party software needs LEGAL-005 and LEGAL-011; commercial launch needs LEGAL-008, 009, 012 and 013.
- **Owner actions (not code):** LEGAL-002/003 (contact the RetroBat maintainers), 004, 008 (trademark search for "RetroDrive"), 012 and 013.

## Story format
Every GitHub issue should state: persona/problem, user-visible result, constraints, dependencies, implementation notes, tests, documentation updated and acceptance criteria. Label e.g. `area:catalog`, `area:storage`, `area:adapter`, `area:security`, `type:feature`, `type:bug`.

## Immediate recommended order
1. ~~Run current scaffold on actual Windows dev machine and log defects.~~ Done 2026-11-04. Defect found and fixed: Vite's watcher crashed on `src-tauri/target` (EBUSY).
2. ~~Create GitHub repo/CI, pin toolchain and dependency versions.~~ Repo `gollum10/RetroInstaller`, lockfiles committed, Windows CI workflow added.
3. ~~Add tests and normalized schema migrations.~~ RDS-001.
4. ~~Build read-only robust catalog before any writes.~~ M1 done (RDS-001 to RDS-005).
   - **Follow-up found by measurement:** first-time scans got about 4× slower with the search index (39,300 files in 12 s, versus about 3 s before). Option: bulk-load the search index at the end of a scan instead of one trigger per file.
5. ~~RDS-009 deployment plan (read-only).~~ Done 2026-11-05.
6. ~~RDS-010 staged, verified copy.~~ Done 2026-11-05 (docs/18).
7. ~~Arcade choice (RDS-016).~~ Done 2026-11-05.
8. **Owner-approved order (2026-11-05):** ~~(1) Plex-style library: Systems page, grid/list views~~ (done) → ~~(2) import the owner's reference catalog (names, details, genres; no art)~~ (done; follow-ups: search and sort by catalog titles, hand-correcting matches, routing arcade games by their source folder `mame`/`fbneo`) → ~~(3) box art from the user's own images (local only, never shipped)~~ (done: linked during scans from gamelist.xml and folder conventions, plus Add art; follow-ups: game details from gamelist.xml such as descriptions, art in the list view, optional thumbnail speed-up) → ~~(4) ignored files / new platforms~~ (done: 49 systems, hack folders, BIOS kept separate; follow-up: a BIOS checker in M4) → (5a) ~~leave out / duplicate cleanup~~ (done) → (5b) RDS-017 conflict choices (use this version, keep both as `(2)`, replace with backup; safety review addendum) → (6) RDS-019 small-file speed → (7) collections → (8) RDS-008 Batocera → (9) scraper (e.g. ScreenScraper with the user's own account), **blocked on LEGAL-007**.
9. **Earlier candidates:** confirm copied games launch in RetroBat (docs/18 §6), RDS-017 conflict and clash choices, RDS-008 Batocera (read-only), collections, RDS-019 small-file speed.

## Definition of done
Code reviewed; automated tests passing; sample manual Windows test complete; failure/retry behavior tested; documentation updated; no unexpected file mutations; security-sensitive features assessed; versioned migration/backup path documented.
