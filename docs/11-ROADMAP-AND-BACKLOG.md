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
- RDS-008 Batocera adapter: inspect accessible userdata layout, report limitations. **Done 2026-11-06 (read-only)**: detection of the game folder, boot-partition and drive-root hints, layout checks, placement preview with Batocera folders; copying is off until its own safety review (docs/06, docs/18 §9). Follow-up: RDS-022 copy to Batocera.
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
- RDS-022 **Copy to Batocera**: **done 2026-11-07** (game drive and network share; share id; docs/18 §21; real SMB test through `\\localhost`). Not yet tried on a real Batocera or an exFAT drive. Next: Sync, art and details for Batocera (step 2), then RDS-024 settings and themes (step 3), then RetroPie (RDS-023).
- **Game art and details in RetroBat (owner request 2026-11-07: "apply those to RetroBat and every emulator if it allows"):** **done** for RetroBat (box art, logos, catalog details; when copying and from Customize; RetroBat's own art kept). Batocera/RetroPie use the same `gamelist.xml` format, so this extends to them with their connectors (RDS-022..024). Art is never shipped with RetroDrive; only the user's own images are copied to the user's own drive.
- **Found in owner testing 2026-11-07, fixed:** PSP games placed as PS2 (PSP is now its own platform); the same games copied again under a second naming style (the plan now skips a game already on the drive under another name, ADR-042). Not matched yet: 8-letter DOS-style names (`CITYCON`) and numeral differences (`Castlevania2` vs `Castlevania II`).
- **Sync (owner request 2026-11-07):** **done** for RetroBat (ADR-043). Later: Sync for Batocera once copying there is reviewed (RDS-022); optional reminder when new games appear in the library.
- **Choices out of the way (owner feedback 2026-11-07):** **done** (ADR-044): *Review choices* link, "for all" choices, saved choices, Start under the summary.
- **Plan chooses, Sync maintains (owner correction 2026-11-07):** **done** (ADR-045). Later: same-size changes on the drive are found during the sync's content check and reported, not yet offered as *Restore*.
- **Choose which songs play (owner request 2026-11-07: "turn off only the music I want"):** **done** through RetroBat's favorites playlist from a design's Music tab.
- **Designs (owner request 2026-11-06: "design, then finish or abandon… then it becomes selectable in another screen"; "can you choose when/where music is played"):** **done**: design studio with Finish/Abandon and saved drafts, My designs, apply as one previewed and undoable step, music settings including music per system. Next: share/export a design file, apply to several installations, Batocera/RetroPie connectors applying the same design (RDS-023/024).
- **Customize page (owner request 2026-11-06: "include as many aspects about the system as the system allows… boot video, the overall theme"; "add their own video and music"):** **done** as one workspace item with Look / Behavior / Emulators tabs, boot video, own video and music, curated retrobat.ini options, shared change history. Next candidates (each verified in RetroBat's files first): screensaver and its timing, menu sounds, language, theme options (Carbon's subsets such as system view, colorset), splash screen image, game list options, per-game settings, controls.
- **Theme builder (owner request 2026-11-06: "creating themes and also changing the settings"):** step 1 **done** (remix an installed RetroBat theme: colors, background picture, fonts, preview, switch with undo; docs/18 §13). RDS-026 step 2: build new themes from RetroDrive's own original layouts (no logos or art shipped; system names as text).
- **Connectors for more platforms (owner request 2026-11-06).** The adapter contract and the customizer were built to extend; each platform needs its facts verified from its own docs and source first, like RetroBat and Batocera.
  - RDS-024 **Batocera settings and themes:** per-system settings live in `batocera.conf` (`<system>.emulator`, `.core`, `.shaderset`, `.bezel`, `.ratio`, …: believed to match RetroBat's keys, to verify); themes in `userdata/themes`, same theme format (Carbon is shared). Writing over the network share needs RDS-022's safety review first.
  - RDS-023 **RetroPie:** read-only first. Games in `~/RetroPie/roms/<system>` (Samba share `\\RETROPIE\roms`); emulator choices in `/opt/retropie/configs/<system>/emulators.cfg`; classic EmulationStation themes and `es_settings.cfg`. All to verify against RetroPie's docs; the SD card's Linux partition isn't readable from Windows, so access is through the share.
  - RDS-025 **Others:** Recalbox (`recalbox.conf`), ES-DE (Windows/Linux frontend), Knulli/ROCKNIX handhelds, Lakka. Ordered by owner interest.
- RDS-020 **OneSauce / RetroFE adapter** (AtGames Legends Ultimate): read-only first, reading collections and accepted file types from the drive's own `settings.conf`. **⛔ Blocked on LEGAL-015** (docs/16 §6).
- RDS-021 **Version 1 done 2026-11-06** (per-system emulator/core, shader, bezel, aspect ratio, integer scaling in RetroBat's es_settings.cfg, with review, backup and undo; docs/18 §12). Later: per-game settings, controls, advanced options, Batocera.
- RDS-021 (original scope) **Emulator customizer:** per-system emulator/core, display (shaders, bezels), controls and per-game overrides, described by each adapter. Must change the frontend's own settings (RetroBat rewrites many emulator configs at launch; verify first), back up before changing anything, show the exact change, support undo, and get its own safety review (like docs/18).
- **Excluded:** preparing drives or machines with RetroBat or games for others or for sale (docs/16 §5).

### Compliance track (runs alongside the milestones; see docs/16 §13 and §18)
- LEGAL-001 inventory: 🟡 software done (docs/17); add each platform or asset as it is considered.
- LEGAL-014 CI license gate: 🟡 `scripts/check-licenses.py` runs in CI; release sign-off still to be defined.
- **Gates:** online metadata and artwork are not planned (owner decision 2026-11-06; LEGAL-007 closed); any download of third-party software needs LEGAL-005 and LEGAL-011; commercial launch needs LEGAL-008, 009, 012 and 013.
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
8. **Owner-approved order (2026-11-05):** ~~(1) Plex-style library: Systems page, grid/list views~~ (done) → ~~(2) import the owner's reference catalog (names, details, genres; no art)~~ (done; follow-ups: search and sort by catalog titles, hand-correcting matches, routing arcade games by their source folder `mame`/`fbneo`) → ~~(3) box art from the user's own images (local only, never shipped)~~ (done: linked during scans from gamelist.xml and folder conventions, plus Add art; follow-ups: game details from gamelist.xml such as descriptions, art in the list view, optional thumbnail speed-up) → ~~(4) ignored files / new platforms~~ (done: 49 systems, hack folders, BIOS kept separate; follow-up: a BIOS checker in M4) → (5a) ~~leave out / duplicate cleanup~~ (done) → ~~(5b) RDS-017 conflict choices~~ (done: docs/18 §7; follow-up: restore-from-backup and clear-old-backups screen) → ~~(6) RDS-019 small-file speed~~ (done: 1.7× faster; follow-up: copy several files at once) → ~~(7) collections~~ (done: Collections tab, add one or all shown, copy a collection; follow-ups: favourites, ordering, smart collections) → ~~(8) RDS-008 Batocera~~ (done, preview only; follow-up RDS-022: copying to Batocera after its own safety review) → ~~(9) scraper~~ (**dropped 2026-11-06**: RetroDrive is free and won't fetch online metadata or art). **Next (owner, 2026-11-06), one PR each:** search and sort by catalog title (done: schema v14, plus faster library pages) → ~~restore-from-backup screen~~ (done: Backups with undo replace, docs/18 §10) → ~~copy several files at once~~ (done: 4 games at once, about 3.7× faster for small files; docs/18 §11) → ~~emulator customizer (RDS-021)~~ (version 1 done: per-system emulator/core, shader, bezel, ratio, integer scaling; docs/18 §12).
9. **Earlier candidates:** confirm copied games launch in RetroBat (docs/18 §6), RDS-017 conflict and clash choices, RDS-008 Batocera (read-only), collections, RDS-019 small-file speed.

## Definition of done
Code reviewed; automated tests passing; sample manual Windows test complete; failure/retry behavior tested; documentation updated; no unexpected file mutations; security-sensitive features assessed; versioned migration/backup path documented.
