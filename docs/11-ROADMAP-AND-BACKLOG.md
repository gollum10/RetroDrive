# 11 | Development Roadmap and Backlog

## Releases

### M0: Current scaffold (already provided; not yet independently build-tested)
Navigation, basic read-only mounted volume discovery, folder scan into simple SQLite file catalog and offline-file detection. This is not the normalized design and not a deployer.

### M1: Safe library foundation
- RDS-001 Add migration framework and normalized storage/game/file model. **Done 2026-11-04** (branch `feature/RDS-001-normalized-schema`).
- RDS-002 Implement background paginated scanner with cancellation and progress. *Partial:* the scan now runs off the UI thread; progress, cancellation and the jobs table are still open.
- RDS-003 Distinguish offline root versus removed file; stable root identifiers. *Partial:* `offline` (volume unreachable) and `missing` (not found by a complete scan) are now separate. Stable volume IDs are still open.
- RDS-004 Add search/filter and duplicate candidate views.
- RDS-005 Add tests for paths, Unicode, discs, archive ambiguity, offline drives. *Partial:* 25 Rust unit tests cover paths, Unicode, archive ambiguity, disc tracks, missing and offline states, and migrations. `.cue/.m3u` grouping is still open.

### M2: First real game deployment
- RDS-006 Define versioned `PlatformAdapter` interface.
- RDS-007 RetroBat adapter: identify an existing installation, validate folders and plan placement.
- RDS-008 Batocera adapter: inspect accessible userdata layout, report limitations.
- RDS-009 Build capacity and file-conflict preview; preflight all paths.
- RDS-010 Build transactional staged copier with hash verify, recovery journal and manifest.
- RDS-011 Add user-confirmed deletion/overwrite policy and safety tests.

### M3: Install and guide
- RDS-012 RetroBat portable installation guided workflow and official download validation.
- RDS-013 Batocera official image selection and bootable-media preflight design.
- RDS-014 Elevated imaging helper only after independent destructive-disk review and QA.
- RDS-015 First-boot guide and setup verification.

### M4: Rich management
Metadata/artwork matching and attribution; gamelist imports; BIOS checker; profiles; backup/restore; controller setup and maintenance dashboard.

### M5: Advanced suite
Arcade Builder, additional adapters, cloning/migration, optional AI diagnostics, optional secure remote/NAS optimization.

## Story format
Every GitHub issue should state: persona/problem, user-visible result, constraints, dependencies, implementation notes, tests, documentation updated and acceptance criteria. Label e.g. `area:catalog`, `area:storage`, `area:adapter`, `area:security`, `type:feature`, `type:bug`.

## Immediate recommended order
1. ~~Run current scaffold on actual Windows dev machine and log defects.~~ Done 2026-11-04. Defect found and fixed: Vite's watcher crashed on `src-tauri/target` (EBUSY).
2. ~~Create GitHub repo/CI, pin toolchain and dependency versions.~~ Repo `gollum10/RetroInstaller`, lockfiles committed, Windows CI workflow added.
3. ~~Add tests and normalized schema migrations.~~ RDS-001.
4. Build read-only robust catalog before any writes. Next up: RDS-002 (jobs, progress, cancel), RDS-003 (stable volume IDs), RDS-004 (paginated search, duplicates).
5. Add RetroBat folder deployment as the first end-to-end data-moving feature.

## Definition of done
Code reviewed; automated tests passing; sample manual Windows test complete; failure/retry behavior tested; documentation updated; no unexpected file mutations; security-sensitive features assessed; versioned migration/backup path documented.
