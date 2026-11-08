# Theme Studio delivery tracking

PR 2 update: backend storage for PROJ-001/002/004 and NFR-002 is implemented in [projects/IPC](persistence.md), including checkpoint/draft separation, revision conflicts, recovery/discard, rename and Save As. UI autosave/recovery/navigation flushing remains PR 3. The table below records PR 1 evidence and later scope; it does not imply PR 2 UI completion.

The [approved master](../RetroDrive_Theme_Studio_Master_Design_v1.md) retains all requirement IDs/priorities. Sprint completion is not completion of every P0 feature.

| Requirements | PR 1 evidence | Remaining work |
|---|---|---|
| PROJ-001/004/007, NFR-003 | Universal source, no-target intents, hash-based asset declarations, stable IDs, version refusal and extension-preservation tests | PR 2 persistence/checkpoints; PR 3 wizard/browser; secure asset store and packages later |
| PROJ-002/003/005/006 | No runtime implementation | Autosave/recovery in PR 2; undo/redo, snapshots, templates/components/variants later |
| VIS-001–008 | Scene/node objects and explicit layout units | Canvas/inspector/layers, transforms/guides/groups, styles/conditions, breakpoints and accessibility |
| SCENE-001–003 | Eight design roles, hierarchy checks, small typed binding vocabulary | Scene editing, library bridge, override resolver and typed video binding |
| SCENE-004–007 | Navigation payload preserved in future fixture | Navigation graph/input simulator, conditions/fallbacks, future state machine |
| MEDIA-001–008 | Asset declarations/references, provenance/license fields; audio/video payload preservation | Secure import/browser, content hashes/decoding/SVG safety, library copies/references, media editors/optimizer/intro compositor |
| ANIM-001–005 | Animation payload preserved | Timeline/keyframes, playback/easing/triggers, audio sync, reduced motion and target fallbacks |
| PREV-001–007 | No preview shipped | Sample/real data, aspect/safe area, keyboard/gamepad, exact compatibility, export blockers and native/performance tests |
| DEPLOY-001–009 | Target intent separate from future native output; no writes | PR 4 adapter/capability foundation; RetroBat compile/export, scoped installation review, diff/staging/backup/verify/rollback; Batocera next; source/native archives separately |
| LIB-001–005 | No library/sharing UI shipped | PR 3 project browser; thumbnails/tags/support, safe import, presets, packaging exclusions and privacy/license audit; public gallery P2 |
| NFR-001/002/004–010 | Pure offline bounded parsing; no filesystem/network/execution APIs or new dependencies | Persistence recovery/jobs, asset/package/media sandboxing, accessible editor, measured budgets and privacy/licensing gates |

Backlog: TS-001 audit completed in proposal; TS-003 contract/validation foundation implemented in PR 1 (format migration tests await a real migration). TS-002 target fixtures and TS-004 adapter/export spike remain ahead. TS-005 browser is PR 3. TS-006–020 retain the full approved scope in the [plan](../theme-studio-sprint-1-plan.md).

Owner adjustments (2026-11-08): authoritative SQLite working state and portable JSON; explicit Universal / No Target Yet; extensible scene/layer/canvas/inspector/timeline UI; isolated worktree; existing Customize/designs/target files preserved; four small PRs and no merge to main without authorization.
