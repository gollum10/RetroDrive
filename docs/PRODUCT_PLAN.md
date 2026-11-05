# RetroDrive Studio — Product Plan v1.0

## Mission
Make creation, configuration, library management, and maintenance of retro gaming drives accessible and safe.

## Architecture
Windows-first Tauri 2 + React/TypeScript + Rust + SQLite, using isolated per-platform adapters. Offline-first design.

## Hybrid Library Model
1. Game identity: canonical game and platform metadata.
2. Game assets: ROMs, disc images, artwork, videos and firmware validation references.
3. Storage location: mounted local/USB/network physical paths, tracked independently from catalog.
4. Installations: target emulation platforms, versions and folder rules.
5. Deployments: manifests of files installed onto a destination, with integrity status and transaction history.

Existing source files must not be silently moved or deleted. Missing drives are marked offline, preserving catalog records.

## Milestones
### M1 — Working Foundation
- [x] UI shell with navigation (starter)
- [x] SQLite file index (starter)
- [x] Read-only mounted volume discovery (starter)
- [x] Import from folder, basic classification (starter)
- [x] Normalize game identity from physical assets (schema v1, RDS-001; matching/grouping still open)
- [ ] Platform-specific Batocera and RetroBat folder adapters
- [ ] Cancellable scan jobs and import progress
- [ ] Deployment preview with free-space and compatibility checks
- [ ] Verified transactional copy with resume/recovery
- [x] Unit tests and automated CI (25 Rust tests; Windows CI workflow)

### M2 — Library Studio
- [ ] Game identity matching, disc sets, duplicates and hashes
- [ ] Metadata provider integrations with approved rate limits/terms
- [ ] Collection creation and filters
- [ ] Existing ES-DE/EmulationStation gamelist integration

### M3 — Maintenance
- [ ] BIOS validation (never bundle copyrighted files)
- [ ] Controller profiles, backups, migration, safe updates
- [ ] File conflict resolution and restore support

### M4 — Advanced
- [ ] Arcade Builder, remote management, optional guided diagnostics

## Safety acceptance criteria
- Never automatically choose a disk for wiping.
- Never offer a system disk as an installation destination without robust protections.
- Require visible capacity/model identity, backup warning and final explicit confirmation before partitioning/flashing.
- Check official image authenticity and integrity.
- Never automatically delete originals on source/destination reconciliation.
- Record operation and failure history.

## Immediate acceptance test
Launch the Windows app, import a sample ROM folder, view imported games, disconnect a removable location and verify imported records remain with `Offline` status. No files in the imported folder should change.
