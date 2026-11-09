# 09 | Profiles, Maintenance, Backup and Repair

## Profiles
Portable configuration objects, versioned by schema and adapter. Initial templates: Basic Batocera, Portable RetroBat, Classic Consoles, Four-Player Arcade, Vertical Arcade, Travel USB. Profiles include settings *intent* and adapter-specific applicability. Preview all generated changes; never copy incompatible emulator config wholesale.

## Controller center (future)
Discover USB and compatible controllers (Xbox, PlayStation, generic USB and arcade encoders). Templates for 2/4 players, trackballs, spinners and light guns where supported. Provide guided button identification with platform-specific mapping and backup of existing config. Controllers and drivers can differ; no universal one-click guarantee.

## Maintenance checks
- Storage available and writable/readable as required.
- Indexed file missing, corrupt or hash changed.
- Manifest differs from destination.
- Required folder/playlist structure invalid.
- BIOS validation incomplete or incompatible for a known platform.
- Supported upstream update available, downloaded from verified origin.
- Backup missing/aged; configuration differs from last good snapshot.

## Repair model
Each fix produces a **repair plan** with: diagnostic evidence, files/config touched, expected impact, backup action, permissions, estimated disk use, user confirmation, rollback recipe and post-check. Prefer non-mutating advice whenever evidence is uncertain.

## Backups
**Step 1 implemented 2026-11-09 (ADR-090, docs/18 §56):** RetroBat and ES-DE backups of saves, settings and game lists into a new folder on the PC, with a SHA-256 manifest and **Check**. Restore (step 2) is next and needs its own safety review.

Back up configuration, saves, gamelists, manifests and user metadata separately from bulk ROMs. Record included paths, hash manifest, date, source adapter/version, restore prerequisites and verified status. Restore is always a previewed operation, with conflict handling.

## Updates
Differentiate application updates from emulator/frontend OS updates. Consult official sources, release compatibility, target disk safety, and backup first. Never silently apply updates that rewrite a boot partition or config.

## Optional AI diagnostics (phase 4)
Local triage first. External AI assistance must be explicitly enabled and should redact machine names, local paths, usernames and sensitive logs before sending. AI suggests repair plans but cannot directly authorize disk-changing actions.
