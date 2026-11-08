# Theme Studio local working storage (Sprint 1 PR 2)

SQLite is the authoritative working store. The universal 1.0.0 JSON contract is unchanged and remains portable for future .rdtheme packages. Local record revisions/timestamps/drafts are not portable source data. No project folder, assets, frontend installation or compiled output is written by this service.

## Schema v27

The existing backup-first transactional migration adds only `theme_projects` and its recent-project index. Columns: id, indexed name, schema_version, saved_json, nullable draft_json, revision, created_at, updated_at, saved_at. UTC timestamps follow existing catalog conventions. Different projects may share display names; IDs are newly generated UUIDs. SQL constraints enforce valid JSON, matching source/record IDs and versions, active-source/name projection consistency and JavaScript-safe integer revisions.

Legacy designs, design_applications, settings_changes and game records are not converted or rewritten. Upgrading preserves those workflows. An older v26 executable still refuses a v27 catalog by design; compatibility does not mean downgrading the schema. Do not launch experimental builds against a shared live catalog for testing; automated checks use disposable catalogs. The isolated Git worktree does not isolate Tauri's normal app-data location.

## Operations and revision contract

All IPC commands run on `spawn_blocking` and open only RetroDrive's managed catalog. Frontend requests cannot name a database, installation or writable filesystem root. Responses retain raw source strings so future/uneditable data can be shown without manufacturing defaults. Reads do not erase drafts. JSON writes pass the PR 1 size/depth/graph/path/version validation before the transaction. No arbitrary execution or media decoding occurs.

| Tauri command | Arguments (frontend camelCase) | Behavior |
|---|---|---|
| list_theme_projects | limit, offset | Recent summaries; page size 1–100; no full JSON blobs |
| open_theme_project | id | Saved checkpoint, optional draft, revision, timestamps and projected name |
| create_theme_project | documentJson | Validated portable source becomes a new saved project; server assigns UUID/revision 0 |
| write_theme_project | id, revision, documentJson, kind | kind is autosave or checkpoint; SQL compare-and-swap |
| rename_theme_project | id, revision, name | Rename checkpoint and draft together, retaining uncheckpointed content |
| duplicate_theme_project | id, revision, name, useDraft | Save As/duplicate explicitly chooses checkpoint or existing draft; independent new UUID/revision 0 |
| recover_theme_project | id, revision | Return validated draft to resume editing; checkpoint/revision are unchanged |
| discard_theme_project_draft | id, revision | Remove only the draft, restore checkpoint's name projection, advance revision |

TypeScript wrappers are in `src/theme-studio/api.ts`. Responses use camelCase. Errors use `{code, message}` with conflict, invalid_document, invalid_id, invalid_revision, identity_mismatch, invalid_page, not_found, no_draft, storage, catalog or worker codes. Unsupported stored source can be read intact but cannot be renamed, duplicated or cleared through a fallback/default project. A write cannot silently downgrade a stored future schema.

Every successful mutation of an existing project advances its revision. Autosave changes draft/name/updated_at without touching checkpoint/saved_at. Checkpoint save atomically writes saved source and clears the matching draft. A late autosave with the earlier revision is refused. Rename is an explicit checkpoint metadata change and updates saved_at; existing draft-specific edits remain separate. Duplicate copies JSON declarations only; actual asset-store copying remains future work.

The frontend must serialize its pending writes, use the returned revision for the next write, and reconcile conflicts explicitly. Wrappers never automatically retry stale writes. Failed writes return errors after transaction rollback and do not report successful persistence. Save indicators, recovery prompts, navigation-away flushing and user retry controls are PR 3 work; no timers or UI autosave are claimed here.

## Tests and limits

Tests cover create/list/pagination/Unicode names, explicit saves, drafts/recovery/discard, rename/Save As, future-field preservation, stale/reordered writes, simultaneous writers and two connections, insert/update failure rollback, acknowledged drafts across connection loss/reopen, invalid IDs/versions/revisions, SQL invariants, legacy design preservation and untouched synthetic target files. Migration tests verify v26 backup data, retained designs/drafts/history, idempotence and rollback on migration failure; existing earlier-schema/refusal tests remain in the full suite.

Recovery tests simulate connection loss and uncommitted rollback, not hardware power loss. No GUI, real target, exporter, package or live catalog test is claimed. SQLite's existing WAL configuration remains unchanged. No new dependency or disk-mutation authorization is introduced.

Run from repository root: `npm.cmd run build`, `npm.cmd run test:theme-contract`, `python scripts/check-licenses.py`. In `src-tauri`: `cargo test --locked --quiet`, `cargo clippy --locked --all-targets -- -D warnings`.
