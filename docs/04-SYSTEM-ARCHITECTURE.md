# 04 | Technical Architecture

## Baseline stack
- **Windows desktop:** Tauri v2, React, TypeScript, Vite.
- **Native services:** Rust, Tauri commands/events; no arbitrary shell execution from frontend.
- **Database:** SQLite in app-data folder; migrations and transactional writes.
- **UI state:** React with query/pagination patterns for large libraries.
- **External sources:** official upstream release/download APIs as supported; user-authorized metadata provider APIs.
- **Optional future:** NAS via mapped drive/SMB and authenticated remote agents, not required for local operation.

## Logical view
```mermaid
flowchart TD
 UI[React Desktop UI] -->|Typed IPC| APP[Rust Application Services]
 APP --> CAT[Catalog and Metadata]
 APP --> DRV[Storage Discovery]
 APP --> JOB[Job Engine]
 APP --> ADP[Platform Adapter Registry]
 CAT --> DB[(SQLite)]
 DRV --> FS[Approved Local and Network Paths]
 JOB --> FS
 JOB --> MAN[Deployment Manifests and Logs]
 ADP --> BAT[Batocera Adapter]
 ADP --> RET[RetroBat Adapter]
 ADP --> MORE[Future Adapters]
 APP --> PROVIDER[Opt-in External APIs]
```

## Modules/interfaces
- `storage`: read-only physical/volume discovery, stable IDs, mount status, access permissions and protected-disk tagging.
- `catalog`: indexed roots, file metadata, game identity, deduplication and matching confidence.
- `platforms`: `PlatformAdapter` with `detect`, `capabilities`, `validateLayout`, `planFilePlacement`, `renderMetadata` and, later, `install`/`upgrade` gates.
- `deployment`: immutable plan, target validation, staging, verify, commit, resumable job journal.
- `metadata`: provider client, rate-limit/caching, provenance and manual editing.
- `maintenance`: check registry, diagnostics, repair plans, backup-first operations.
- `jobs`: state machine and cancellations; events for UI progress; durable log.
- `settings`: provider keys stored through platform secure storage as feasible; user-approved roots and options.

## Platform adapter contract (conceptual)
```typescript
interface PlatformAdapter {
  id: string;
  displayName: string;
  kind: 'bootable-os' | 'portable-app' | 'frontend';
  detectInstallation(root: string): Promise<DetectionResult>;
  validateLayout(root: string): Promise<ValidationResult>;
  planGameFiles(request: DeploymentRequest): Promise<PlannedFile[]>;
  capabilities(): AdapterCapabilities;
}
```
Installation and updates intentionally require independent, more privileged interfaces and separate threat reviews. Adapter IDs/versioning must remain stable across schema migrations.

## Job lifecycle
`queued -> preflight -> awaiting_confirmation -> running -> verifying -> committing -> completed` with branches to `paused`, `cancelling`, `cancelled`, `failed`, `recovery_required`. Persist state at safe boundaries so a restart can identify unfinished jobs. Write temporary files in target filesystem when possible; atomic rename on completion when supported; no manifest success before verification.

## Trust boundaries
Frontend is unprivileged. Rust services validate all command arguments. A future high-privilege disk helper, if needed, must use a narrow allowlist, per-operation user approval, a verified physical disk identity and no generic command interpreter.

## Performance and reliability
- Paginate game lists; index title/platform/status.
- Use incremental enumeration and background hash queues, not hash every multi-terabyte collection during first import.
- Store relative paths where possible with stable root identifiers.
- Handle Windows long paths, case-insensitive comparisons, network interruptions and filesystem permissions.
- Never update catalog to reflect a write until verified.

## Implemented today (updated 2026-11-04, RDS-001)
Native backend modules in `src-tauri/src/`:

| Module | Responsibility | Target module above |
|---|---|---|
| `lib.rs` | Tauri setup and the commands `list_games`, `scan_library`, `list_drives`. The catalog is migrated once at startup; if that fails, every command returns the error. | app services |
| `db.rs` | Opens connections (WAL, foreign keys, busy timeout). Versioned migrations via `PRAGMA user_version`, with a backup first and v0.1 data conversion. | `catalog` |
| `scanner.rs` | Read-only folder indexer that returns a `ScanSummary` (counts plus issues). | `catalog` / `jobs` |
| `classify.rs` | Infers the platform from folder names and extensions, and records where the hint came from. | `catalog` |
| `catalog.rs` | Library listing with per-file availability (`online` / `offline` / `missing`). | `catalog` |
| `paths.rs` | Path normalization without `\\?\`, plus volume-root and relative-path splitting. | `storage` |
| `drives.rs` | Read-only volume listing; classifies volumes as local, removable or network. | `storage` |

Behavior that differs from the target design, and is still open:
- **Scans** run on a worker thread (`spawn_blocking`) but are not yet durable jobs: no progress events, cancellation, or `jobs` table (RDS-002).
- **Connections:** each command opens its own SQLite connection, with no pool.
- **Commands** return errors as plain strings; structured error codes are still to come.
- **Listing** returns the whole catalog; pagination is part of RDS-004.
- **Availability** comes from the last scan, plus one reachability check per volume. Listing never checks individual files.
- **Security policy:** `tauri.conf.json` still has `csp: null`.
