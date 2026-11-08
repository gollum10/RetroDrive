# Theme Studio: technical gap analysis and Sprint 1 proposal

Date: 2026-11-08. Status: **architecture and four-PR approach approved; PR 1 implementation under review**.

Owner amendments: SQLite stays the authoritative local working store; portable JSON excludes machine paths. The wizard explicitly offers Universal / No Target Yet. Future UI separates scene/layers, canvas, inspector and timeline. Implementation is isolated in `.worktrees/theme-studio` on `feature/theme-studio-schema`, preserving both prior checkouts. PR 1 adds source contract/validation only; see [contract](theme-studio/project-format.md) and [tracking](theme-studio/requirements.md). No merge to main without authorization.

## Baseline and scope

- Repository: `gollum10/RetroInstaller`. Initial worktree was clean on `main`.
- Fetched `origin`; local main was two commits behind. Created the previously absent `feature/theme-studio` at `origin/main` (`a0a7b89`). Local main was not moved. No implementation changes, commits, pushes, or PRs were made.
- The requested master document was absent from `docs/`. Read the complete supplied Downloads copy and copied it unchanged to `docs/RetroDrive_Theme_Studio_Master_Design_v1.md` so the proposed work has its intended source of truth.
- The master file's header says “Proposed design baseline.” The owner's current request explicitly approves it and selects Option B; this request governs. Illustrative schemas, suggested file layouts, and external capability assumptions still require engineering validation.
- Read AGENTS.md, the AI brief and design index, architecture, safety review through addendum 53, Features.md, and supporting scope, data-model, deployment, security and roadmap documents. Reviewed Customize, themes, designs, customizer, database, app navigation, IPC, platform detection and CI.
- Existing documentation includes stale scaffold descriptions and historical statuses. Findings below use source and executed tests as evidence; shipped labels alone do not prove Theme Studio capabilities.

## Technical gap analysis

| Master requirements | Verified current implementation | Gap and proposed approach |
|---|---|---|
| Separate workspace; PROJ-001/004, LIB-001 | `src/App.tsx` uses a Page union, ordered `menus`, and conditional workspace rendering. Customize is a standalone component. | Add Theme Studio immediately after Customize. Give it its own state and project browser. Keep Customize and its commands operational. |
| Universal versioned project; PROJ-001/004, NFR-003 | `designs.rs::DesignSpec` stores a RetroBat theme selection/remix, INI settings, music, media paths and emulator settings. `create` fixes platform to RetroBat. | Introduce a distinct universal document and project service. Existing DesignSpec is neither a scene graph nor a portable theme. Do not repurpose or bulk-convert it. |
| Save, recovery, undo; PROJ-002/003/005 | Customize debounces `save_design_draft` by 400 ms; database retains draft and finished JSON. Finish/Abandon exists. | New revision-aware autosave/checkpoint semantics, recovery UI and editor command history. Existing debounce has no general undo stack or Theme Studio recovery protocol. |
| Visual authoring; VIS-001–008 | `ThemeMockup` renders hard-coded approximate system/game screens using chosen colors, picture and font. | Canvas, scene selection, layer tree, inspector, transforms, guides, grouping, tokens, responsive layouts and accessibility inspection are new work. Start from original blank/starter scenes, independent of installed themes. |
| Scenes, bindings, overrides; SCENE-001–007 | No universal scene/navigation model in the reviewed modules. Catalog contains titles, reference metadata, descriptions, players/rating and media links. | Stable scene/node IDs, explicit units, typed bindings and strict override precedence. Read-only LibraryBridge later supplies bounded sample data and offline placeholders. |
| Assets; PROJ-007, MEDIA-001/002/003/007 | `themes.rs` reads theme variables and subset defaults, lists fonts, copies user pictures/fonts into a new remix. `media.rs` and rdmedia serve recorded library images. | Controlled content-addressed asset store, provenance/licenses, usage references and format validation. Existing preview accepts an arbitrary supplied path with extension and 25 MB checks; it is not a project sandbox or content validator. WebP is absent from this theme preview allowlist; SVG safety needs separate handling. |
| Media and animation; MEDIA-004–008, ANIM-001–005 | Customize can add MP4 video and OGG/MP3 music; designs can select tracks and music settings. | No media probe/transcoder, trim editor, synchronized timeline, SFX authoring, intro compositor or keyframe engine. Preserve extension data now; add typed features and target fallbacks incrementally. Any FFmpeg dependency requires license/component review. |
| Input/native preview; PREV-001/002/004/005 | Approximate preview is labeled. Controllers module detects devices and confirms styles. | Controller detection is not a preview input simulator. Add keyboard/gamepad focus/back behavior later, then separately controlled native target preview. Never imply editor output is native verified behavior. |
| Exact target compatibility; PREV-003/007, DEPLOY-001 | Existing PlatformAdapter contract v3 covers detection, layout, game placement and copy capability. RetroBat reads version.info; Batocera game-share detection reports unknown version. | Separate theme adapter registry and capability rules keyed to frontend fork/version/platform/theme schema. A RetroBat release number alone does not prove the bundled engine's feature support. Unknown stays unknown; no wildcard support claims. |
| Compiler/export; DEPLOY-002/009, NFR-002 | `themes.rs` edits an existing theme's variable text/subsets, using formatVersion 7 in generated subset XML. Remix metadata is `retrodrive-theme`, not the proposed `retrodrive.theme` source package. | New normalized IR, validation, lowering, deterministic output and build manifest. Source projects, compiled output and installed files have separate identities. RetroBat first; Batocera next; ES-DE independently assessed later. |
| Install/recovery; DEPLOY-003–008 | customizer supplies journaled text-file changes, backups, SHA checks at callers, read-back checks, no-replace renames, undo and startup recovery. designs applies additions then settings; its undo retains added files. Copier is game-plan-specific. | Reuse proven primitives and identity checks, not the entire game copier or design workflow as a theme installer. Need a theme-specific immutable plan, complete staged-folder verification, multi-file recovery journal, retained backup/previous selection and explicit activation. Existing per-file undo is not a globally atomic theme transaction. |
| Packaging/import/sharing; LIB-002–005, DEPLOY-009 | No .rdtheme package workflow in reviewed source. Remix preserves theme credits. | New bounded archive parser/writer, path and payload security, manifests, license/privacy audit, safe import copies and separate native archives. Public gallery remains P2. Never automatically package game/BIOS files or library art/music. |
| Offline, performance, security; NFR-001–010 | React/TS/Vite + Rust/Tauri v2 + SQLite; blocking work commonly uses spawn_blocking; job registry/cancellation exists for scans/copies. CSP remains null. | Keep stack. Add bounded theme jobs when needed, project-specific IPC validation and safe rendering. Retain all future work in requirement tracking; no second UI framework, arbitrary scripts, online scraping or cloud prerequisite. |

## Proposed architecture decisions

1. React workspace and editor state under `src/theme-studio/`; pure Rust domain, project persistence, validation and adapter interfaces under `src-tauri/src/theme_studio/`.
2. Existing installation adapters remain unchanged. A ThemeAdapter consumes an explicitly evidenced TargetProfile and validated IR, returning generated relative files and a manifest. It never mutates installations. A later DeploymentService owns approved mutations.
3. Working storage is managed by Rust beneath app data, using opaque project IDs. Frontend IPC sends IDs and revisions, not arbitrary writable roots. Use the package's logical `project/theme.json`, assets, previews and attribution structure when materializing/exporting a working folder.
4. SQLite is the authoritative source document store in Sprint 1. This deliberately avoids two competing authoritative copies between database and filesystem. A later working-folder/package materializer writes a verified snapshot; it does not change the saved source project. Assets will live in controlled project storage, not JSON blobs.
5. JSON contract includes schemaVersion, stable project/scene/node IDs, metadata, explicit dimensions/units, scenes, nodes, bindings, variants, tokens, assets and target intents. Establish inheritance order from the master: global styles → theme variant → aspect → system → component. Include non-executable extension storage for future animations, video, audio, navigation, components and sharing metadata.
6. Preserve unknown object fields and unknown node payloads on round-trip; unfamiliar required features are read-only or block lowering. Newer unsupported major schema versions are refused for editing with an explanatory error. Do not silently deserialize away fields as the legacy typed DesignSpec path can do.
7. Keep local absolute library references outside portable source/package data. Later bridges distinguish workspace copies from local reference bindings, with an export audit and offline handling.

## Sprint 1 goal and boundary

Deliver an offline Theme Studio project workspace: create a universal project without a connected installation or base theme, edit metadata, save/open/duplicate it, autosave and recover it, inspect starter scenes and target-support status. Establish the adapter contract and evidence matrix before claiming native export support.

This is a foundation increment, not the completed P0 release or full visual designer. Sprint 1 has no installation, activation, target file replacement, source archive import, asset execution, media transcoding or public sharing. Canvas manipulation and a working native exporter follow in the next increments. No disabled feature is labeled shipped.

### Small PR sequence

| PR | Work and files | Required evidence |
|---|---|---|
| 1: universal document contract | New `theme_studio/mod.rs`, `model.rs`, `validation.rs`; `src/theme-studio/types.ts`; `src-tauri/tests/fixtures/theme_studio/` with blank/starter, invalid and future-extension documents. Add a project JSON contract in `docs/theme-studio/project-format.md`. | Rust/TS wire shape agrees via shared fixtures. Round-trips preserve extension data. Stable IDs, explicit units, finite bounded numeric values, unique IDs, valid parent/child references, no cycles, safe asset paths and typed binding identifiers are validated. No filesystem or installation writes. |
| 2: project persistence and recovery | New `theme_studio/projects.rs`, `commands.rs`; modify `db.rs` and `lib.rs`; add migration/project integration tests. | Additive migration, CRUD/checkpoints/autosave/conflict recovery all pass; existing designs/history are unchanged. Source survives simulated interrupted save and failed transactions. |
| 3: workspace and project browser | New `src/ThemeStudio.tsx`, `src/theme-studio/ProjectBrowser.tsx`, `NewProjectWizard.tsx`, `ProjectOverview.tsx`, `api.ts`, `theme-studio.css`; modify `src/App.tsx` only at navigation/import/render integration. | Theme Studio sits directly below Customize. New blank/original starter project, metadata, target intent, display choice, scene outline, Save/Save As, Duplicate and recovery offer work offline. Honest saved/saving/error indicators; keyboard labels/focus; no dependency on an installation. |
| 4: capability boundary and discovery record | New `theme_studio/capabilities.rs`, `adapters/mod.rs`; fixtures for exact/unknown/mismatched profiles; `docs/theme-studio/target-matrix.md`; status display in overview. | Pure mock adapter proves target separation and native/convert/degraded/unsupported/unknown issue shape. Read-only discovery maps existing installation facts without inventing renderer versions. Record first RetroBat fixture provenance and remaining native verification work. No fake production exporter. |

Each PR includes its tests and relevant documentation. Sequence PRs against the feature integration branch where practical, keeping prerequisite diffs small. Do not merge any PR into main without the owner's authorization. No implementation PR is created by this planning task.

### New and modified files beyond implementation

- Planning/spec files in this task: the unchanged master design and this proposal only.
- During approved implementation, update `Features.md`, `docs/00-START-HERE.md`, `03-UX-AND-SCREENS.md`, `04-SYSTEM-ARCHITECTURE.md`, `05-DATA-MODEL.md`, `11-ROADMAP-AND-BACKLOG.md`, `13-DECISION-LOG.md`, `15-QUALITY-AND-TESTS.md` and `17-THIRD-PARTY-COMPONENTS.md` where affected.
- Add `docs/theme-studio/requirements.md` with every master requirement mapped to sprint/phase, current status, evidence and acceptance gate; include original backlog TS-001–020 mappings. Add `docs/theme-studio/security.md` covering trust boundaries and future installer/package reviews.
- Existing `Customize.tsx`, `themes.rs`, `designs.rs`, `customizer.rs`, `copier.rs`, game schema and platform adapters require no production behavior changes in Sprint 1. Add compatibility tests beside their existing tests where appropriate.
- No runtime dependency is currently needed for the project foundation. A frontend test harness is a proposed dev dependency: Vitest + Testing Library/jsdom, pinned for the existing Vite version after license/compatibility review. If adopted, modify `package.json`, lockfile, test config, docs/17 and CI to run it. Select versions at implementation time, not from unverified assumptions.

## Database proposal

Current source schema is v26. Reserve v27 for this work, using the next free version if another feature lands first. Apply through the existing backup-first, single-transaction migration framework.

Add **one table**, `theme_projects`:

| Field | Purpose |
|---|---|
| `id TEXT PRIMARY KEY` | Opaque project ID, not a path or project name. |
| `name TEXT NOT NULL` | Display name; normalized and length-bounded by service. Different projects may share names. |
| `schema_version TEXT NOT NULL` | Universal document schema version, independent of SQLite user_version. |
| `saved_json TEXT NOT NULL` | Last explicit save/checkpoint, including complete metadata and universal content. |
| `draft_json TEXT NULL` | Last successfully persisted autosave; distinct from explicit save. |
| `revision INTEGER NOT NULL` | Monotonic optimistic concurrency token. Every successful save/autosave advances it. |
| `created_at`, `updated_at`, `saved_at` | UTC timestamps. |

Add an index on updated_at/id for paged recent-project listing and checks for nonnegative revision and valid stored JSON. Project JSON stays authoritative for metadata beyond the indexed name; update projections atomically. IDs in JSON and table must match.

- Create writes a valid starter saved document in one transaction. Autosave updates draft and revision only when expected revision matches. Explicit save advances the checkpoint and clears the matching draft atomically.
- Open reports saved and recoverable draft versions; user may recover or discard the draft. A save failure never reports success or erases the last valid checkpoint. Selecting another project or leaving the workspace waits for pending persistence or displays a retry decision.
- Duplicate/Save As uses a new project identity and independent revision state. Preserve internal scene/node IDs because references are project-scoped; never share mutable draft state.
- No asset, build, deployment, timeline or snapshot-history tables yet. Add them only with their feature's transaction/recovery requirements.
- Do not alter or rewrite `designs`, `design_applications` or `settings_changes`, or change their IDs, JSON, timestamps or history. No implicit legacy conversion. A later user-requested conversion would create a copy with clear unsupported-feature warnings.
- Compatibility means the upgraded app preserves old data and workflows. It does not mean an older executable can open v27: the existing newer-schema refusal remains the safe behavior. Document that distinction.

## Tests and acceptance criteria

### Automated coverage required for Sprint 1

1. Schema fixtures: valid blank and starter documents; scenes for startup, systems, games and details plus launch/idle/exit design roles; unknown fields/node payloads preserved; unsupported schema errors; duplicate IDs, graph cycles, invalid units/numbers, broken references and unsafe paths rejected. Main-menu concepts remain represented in requirements and refined before a navigation editor ships.
2. Persistence: create/list/open/save/rename/Save As/duplicate, optimistic revision conflicts, stale autosave arriving after explicit save, rollback on failure, database reopen after interruption, recover/discard draft and UTF-8 names. No silent last-writer-wins data loss.
3. Migration: fresh database, v26→new schema, repeat migration, earlier supported upgrade paths, newer catalog refusal and backup preservation. Seed finished legacy designs, draft designs and application/settings history; verify all old values survive and legacy operations still work.
4. Isolation: project commands cannot name writable target paths; snapshot synthetic RetroBat/Batocera installations before and after every Sprint 1 workflow and assert unchanged. Validate project IDs and payload size/depth limits at Rust IPC boundaries.
5. UI: sidebar order and both workspace routes; no-install empty state; wizard validation; project create/reopen/duplicate; autosave/recovery/conflict/error state; leaving during pending save; Customize smoke coverage; keyboard focus and labels. Mock IPC for component tests, then run a Windows Tauri smoke test with a disposable catalog.
6. Adapters: same feature may produce different outcomes per exact target profile; unknown or mismatched versions remain unknown; no install/compile capability appears just because a game-copy adapter exists. Preserve source data when reporting unsupported features.

### Sprint exit demo

- Start offline with no attached frontend. Open Theme Studio below Customize.
- Create an original blank/starter universal project; choose intended RetroBat target and a display preset without asserting verified target compatibility.
- Edit project metadata, save, close and reopen with identical source content; duplicate independently.
- Simulate an interrupted session after acknowledged autosave; relaunch and recover the draft while retaining the explicit checkpoint.
- View starter scenes and an honest compatibility status. Future media/navigation/animation data survives a round-trip fixture.
- Open Customize and exercise existing saved-design/theme workflows against fixtures; existing database rows and target files remain intact.
- All relevant CI checks and documented Windows smoke checks pass before declaring Sprint 1 complete. A working WYSIWYG editor, native export and deployment remain explicitly unimplemented at that point.

### Commands for implementation PR validation

From repository root: `npm.cmd ci` when dependency lockfiles change; `npm.cmd run build`; `npm.cmd run test -- --run` after the proposed test harness exists; `python scripts/check-licenses.py`.

From `src-tauri`: `cargo test --locked`; `cargo clippy --locked --all-targets -- -D warnings`. Use targeted tests during development, then full Windows CI at PR submission. Never run ignored tests that access real installations without checking their scope and authorization.

## Remaining roadmap and gates

| Increment | Features retained from full master scope |
|---|---|
| Next authoring slices | Canvas select/move/resize, layers, inspector, undo/redo, secure local asset import/browser, library sample bindings, templates and style tokens. Then complete grouping/alignment/rotation/conditional properties, aspect presets and override resolution. |
| First target export | Verified RetroBat frontend/version fixtures; normalized IR; capability analyzer; simple system/gamelist golden outputs; broken-reference blocking, deterministic manifests and offline export. Follow with remaining supported scene mappings. Native screenshot/smoke comparison is a separate evidence gate. |
| Installation | Dedicated theme deployment safety review and owner-approved scoped plan before new target mutations: dry-run diff, identity/free-space recheck, staging, backup, activation consent, full verification, failure injection and restart rollback. Never delete backups or touch boot partitions automatically. |
| Second target | Batocera-specific adapter, exact version evidence, native tests and scoped deployment review. Do not assume RetroBat XML or release evidence applies. Unknown share-reported versions require explicit evidence/review. |
| Rich authoring | Video/audio editing and optimization, intro composition, navigation graph, controller simulation, timelines/keyframes, easing/triggers, reduced motion, responsive variants, reusable components and snapshots/compare. Fallback rendering only when the target supports that media in the same context. |
| Packaging/hardening | Separate editable .rdtheme and native archives; safe package import, license/privacy audit, local theme library, sharing/export and round-trip checks, hostile archive/SVG/XML tests, accessibility and measured hardware budgets. Packaging and P0 keyboard/gamepad preview must ship before a complete P0 release, regardless of later phase labels. |
| Expansion | Independently versioned ES-DE exporter after RetroBat and Batocera; optional authenticated remote deployment/credential storage. Public gallery, ratings and advanced renderer/state machine remain P2/future scope. |

The design's priority list and suggested phase roadmap differ in places: Batocera deployment, source packaging and controller preview are P0 but scheduled later in its phases. Track P0 completion separately from sprint completion. Sprint 1 approval does not waive those requirements.

## Open evidence and risks

- First candidate RetroBat release is 8.3.1.1 because repository detection tests and historical real-world notes mention it. This is not a verified Theme Studio exporter target. Capture actual bundled ES fork/version/theme format and sanitized representative fixtures before enabling export.
- Batocera version is unavailable from current SHARE detection. Exact release/platform test matrix and hardware budgets remain unverified. ES-DE requires a separate assessment.
- No external frontend documentation was revalidated in this local gap analysis; the master's reference list is research input for the adapter work, not evidence of implementation support.
- Source schema must support future work without pretending to implement media/timeline semantics early. Preserve extensions and require explicit compiler support instead of adding unused service stubs for the whole specification.
- Existing settings writer is a useful primitive but assumes callers perform validation and stale-review checks. Any later reuse must preserve those checks and the full hazards in docs/18. Multi-file theme install/recovery needs its own tests and scoped review.
- New frontend testing dependencies require license review; no canvas engine, ZIP package library or FFmpeg bundle is selected in this sprint.

## Checks actually executed during planning

Windows workspace, 2026-11-08:

- `npm.cmd run build`: passed (TypeScript and Vite).
- `cargo test --locked themes::tests`: 6 passed, 1 ignored.
- `cargo test --locked designs::tests`: 5 passed.
- `cargo test --locked customizer::tests`: 11 passed, 1 ignored.
- The two ignored tests access real-installation fixtures and were not run. No real target installation was modified. Full cargo test, clippy, license gate and manual Tauri smoke test were not run for this documentation-only proposal.
- The first `npm run build` attempt hit PowerShell's npm.ps1 execution policy; the npm.cmd invocation above passed without changing that policy.

Owner approval received for Sprint 1 architecture and four-PR sequence with the amendments above. Each PR remains subject to review; main merges require separate owner authorization. The planning-time checks above are historical, not the PR 1 validation record.
