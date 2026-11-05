# 13 | Product Decisions and Open Questions

## Accepted decisions (from product discussions)
| ID | Decision | Rationale |
|---|---|---|
| ADR-001 | Name: RetroDrive Studio, under an earlier publisher name. *Superseded by ADR-017.* | Product identity for continued development |
| ADR-002 | Full retro gaming **management suite**, not merely drive creator | Persistent use beyond initial install |
| ADR-003 | **Hybrid** master library | One view, no forced relocation of existing collections |
| ADR-004 | Windows-first desktop | Local drive operations and initial audience |
| ADR-005 | Tauri/React/TypeScript/Rust/SQLite recommended technical baseline | Desktop UI plus native filesystem services |
| ADR-006 | Batocera and RetroBat first | Covers bootable OS and portable Windows scenarios |
| ADR-007 | Read-only, safety-first incremental development | Avoid unexpected user data loss |
| ADR-008 | Game identity, file location and deployment separate | Enables cross-drive library and offline indexing |
| ADR-009 | Use GitHub and Visual Studio Code | User already has GitHub; source-centric workflow |
| ADR-010 | (2026-11-04) `storage_locations` = one row per volume root; user-authorized folders go in a separate `indexed_roots` table; file paths are stored relative to the volume with `/` | Overlapping scans can't duplicate files. A drive-letter change becomes one `root_path` update once RDS-003 adds stable IDs. Alternative considered: one storage location per scanned folder, which needs re-parenting when folders nest. |
| ADR-011 | (2026-11-04) Schema versioning via `PRAGMA user_version`, with a `VACUUM INTO` backup before every upgrade, and newer catalogs refused | Needs no extra table; the backup is a consistent, openable copy. Affects docs/05 and docs/15 QA-017. |
| ADR-012 | (2026-11-04) Repository is `gollum10/RetroInstaller` (private); working copy kept outside OneDrive | The user chose an existing repo. OneDrive syncing conflicts with Cargo's `target/` folder. |
| ADR-013 | (2026-11-04) Scan jobs: batched commits (500 files / 1 s); cancel keeps saved work but skips missing-file detection; one scan at a time; jobs left `running` become `interrupted` at startup | One long transaction would lose everything on a crash and block job-log writes. Skipping missing detection keeps cancel from ever producing false "missing" results. Running one scan at a time avoids SQLite write contention. Alternative considered: rolling back everything on cancel, which wastes work on large libraries. Affects docs/04, 05, 07 and 15 (QA-002). |
| ADR-014 | (2026-11-04) Volume fingerprint = filesystem serial + total size (`vol:XXXX-XXXX:<bytes>`), or `unc:` + share path; `stable_id` is unique, `root_path` is just the last known mount | The serial survives letter changes and is portable between PCs; the size reduces collisions. The volume GUID is stored but not used as the key, because it's specific to each PC. Alternative considered: physical disk serial, which needs elevated or WMI queries and doesn't separate partitions. Clones sharing a fingerprint is accepted for now. Affects docs/05, 07 and 15 (QA-004). |
| ADR-015 | (2026-11-04) Library search uses SQLite FTS5 with the trigram tokenizer (own-content table plus triggers); pages use offset paging with join order fixed by `CROSS JOIN`; filtered counts are capped at 10,000 | Trigram matches anywhere in a word, which suits ROM titles, with no extra dependency. Without fixed join order, SQLite sorted all 100k rows per page (522 ms → 1.4 ms once fixed). Counting every match of a broad search cost more than the page itself (414 ms → 37 ms with the cap). Alternatives considered: keyset paging (more complex, deferred) and a separate search engine (unnecessary). Trade-off: first-time scans are slower (docs/11 follow-up). |
| ADR-016 | (2026-11-04) Multi-file games: the referencing file (`.m3u`, else `.cue`) is the game's primary file; discs and tracks share its release with `file_role` and `parent_file_id`; grouping runs as a post-walk pass inside the scan transaction; detaching runs only after complete scans; references must stay inside the scanned folder | This matches how emulators load these games (playlist, then cue, then tracks). A post-walk pass handles references in any order. Detaching only after complete scans prevents a cancelled or unreadable scan from splitting games. The containment rule blocks path-traversal tricks in downloaded cue sheets (docs/10). Alternative considered: grouping by file-name patterns like "(Disc 1)", which is unreliable. Affects docs/05, 07 and 15 (QA-007). |
| ADR-017 | (2026-11-04) The product is named **RetroDrive**. an earlier publisher name is removed from the app and the docs, and the RetroDrive logo (`branding/retrodrive-logo-full.png`, with a 384 px copy at `src/assets/`) appears in the sidebar. The installer publisher is set to "RetroDrive". The internal app identifier `com.retrodrive.desktop` is **kept for now** | The user's direction. The identifier decides the catalog folder (`%APPDATA%\com.retrodrive.desktop`), so changing it later needs a one-time catalog move. It isn't shown in the UI, but Tauri would have derived the installer publisher from it, hence the explicit `publisher`. Supersedes ADR-001. The app icon uses only the logo's cartridge emblem (the lettering is unreadable at 16–32 px), on a rounded #01071B square; regenerate with `npx tauri icon branding/retrodrive-icon.png`. Android and iOS outputs are deleted (Windows-first, NFR-08). |

## Recommended, awaiting implementation validation
- Separate privileged helper for imaging, with hardened permissions.
- No sign-in requirement or default network telemetry.
- RetroBat ordinary-folder deployment before raw disk imaging.
- Metadata provider integration with licensing review.

## Open questions to resolve in development
1. What minimum supported Windows build and hardware architectures?
2. Should network share indexing ship in M1 or later?
3. How should the user manage title/region/revision resolution when no external metadata exists?
4. Which metadata providers have acceptable commercial licensing?
5. Which Batocera device/image families are supported by the first installer?
6. What portable RetroBat versions/installer formats are supported?
7. Which filesystem types will future deployment target directly?
8. What should be offered free versus in a Pro tier after product validation?
9. What license will apply to RetroDrive's own code?
10. Should the app provide optional encrypted backups and how should keys be managed?

## Change control
Record each major design change with date, alternatives, security/data migration impact and affected docs. Mark recommendations as proposals until confirmed; don't silently turn roadmap concepts into shipped features.
