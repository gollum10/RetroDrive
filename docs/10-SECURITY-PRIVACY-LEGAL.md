# 10 | Security, Privacy and Legal Safeguards

## Safety principles
1. Read-only discovery and previews before mutation.
2. Least privilege; isolate privileged operations behind a narrow audited helper only when needed.
3. Source/destination binding to stable disk identities, not letters alone.
4. Users must explicitly opt in to data transfers and destructive changes.
5. Checksums, backup-first recovery and persisted operation history.
6. No bundled copyrighted ROMs or BIOS; legitimate user-supplied files and homebrew only.

## Threat model highlights
- **Wrong disk selected:** system disk blacklist, serial/model verification, revalidation, typed confirmation.
- **Path traversal/links:** canonicalize within approved roots; restrict symlink/junction escape; prevent UNC surprises.
- **Malicious archives:** parse in restricted process, size/decompression-bomb limits, no execution of supplied content.
- **Compromised downloads:** HTTPS plus publisher-origin validation and upstream checksum/signature verification where available.
- **Supply chain:** dependency pinning and audit, signed release artifacts, reproducible build aspirations, secure CI.
- **API secrets:** don't store raw provider passwords in plain SQLite; leverage OS secure credential storage where appropriate.
- **Metadata licensing:** obey API quotas, content attribution, media rights and caching terms.
- **Ransomware-like behavior:** no background mass overwrite or deletion; bounded and logged writes only for user-approved targets.

## Privacy defaults
No account needed. The local catalog can reveal sensitive file paths and user collections; keep it local, don't upload without explicit opt-in. Optional telemetry: clearly described and disabled by default initially. Provide export/delete catalog controls that don't delete ROM files. *(Done 2026-11-09: export and removing entries, ADR-097; a library-only reset with the old catalog saved and Put back, ADR-098. Settings → Library catalog.)*

## Copyright/legal
The detailed legal assessment, compliance backlog and release gates are in [docs/16](16-LEGAL-AND-COMPLIANCE.md); the component inventory is in [docs/17](17-THIRD-PARTY-COMPONENTS.md).

The product orchestrates existing emulation software but does not grant users ROM distribution rights or rights to console firmware. Review each project's license, trademark rules, dependencies and commercial redistribution policy before bundling. Links to authorized official downloads are generally preferable to repackaging components. No unlicensed game artwork bundled in starter assets.

## File writes (RDS-010)
Copying games is the only write path today. Its hazards, mitigations, tests and owner sign-off are in [docs/18](18-SAFETY-REVIEW-RDS-010.md). It never overwrites or deletes user files, never writes through links, and runs only after the user confirms a reviewed plan.

## Security release gates
No raw disk write without documented QA on sacrificial media and safe failures for OS disk, disconnected target, device swap, insufficient space, bad image/hash and power interruption. Code review for every elevated operation. Published installer should be signed when distribution begins.

**AI helpers (2026-11-10, ADR-119):** optional and off until the user connects. A Claude key is kept only in Windows Credential Manager and checked once with Anthropic; a private AI must be on this PC or a private network. Library names, folders and settings are never sent. See docs/18 §70.

**Ask AI (2026-11-10, ADR-120):** sends one screen's layers and the words typed, only when the user presses Ask; never pictures, file names, paths, games or settings. Suggestions are checked and undoable. At most 50 requests a day. See docs/18 §71.
