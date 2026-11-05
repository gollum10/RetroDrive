# 14 | AI Coding Assistant Brief (paste into VS Code assistant)

You are assisting with **RetroDrive Studio**, a an earlier publisher name Windows-first Tauri v2 + React + TypeScript + Rust + SQLite retro gaming management application.

## Read first
Read `docs/00-START-HERE.md`, `docs/02-SCOPE-AND-REQUIREMENTS.md`, `docs/04-SYSTEM-ARCHITECTURE.md`, `docs/05-DATA-MODEL.md`, `docs/08-DEPLOYMENT-AND-SYNC.md`, `docs/10-SECURITY-PRIVACY-LEGAL.md`, `docs/11-ROADMAP-AND-BACKLOG.md` and the current code.

## Project-wide constraints
1. **Do not claim unimplemented features already work.** Initial scaffold has a simple file-centric DB, read-only mounted-volume listing, basic scans and UI.
2. Keep a **hybrid** library: canonical game, file identity, physical location and deployment are distinct entities. Offline never means deleted.
3. Every disk mutation must be user-initiated, scoped, previewed and logged. Raw imaging, formatting, partitioning and clone operations are **out of scope unless explicitly tasked and safety-reviewed**.
4. Do not bundle or suggest downloads of copyrighted ROMs, protected BIOS or unlicensed artwork.
5. No arbitrary command execution from user metadata, URL, path or frontend IPC.
6. Prefer minimal, testable patches to rewrites. Use versioned SQLite migrations.
7. Provide exact build/test commands and *actual* executed results. Do not claim Windows tests you did not run.
8. Treat external drive identity and source/destination paths as untrusted and mutable. Handle symlinks, Unicode, file locks, offline changes, insufficient storage and cancellation.
9. Preserve manually edited metadata and existing game files by default.
10. Update the associated design docs and acceptance tests for each significant change.

## Next suggested coding task
RDS-001 (schema and migrations) and RDS-002 (scan jobs, progress, cancel) are done; see docs/05 and docs/04. Next is `RDS-003`: stable volume identity, so a drive that comes back under a different letter is matched to its existing `storage_locations` row (volume serial or GUID into `stable_id`), with confirmation when the match is ambiguous (QA-004, QA-015). Before changing files, report the plan, risk and test plan.

## Keep docs in sync
Every change updates [`Features.md`](../Features.md) (feature status) and the affected docs here (architecture in 04, schema in 05, rules in 07/08, roadmap status in 11, QA coverage in 15, decisions in 13).

## Required response format from coding assistant
- What was changed and where.
- What remains unimplemented.
- How to run/test it.
- Tests actually executed and their results.
- Known risks and next suggested small step.
