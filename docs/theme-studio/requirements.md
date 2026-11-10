# Theme Studio delivery tracking

**SCENE-003/PREV-001 partial sample context:** synthetic scalar metadata and typed text binding selection now support no-emulator/no-library authoring. This is text preview only: sample artwork, real catalog bridge, navigation/list components, target-native previews and full fallback/formatting rules remain future increments.

**VIS-004 partial image presentation:** Fit/Fill, percentage crop/reset and asset metadata inspection now integrate with history and immutable PNG previews. Direct graphical crop handles, anchors/conditional visibility, advanced image effects, codecs and native compilation remain future work. Browser pixels pass; native acceptance remains pending.

**MEDIA-001/VIS-003 partial PNG workflow:** static user PNG import, license/attribution, managed catalog bytes, image-layer authoring and verified previews are implemented for review. Byte limits/normalization remain as documented; native end-to-end QA is pending. More codecs, crop tools, full asset browser/cleanup, source packages and native export remain unimplemented.

**MEDIA-001/NFR partial foundation:** a bounded PNG codec normalizes supplied bytes and creates a portable hash path. This does not implement user file import, asset library, image layers or preview serving. Managed storage and UI wiring follow next; native export remains disabled.

**VIS-002 partial lock/hide:** supported leaf text/panel layer flags now persist and are undoable, guarded across content operations, with list-based unlock/show. Nested/group authoring, native conditional visibility and native target output remain unimplemented. Browser tests pass; native smoke validation is pending.

**VIS-001 partial resize:** selected supported leaf layers now expose a bottom-right handle and handle-focused arrow sizing. Other handles, aspect locking, multi-selection, snapping/grid/guides, groups and native engine preview remain unimplemented; native interaction QA is pending.

**VIS-001 partial alignment:** single supported leaf layers can align their rotated rectangles to canvas edges/centers with undo/redo. Multi-selection, distribute, snapping/grid/guides and resize/rotation handles remain unimplemented; native rendering is unverified.

**VIS-001/004 partial follow-up:** numeric rotation and opacity authoring now reuse existing source fields and history for supported leaf text/panel layers. Rotation handles, anchors, conditions, grouping and complete style resolution remain planned; native rendering is not verified.

**VIS-002 partial follow-up:** back/front and one-step layer ordering now operate on flat text/panel scenes through existing undo/draft/checkpoint services. Unsupported/nested scenes are guarded. Grouping, locking and hiding remain unimplemented; native interaction QA is outstanding.

First canvas increment advances PROJ-003 and VIS-001–004 with top-level text/panel creation/selection, movement, numeric sizing/properties and bounded undo/redo. This is partial visual authoring: snapping/guides, handles, grouping/z-order/lock/hide, images, advanced variants and full property/binding resolution remain later work. [Scope and QA limits](canvas.md).

PR 4 update: DEPLOY-001/PREV-003 foundation now includes exact theme-adapter contracts, read-only local profile discovery and source-preserving capability diagnostics. Production export/deploy are always disabled; real engine-version/native fixtures remain outstanding. [Matrix and provenance](target-matrix.md). Four PRs prepared is not completion of every P0 or the native acceptance gates.

PR 3 update: independent workspace, no-target wizard/browser and scene overview with metadata editing are implemented. PROJ-001/002 UI now uses serialized autosave, explicit checkpoints/recovery/discard and leave guards. Scene/layer/canvas/inspector/timeline composition is ready for further authoring, but visual manipulation and animation are not implemented. See [workspace](workspace.md) for verification gaps.

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
