# 14 | AI Coding Assistant Brief (paste into VS Code assistant)

You are assisting with **RetroDrive**, a Windows-first Tauri v2 + React + TypeScript + Rust + SQLite retro gaming management application.

## Read first
Read `docs/00-START-HERE.md`, `docs/02-SCOPE-AND-REQUIREMENTS.md`, `docs/04-SYSTEM-ARCHITECTURE.md`, `docs/05-DATA-MODEL.md`, `docs/08-DEPLOYMENT-AND-SYNC.md`, `docs/10-SECURITY-PRIVACY-LEGAL.md`, `docs/11-ROADMAP-AND-BACKLOG.md` and the current code.

## Project-wide constraints
1. **Do not claim unimplemented features already work.** Initial scaffold has a simple file-centric DB, read-only mounted-volume listing, basic scans and UI.
2. Keep a **hybrid** library: canonical game, file identity, physical location and deployment are distinct entities. Offline never means deleted.
3. Every disk mutation must be user-initiated, scoped, previewed and logged. Raw imaging, formatting, partitioning and clone operations are **out of scope unless explicitly tasked and safety-reviewed**.
4. Do not bundle or suggest downloads of copyrighted ROMs, protected BIOS or unlicensed artwork. Don't build features that download, install or bundle third-party software (RetroBat, Batocera, emulators, cores) or fetch artwork or metadata from online services (the owner decided against scrapers on 2026-11-06, ADR-033). Add every new dependency or asset to docs/17 and keep `python scripts/check-licenses.py` passing.
5. No arbitrary command execution from user metadata, URL, path or frontend IPC.
6. Prefer minimal, testable patches to rewrites. Use versioned SQLite migrations.
7. Provide exact build/test commands and *actual* executed results. Do not claim Windows tests you did not run.
8. Treat external drive identity and source/destination paths as untrusted and mutable. Handle symlinks, Unicode, file locks, offline changes, insufficient storage and cancellation.
9. Preserve manually edited metadata and existing game files by default.
10. Update the associated design docs and acceptance tests for each significant change.

## Next suggested coding task
M1, RDS-006/007, RDS-009 and RDS-010 are done: read-only RetroBat support, the deployment plan (`deployment.rs`) and the staged, verified copier (`copier.rs`); see docs/04, 08 and the safety review in docs/18. **Any change to `copier.rs` must keep every hazard in docs/18 §2 covered by a test**, and must never add overwrite or delete behavior without a new user-authorized plan and safety review. Replace (step 5b) moves originals to `.retrodrive/backup` and never deletes them (docs/18 §7). Adapter contract is v2 (`PlacementOptions`, e.g. the arcade choice). Never place arcade games without the user's choice, and never copy BIOS files from any target build (e.g. OneSauce). Next candidates are listed in docs/11. Report the plan, risk and test plan before changing files.

## Keep docs in sync
Every change updates [`FEATURES.md`](../FEATURES.md) (the plain-language feature list, for users: keep it short, no ADR numbers or dates), adds a line to [`CHANGELOG.md`](../CHANGELOG.md) (newest first, with the ADR number) and the affected docs here (architecture in 04, schema in 05, rules in 07/08, roadmap status in 11, QA coverage in 15, decisions in 13).

## Required response format from coding assistant
- What was changed and where.
- What remains unimplemented.
- How to run/test it.
- Tests actually executed and their results.
- Known risks and next suggested small step.
