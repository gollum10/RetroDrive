# 08 | Deployment, Capacity Planning and Synchronization

## Use case
Select collections (e.g. Arcade Classics, NES, SNES, Genesis) from any indexed sources and deploy to a supported target installation or folder after a truthful capacity/compatibility preview.

## Pipeline
```mermaid
flowchart LR
 A[Select games] --> B[Resolve source files]
 B --> C[Adapter placement plan]
 C --> D[Capacity / conflicts / BIOS warnings]
 D --> E{User confirms plan?}
 E -->|No| X[No change]
 E -->|Yes| F[Stage files]
 F --> G[Verify hashes]
 G --> H[Atomic commit and manifest]
 H --> I[Summary and recovery data]
```

## Deployment plan contains
- Source file identity and last-observed state; destination installation identity and root.
- Full set of desired destination paths, add/replace/skip/conflict actions, exact bytes.
- Adapter version, paths, required directory creation, supported formats, filesystem maximum file size limitations.
- Capacity calculation from **current** measured free space minus reserve and temporary staging overhead; recheck immediately before copy.
- Conflicts and symlink/case-path collisions; current source/destination snapshots and plan hash.

## Implemented plan rules (RDS-009, `src-tauri/src/deployment.rs`, ADR-020)
The plan is read-only: it reads catalog rows and destination metadata and never creates, copies or changes anything.

| Check | Result |
|---|---|
| Destination relative path is empty, absolute, or has `.`, `..` or `:` parts | **Blocked** (outside the installation) |
| Source path is the destination (the installation's own folder was scanned) | **Already in place** |
| Several library files want one destination (case-insensitive): same size | First one copies (the in-place one wins); the rest are **Duplicate** |
| ...different sizes | All **Blocked** (nobody can pick a winner automatically) |
| File larger than the filesystem allows (FAT32 4 GiB − 1; FAT/FAT16 2 GiB − 1) | **Blocked** |
| A folder on the way is a symlink or junction | **Blocked**: never write through links |
| A file sits where a folder is needed, or a folder/link where the file goes | **Blocked** |
| Destination file exists with the same size | **Already there** (skipped). Content isn't compared in the plan; RDS-010 verifies before skipping |
| Destination file exists with a different size | **Conflict**: the existing file is kept |
| Otherwise | **Copy** |

A game takes its most serious file action, so a game with one conflicting file isn't partly copied. Full paths over 259 characters are counted as a warning (some emulators can't open them).

**Capacity:** `required = Σ copy files rounded up to whole clusters + one cluster per new folder + the largest copy file (staging/retry room)`. It fits when `required ≤ free − reserve`, where `reserve = max(1 GiB, 2% of the drive)` and `free` is what Windows lets this user write (quotas included). An empty plan always fits; if the drive can't be measured, "fits" is unknown, never yes.

**Selection:** the plan lists every system folder with placeable games; the user picks a subset (collections come later).

**Fingerprint:** a stable 64-bit FNV-1a hash over adapter, contract, installation, and every source, destination, size and action. RDS-010 must rebuild the plan right before copying and refuse to start if the fingerprint changed.

## Transaction behavior
Stage temporary files in destination where feasible; stream chunks and hashes; atomically rename completed files where supported; journal each item. The full multi-file job cannot always be globally atomic, so clearly mark partial completion and support recovery/cleanup. Never mark a deployment item completed before verification.

## Conflict options
- Keep destination (default when contents differ).
- Skip this item.
- Replace after optional backup and explicit confirmation.
- Rename to side-by-side path if supported by destination adapter.
Never make delete-on-sync the default.

## Synchronization semantics
- New source game: offer deployment to user-selected targets; optional opt-in automation later.
- Removed source game: preserve destination copy unless explicitly requested.
- Metadata changed: plan compatible metadata changes, avoid trampling hand edits.
- Drive disconnected: pause and resume after identity confirmation.
- Library differences: show three-way comparison when previous manifest exists; don't infer which copy should win solely from modified time.
- Job restart: revalidate source/destination identities, partial files and plan before continuing.

## Save-game synchronization is separate
Saves can diverge on multiple devices. Design save metadata/versioning, backup snapshots and explicit per-conflict choices; never automatically overwrite a newer or otherwise different save based only on timestamp. Future scope.

## Example acceptance tests
- 100 files, including names with Unicode and spaces, deploy with every verified hash matching.
- A deliberate destination content conflict is displayed and preserved by default.
- Pulling an external drive mid-copy leaves only temporary files, no false 'complete' entries.
- Source and destination cannot be the same path in a destructive replacement plan.
- A target filesystem with per-file size limit prevents impossible deployments.
- Capacity estimator refuses insufficient space, factoring in staging overhead.
