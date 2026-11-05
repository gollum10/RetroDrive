# 14 | AI Coding Assistant Brief (paste into VS Code assistant)

You are assisting with **RetroDrive**, a Windows-first Tauri v2 + React + TypeScript + Rust + SQLite retro gaming management application.

## Read first
Read `docs/00-START-HERE.md`, `docs/02-SCOPE-AND-REQUIREMENTS.md`, `docs/04-SYSTEM-ARCHITECTURE.md`, `docs/05-DATA-MODEL.md`, `docs/08-DEPLOYMENT-AND-SYNC.md`, `docs/10-SECURITY-PRIVACY-LEGAL.md`, `docs/11-ROADMAP-AND-BACKLOG.md` and the current code.

## Project-wide constraints
1. **Do not claim unimplemented features already work.** Initial scaffold has a simple file-centric DB, read-only mounted-volume listing, basic scans and UI.
2. Keep a **hybrid** library: canonical game, file identity, physical location and deployment are distinct entities. Offline never means deleted.
3. Every disk mutation must be user-initiated, scoped, previewed and logged. Raw imaging, formatting, partitioning and clone operations are **out of scope unless explicitly tasked and safety-reviewed**.
4. Do not bundle or suggest downloads of copyrighted ROMs, protected BIOS or unlicensed artwork. Don't build features that download, install or bundle third-party software (RetroBat, Batocera, emulators, cores) or fetch artwork or metadata until the matching LEGAL item in docs/16 is resolved. Add every new dependency or asset to docs/17 and keep `python scripts/check-licenses.py` passing.
5. No arbitrary command execution from user metadata, URL, path or frontend IPC.
6. Prefer minimal, testable patches to rewrites. Use versioned SQLite migrations.
7. Provide exact build/test commands and *actual* executed results. Do not claim Windows tests you did not run.
8. Treat external drive identity and source/destination paths as untrusted and mutable. Handle symlinks, Unicode, file locks, offline changes, insufficient storage and cancellation.
9. Preserve manually edited metadata and existing game files by default.
10. Update the associated design docs and acceptance tests for each significant change.

## Next suggested coding task
M1, RDS-006/007 and RDS-009 are done (adapter contract v1, read-only RetroBat support, read-only deployment plan in `deployment.rs`); see docs/04, 06 and 08. Next is `RDS-010`, the first feature that **writes**: a staged copier (temp file in the destination folder, streamed hash, verify, atomic rename), a recovery journal and a manifest. It must rebuild the plan and match its fingerprint, re-check the destination's volume identity (QA-015) and free space immediately before copying, never overwrite (conflicts are kept), and compare contents before treating a same-size file as already there. **Get the user's explicit go-ahead and do a dedicated safety review before writing any copy code** (AGENTS.md). Report the plan, risk and test plan first.

## Keep docs in sync
Every change updates [`Features.md`](../Features.md) (feature status) and the affected docs here (architecture in 04, schema in 05, rules in 07/08, roadmap status in 11, QA coverage in 15, decisions in 13).

## Required response format from coding assistant
- What was changed and where.
- What remains unimplemented.
- How to run/test it.
- Tests actually executed and their results.
- Known risks and next suggested small step.
