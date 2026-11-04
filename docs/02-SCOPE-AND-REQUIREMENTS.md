# 02 | Scope and Functional Requirements

## Core product modules

| ID | Module | Capabilities | Target phase |
|---|---|---|---|
| FR-01 | Dashboard | Inventory, connected drive state, counts, jobs, warnings, health indicators grounded in checks | 1 |
| FR-02 | Drive Manager | Read-only discovery, volumes, capacity, filesystems, identity, offline detection | 1 |
| FR-03 | Master Library | Index ROM paths, searchable catalog, platform hints, file presence, hashes, dedupe | 1-2 |
| FR-04 | Platform Manager | Adapter catalog and compatibility; Batocera and RetroBat first | 1-3 |
| FR-05 | Deployment Engine | Plan, capacity, file conflicts, staging, pause/cancel, hash verify, manifest | 1-2 |
| FR-06 | Artwork/Metadata | Provider matching, art, descriptions, attribution, manual corrections | 2 |
| FR-07 | BIOS Checker | Validate user-supplied firmware paths/hashes against legal requirements data | 2-3 |
| FR-08 | Controllers | Controller/arcade encoder layouts, templates and validation | 3 |
| FR-09 | Backup and Clone | Versioned config/save backup, verified restore; later safe disk cloning | 3 |
| FR-10 | Maintenance | Health checks, update previews, diagnostics, logs, repair with rollback | 3 |
| FR-11 | Arcade Builder | 2/4-player, vertical and horizontal screens, cabinets and peripherals | 4 |
| FR-12 | Advanced Assistance | Optional AI troubleshooting, with local redaction and explicit opt-in | 4 |
| FR-13 | Platform Expansion | Recalbox, Lakka, ES-DE, RetroArch, EmuDeck, RetroDECK, etc. where appropriate | Future |

## Phase 1 / initial demonstrable vertical slice
1. Tauri desktop shell with navigation and command-result errors.
2. Local SQLite store with migrations and distinct games, file records and storage locations.
3. Read-only mounted-volume discovery; stable drive identity where feasible.
4. User-selected folder indexer, per-file metadata, file availability and path normalization.
5. Basic game search/filter, missing/offline status, duplicate candidates.
6. Batocera and RetroBat adapters for **layout assessment** (not initially formatting/flashing).
7. Deployment preview and capacity estimation, then user-approved staged copy to an ordinary writable destination folder; verify and record manifest.
8. Cancellation, error reporting and a test suite for file operations.

## Explicitly out of phase 1
Automatic partitioning, writing raw disk images, resizing partitions, modifying bootloaders, advanced BIOS/game scraping, platform updates, saves sync, remote management and clone/restore. Future work requires extra safety tests and platform-specific validation.

## Nonfunctional requirements
- NFR-01: No destructive storage operation on default interaction; operations require specific target selection and previews.
- NFR-02: Never follow directory symlinks/junctions outside approved paths without opt-in.
- NFR-03: No unbounded UI blocking during scans and copies; run cancellable jobs.
- NFR-04: Job restart/recovery, filesystem operation logging, versioned migrations.
- NFR-05: Offline-first; no login necessary for local scanning and cataloging.
- NFR-06: Supports large collections with paginated queries, indexed search and incremental scans.
- NFR-07: Accessible keyboard navigation, readable text, progress and error explanations.
- NFR-08: Windows-first; do not promise macOS/Linux support until platform layer is tested.
- NFR-09: Signed/download-verifiable official installers and least-privilege filesystem access.
- NFR-10: Provider licensing, privacy, and data usage settings respected.

## User-facing definition of status
- **Ready** = automated prerequisites and integrity checks passed; not a guarantee gameplay succeeds.
- **Warning** = one or more likely blockers or unknowns.
- **Offline** = indexed storage location currently unavailable, catalog persists.
- **Unverified** = no reliable checksum or validation available.
- **Job failed** = operation rolled back or left safe, explicit cleanup instructions provided.

## Prioritization rule
Safety and recoverability > correctness and metadata integrity > usability > new system breadth > advanced automation.
