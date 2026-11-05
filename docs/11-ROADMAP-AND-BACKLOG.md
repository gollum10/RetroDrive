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
- RDS-009 Build capacity and file-conflict preview; preflight all paths.
- RDS-010 Build transactional staged copier with hash verify, recovery journal and manifest.
- RDS-011 Add user-confirmed deletion/overwrite policy and safety tests.

### M3: Install and guide
- RDS-012 RetroBat portable installation guided workflow and official download validation. **⛔ Blocked on LEGAL-002/003** (RetroBat's written permission for commercial download/install; docs/16 §5).
- RDS-013 Batocera official image selection and bootable-media preflight design. **⛔ Blocked on LEGAL-004.**
- RDS-014 Elevated imaging helper only after independent destructive-disk review and QA. **⛔ Also needs LEGAL-004 and LEGAL-010.**
- RDS-015 First-boot guide and setup verification.

### M4: Rich management
Metadata/artwork matching and attribution; gamelist imports; BIOS checker; profiles; backup/restore; controller setup and maintenance dashboard.

### M5: Advanced suite
Arcade Builder, additional adapters, cloning/migration, optional AI diagnostics, optional secure remote/NAS optimization.

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
5. **Next:** RDS-009, the deployment preview with real capacity (free space minus a reserve) and destination conflict checks (same name, different file), still read-only. Then RDS-010, the first copy: staged, verified and journaled. RDS-008 (Batocera, read-only) can run in parallel.

## Definition of done
Code reviewed; automated tests passing; sample manual Windows test complete; failure/retry behavior tested; documentation updated; no unexpected file mutations; security-sensitive features assessed; versioned migration/backup path documented.
