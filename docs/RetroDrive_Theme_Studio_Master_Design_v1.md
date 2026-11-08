# RetroDrive Theme Studio
## Master Product, UX, Architecture and Delivery Specification
**Version:** 1.0  
**Date:** 2026-11-08  
**Status:** Proposed design baseline, Option B selected  
**Product:** RetroDrive Studio, integrated Theme Studio module

## 1. Executive summary
RetroDrive Theme Studio is a complete offline-first visual environment for creating, testing, packaging, installing and sharing themes for retro-gaming frontends. RetroDrive remains an emulator installation/configuration and management suite; Theme Studio adds a creative system that reuses its platform discovery, game metadata, artwork, backups and configuration services.

**Architecture decision: Option B.** Theme Studio stores an editable platform-independent project, and compiles its supported subset through frontend-specific adapters. Features unsupported by the selected frontend are reported before deployment and can be downgraded, rendered into assets where equivalent, or excluded with user consent. Future custom rendering is an architectural extension, not a V1 prerequisite.

### Goals
- No coding or hand-edited XML needed for supported designs.
- Consistent visual editing with real asset preview and input simulation.
- Per-system layouts and global layouts with reusable components and responsive breakpoints.
- Full content workflow: startup, main menu, systems, games, game details, media, transitions, and exit **where supported by the target**.
- Safe, reversible installation on local or remote supported targets.
- Local-first, portable, versioned and validated theme packages.

### Non-goals for V1
- Guaranteeing arbitrary custom interactions in third-party frontends.
- Editing emulation core code or changing gameplay behavior.
- Hosting a public marketplace or auto-sharing copyrighted media.
- Delivering a replacement emulator frontend.
- System-level splash control for every hardware/OS configuration.

## 2. Users and journeys
### Personas
1. **Casual customizer:** picks template, changes logos/colors, deploys in a few clicks.
2. **Arcade cabinet builder:** designs highly branded 4:3, 16:9 or portrait experiences and controller input.
3. **Power theming creator:** builds responsive systems, animation, theme variants, reusable assets and shares projects.
4. **Multi-system maintainer:** deploys consistent themes to multiple installs while respecting individual capabilities.

### Primary workflows
**Quick creation:** detect install → choose target+template → customize → preview → compatibility check → backup → install → verify/rollback.
**Advanced creation:** new project → define screen variants → create scenes/components → bind game data → animate/media → simulate → export/install.
**Import:** select supported existing theme → scan references/licensing → map supported elements → preserve unsupported XML as non-editable passthrough *only when safe* → edit a copy → re-export.
**Share:** validate → remove local paths and private data → inspect licenses → package `.rdtheme` → export.

## 3. Requirements catalog
Priorities: P0 minimum useful release; P1 full visual authoring; P2 later.

### 3.1 Project and workspace
- **TS-PROJ-001 [P0]** Create/open/save/save-as/duplicate project, project metadata (name, author, description, version, license).
- **TS-PROJ-002 [P0]** Autosave and recovery of interrupted sessions; explicit save checkpoints.
- **TS-PROJ-003 [P0]** Undo/redo of meaningful edit operations; selectable layer tree.
- **TS-PROJ-004 [P0]** Keep source projects independent from compiled target packages.
- **TS-PROJ-005 [P1]** Version snapshots and compare/revert.
- **TS-PROJ-006 [P1]** Reusable templates, shared components and themes/variants.
- **TS-PROJ-007 [P0]** Relative-path, content-addressed assets, safe path resolution.

### 3.2 Visual editor
- **TS-VIS-001 [P0]** WYSIWYG scene canvas with select, drag, resize, rotate where supported; snap guides, grid, alignment and distribute.
- **TS-VIS-002 [P0]** Layer tree with rename, lock, hide, group and z-order.
- **TS-VIS-003 [P0]** Images, text, logo, panels, basic shapes, metadata text, game art and system/game navigation elements.
- **TS-VIS-004 [P0]** Properties: position, size, anchor, relative dimensions, color, typography, opacity, cropping and conditional visibility.
- **TS-VIS-005 [P1]** 16:9, 4:3, 16:10, 21:9, portrait/vertical breakpoints and safe-area controls.
- **TS-VIS-006 [P1]** Visual style tokens for palettes, typography, effect defaults and variants.
- **TS-VIS-007 [P1]** Complex components: carousel, grid, wheel, video preview, progress/loading, help prompts.
- **TS-VIS-008 [P1]** Accessibility inspection: text contrast, minimum font, reduced motion, controller focus visibility.

### 3.3 Scenes and navigation
- **TS-SCENE-001 [P0]** Scenes: startup splash, system select, game list and game detail; design representations for launch/idle/exit, with target support warnings.
- **TS-SCENE-002 [P0]** Global base scene plus system-specific overrides (e.g., NES/Genesis/Arcade).
- **TS-SCENE-003 [P0]** Game metadata bindings with safe placeholders (title, release year, players, description, rating, artwork, video).
- **TS-SCENE-004 [P1]** Visual graph of navigation states and transitions; keyboard/controller mapping simulator.
- **TS-SCENE-005 [P1]** Conditional overrides by target, screen aspect, and theme variant.
- **TS-SCENE-006 [P1]** Empty collection, unavailable media and load failure fallbacks.
- **TS-SCENE-007 [P2]** More elaborate interaction state machine for future renderer.

### 3.4 Images/video/audio
- **TS-MEDIA-001 [P0]** Local asset import and browser; files stay within controlled project space.
- **TS-MEDIA-002 [P0]** Supported image PNG/JPEG/WebP/SVG by adapter, validated before export.
- **TS-MEDIA-003 [P0]** Reuse existing game art and metadata through RetroDrive library, with user-selectable project copies or references.
- **TS-MEDIA-004 [P1]** Video thumbnails, trimming, loop in/out, crop, scale, transcoding profiles, fallback image.
- **TS-MEDIA-005 [P1]** Background music, per-system playlists, SFX assignments, volume normalization, looping, fades and ducking.
- **TS-MEDIA-006 [P1]** Original intro sequence editor: timeline, image/video/audio clips, synced transitions, rendered output profiles.
- **TS-MEDIA-007 [P0]** Asset deduplication, licenses, provenance metadata and packaging audit.
- **TS-MEDIA-008 [P1]** Asset optimizer with quality presets and memory/disk budget estimates.

### 3.5 Animation
- **TS-ANIM-001 [P1]** Keyframes for position, scale, opacity, rotation and supported properties.
- **TS-ANIM-002 [P1]** Easing functions, repeat, delay, trigger and scene transitions.
- **TS-ANIM-003 [P1]** Multi-track timeline with audio sync, scrub, play, pause and markers.
- **TS-ANIM-004 [P1]** Export strategy per feature: native effect, prerendered media, approximate effect or unsupported.
- **TS-ANIM-005 [P1]** Reduced-motion preview mode.

### 3.6 Preview and validation
- **TS-PREV-001 [P0]** Preview target aspect ratio and safe area with sample or real game library data.
- **TS-PREV-002 [P0]** Keyboard and controller input simulation with focus/highlight and back handling.
- **TS-PREV-003 [P0]** Compatibility report for exact frontend family/version/platform and individual features.
- **TS-PREV-004 [P0]** Distinguish editor approximation from verified native frontend behavior.
- **TS-PREV-005 [P1]** Optional real-target preview via temporary export and launch, with controlled rollback.
- **TS-PREV-006 [P1]** Performance checker for memory, image sizes, codec, video bitrates, FPS target and storage use.
- **TS-PREV-007 [P0]** Block export on broken references and invalid config; warn on quality degradation and licensing.

### 3.7 Export and deployment
- **TS-DEPLOY-001 [P0]** Adapter registry and target capability profile with explicit version matching.
- **TS-DEPLOY-002 [P0]** Compile universal model to target-specific XML/config/files and packaged media.
- **TS-DEPLOY-003 [P0]** Preview changes in dry-run (files added/changed/deleted) and show incompatibilities.
- **TS-DEPLOY-004 [P0]** Backup existing target theme/settings before any change; atomic staging, validation and rollback on failure.
- **TS-DEPLOY-005 [P0]** Install into detected local RetroBat target and supported Batocera target via validated path or explicit supported copy mechanism.
- **TS-DEPLOY-006 [P0]** Never silently edit boot/system partitions or overwrite active installations.
- **TS-DEPLOY-007 [P1]** Remote deployment with credential handling using OS credential store and least privilege.
- **TS-DEPLOY-008 [P1]** Post-install verification and activation instructions/automated activation only when version-safe.
- **TS-DEPLOY-009 [P0]** Export `.rdtheme` editable source package and target-specific archives separately.

### 3.8 Theme library and sharing
- **TS-LIB-001 [P0]** Project list with thumbnail, tags, target support, update version and last edited time.
- **TS-LIB-002 [P1]** Import of compatible existing theme packages with warnings for lost/uneditable features.
- **TS-LIB-003 [P1]** Reusable local component and preset library.
- **TS-LIB-004 [P2]** Public gallery, ratings, dependency resolution and moderation.
- **TS-LIB-005 [P0]** No automatic inclusion of ROMs, BIOS or game files in public theme packages.

### 3.9 Nonfunctional requirements
- **TS-NFR-001** Offline editing, preview and export must work without cloud account.
- **TS-NFR-002** Crash recovery and transactional writes; deterministic build manifests.
- **TS-NFR-003** Project format has schema version and migrations; never lose unknown fields silently.
- **TS-NFR-004** Explicit user consent before modifying anything outside a dedicated RetroDrive theme workspace.
- **TS-NFR-005** Asset import must reject traversal, dangerous executable payloads and suspicious archive entries.
- **TS-NFR-006** UI remains responsive during indexing, transcoding and compilation using worker jobs.
- **TS-NFR-007** Editor support for Windows initially, with OS abstraction for future platforms.
- **TS-NFR-008** Performance budgets defined per hardware profile rather than one guaranteed threshold.
- **TS-NFR-009** Accessibility: keyboard operation, readable contrast, scalable UI, reduced motion.
- **TS-NFR-010** No proprietary game art/music redistributed automatically; explicit license attribution.

## 4. UX and screen specifications
### 4.1 Theme Studio Home / Project browser
**Layout:** left navigation; central project tiles; right recent targets. CTAs: New Theme, From Template, Import Theme, Resume. Each tile shows name, preview, last update, target, warnings.
**Empty state:** guided template wizard. **Actions:** duplicate, rename, archive, package, export.

### 4.2 New Theme wizard
1. Project metadata; 2. primary frontend (RetroBat/Batocera), exact target version if detected; 3. target displays (16:9/4:3/portrait); 4. visual template; 5. optional library sample data; 6. starter scenes.
Show native limitations immediately, without preventing universal editing.

### 4.3 Visual Designer
**Top toolbar:** project/scene, target profile, display breakpoint, save state, undo/redo, preview, validation, build.
**Left sidebar tabs:** Components / Layers / Scenes / Assets.
**Center:** interactive artboard with safe area, rulers, grid, snap, zoom, preview overlays.
**Right inspector:** Position / Appearance / Binding / State / Media / Animation / Compatibility.
**Bottom drawer:** timeline, issue log or data bindings (collapsible).
**Core interactions:** double-click to edit; drag to add; copy/paste; group; lock; context-aware compatibility badge per property.

### 4.4 Scene & Navigation Designer
**Left:** screen library: intro, system selection, game list, details, launch, idle, exit. **Center:** connected flow graph. **Right:** transitions, button mapping, per-system override rules. Validate dead ends and missing back navigation.

### 4.5 Media Studio
Asset browser with details: filename, media type, dimensions/duration, codec, license, size, usage count. Image editor: crop/fit. Video editor: trim, loop, transcode profiles. Audio editor: playlists, per-scene audio and gain/fades, ducking. Media optimizer includes target profiles.

### 4.6 Animation Studio
Scene preview above a multitrack timeline. Keyframe inspector with property, timing, interpolation and trigger. Render unsupported live animations to video **only where the target can play the resulting asset in the same context**.

### 4.7 Live Preview/Test
Fullscreen or windowed; display presets; dummy/real library entries; controller overlay; keyboard shortcuts; toggles: show focus, missing assets, reduced motion, safe area and compatibility. Clearly mark simulated results.

### 4.8 Build/Deploy Wizard
Choose target → detect version and access → analyze capability matrix → generate report and diff → optimize/package → backup → stage → install/activate where supported → verify → finish or rollback. Failed verification never removes backup.

### 4.9 Theme Library
Local projects, installed themes and imported packages; cards with screenshots, variants, provenance, frontend compatibility, validation state. Share action invokes license/privacy audit and package export.

## 5. Architecture
### 5.1 Logical services
```
RetroDrive Desktop Shell
  ├── Existing: Platform Discovery / Library / Media / Backup / Configuration
  └── Theme Studio UI
       ├── Project Manager
       ├── Scene/Layer/Navigation Editor
       ├── Asset, Audio, Video and Timeline Editors
       ├── Interactive Preview Renderer
       └── Build/Deploy Wizard
                  │
           Theme Studio Core
       ├── Versioned Universal Theme Schema
       ├── Asset Index and Content Store
       ├── Data Bindings & Variant Resolver
       ├── Compatibility/Validation Engine
       ├── Build IR + Optimization Pipeline
       ├── Export Adapter Registry
       │    ├── RetroBat / its actual ES fork
       │    ├── Batocera / its actual ES fork
       │    └── ES-DE (future; separate schema)
       └── Installer + Staging + Backup + Rollback
                  │
           Target Frontends / Installations
```
### 5.2 Suggested implementation boundary
Keep the UI renderer distinct from the compiler. If the RetroDrive desktop stack is already established, retain it. Do not introduce a second framework simply for themes. Core data, validation, exporter contracts and installer should be callable without the UI for automated tests or CLI.

If existing RetroDrive is Python-based, a reasonable implementation is Python domain services with PySide6/Qt graphics or an embedded web canvas; choose after auditing current repo and frontend stack. FFmpeg/ffprobe are optional bundled or externally installed media utilities subject to license review, and long jobs should run asynchronously with cancellation.

### 5.3 Intermediate representation
Pipeline: **project JSON → normalized scene graph → resolved variants/data bindings → capability analysis → adapter lowering → asset transformations → target files → manifest → staging**. Adapter output must be deterministic and isolated by frontend/version.

### 5.4 Module boundaries
- **ProjectService:** CRUD, snapshots, schema migrations.
- **SceneService:** nodes, variants, constraints and navigation.
- **AssetService:** secure import, fingerprinting, metadata, usage, license.
- **PreviewService:** generic editor renderer; not necessarily pixel-identical to frontend.
- **CapabilitiesService:** supports(feature, frontend, version, platform) with severity and fallback.
- **CompilerService:** normalize, validate, lower, optimize, stage.
- **Adapter:** formats target files, supported properties, required paths, smoke tests.
- **DeploymentService:** diff, backup, scoped copy, activation, verification and rollback.
- **LibraryBridge:** read-only media/metadata access to existing RetroDrive collections unless explicit sync requested.

## 6. Versioned universal theme file
A `.rdtheme` file is a ZIP container with a safe manifest, source JSON, and assets; it is **not** the same as a frontend-native theme archive. Suggested content:
```
my-theme.rdtheme
  manifest.json
  project/theme.json
  assets/images/<content-hash>.<ext>
  assets/video/<content-hash>.<ext>
  assets/audio/<content-hash>.<ext>
  preview/cover.webp
  licenses/attributions.json
```
Working folder uses the same logical structure, unpacked. Package includes content hashes and file size list. No executable scripts, ROMs, BIOS, absolute system paths or credentials.

### Example proposed manifest (illustrative, not an existing RetroDrive schema)
```json
{
  "format": "retrodrive.theme",
  "schemaVersion": "1.0.0",
  "themeId": "example.neon-arcade",
  "name": "Neon Arcade",
  "version": "1.0.0",
  "author": "Theme Creator",
  "license": "custom",
  "defaultAspectRatio": "16:9",
  "targets": ["retrobat", "batocera"],
  "entryScenes": {"system": "systems", "gamelist": "games"},
  "assets": [{"id": "bg", "path": "assets/images/placeholder.webp", "sha256": "REPLACE_WITH_REAL_HASH", "license": "original"}]
}
```
### Proposed node schema concepts
`id`, `type`, `name`, `layout`, `style`, `children`, `dataBinding`, `variants`, `animations`, `events`, `assets`, `capabilityNeeds`. Node IDs stable across edits. All dimensions have explicit units; bindings are typed and validated. Avoid arbitrary code execution in themes.

### Scene graph
Root → Scene → Groups/Components → properties, media bindings, state variations. Inheritance: global styles → theme variant → aspect ratio → system override → component override. Define precedence strictly and surface effective values in inspector.

## 7. Frontend adapters and compatibility
**Important:** RetroBat and Batocera are both EmulationStation-based but do not have identical schemas, configuration structures or feature behavior. ES-DE has a substantially different current theme format, so never treat all three as one XML target.

### Target profile
`frontendFamily`, `frontendVersion`, `os`, `architecture`, `displayAspect`, `rendererCapabilities`, `themeFormatVersion`, `installationMode`, `systemPermissions`, `estimatedMemoryBudget`.

### Adapter interface (illustrative pseudocode)
```
interface ThemeAdapter:
  identify_target(location) -> TargetProfile
  capabilities(profile) -> CapabilityMatrix
  validate(project_ir, profile) -> Issues[]
  compile(project_ir, profile, staging_dir) -> BuildManifest
  verify(staging_dir, profile) -> VerificationResult
  installation_plan(build, profile) -> InstallPlan
```
Adapters do **not** arbitrarily write to targets. A single DeploymentService mediates mutations using explicit allowlists and backups.

### Capability outcomes
- `native`: expressible without loss in target format.
- `convert`: generated into a supported asset or structure with documented differences.
- `degraded`: approximated with visible user approval.
- `unsupported`: target cannot express; warn and omit, or block when essential.
- `unknown`: not yet tested on target version; no automatic deployment without explicit review.

### Initial compatibility assumptions requiring tests
- Image/text/system and gamelist compositions: strong basis in supported ES themes.
- System-specific imagery and music: generally available, but exact paths and semantics vary.
- Boot video: separate splash mechanism and may need a separate installation flow; cannot assume part of theme XML.
- Arbitrary animated timelines, custom interaction graphs, global overlay and shutdown sequences: no universal guarantee; renderer or conversion needed.
- Theme imports: best effort, not round-trip fidelity guarantee.

### Build degradation decision UX
Per issue show element, unsupported reason, fallback, visual preview and accept/modify action. Never silently delete work from editable source.

## 8. Deployment safety and security
1. Fingerprint target and verify supported/authorized access.
2. Build in temporary directory; parse/check XML/JSON and asset references.
3. Produce preview diff, target directory allowlist and disk-space estimate.
4. Snapshot touched files and settings, preserve current theme selection.
5. Copy to unique staging theme directory; never overwrite existing theme by default.
6. Activate theme only where supported and with explicit user approval.
7. Verify file presence, checksums and available frontend logs/test launch where safe.
8. On failure, restore prior selection and touched files; keep recovery logs and backup.
9. Support a one-click rollback entry from installed-theme history.

Archive import hardening: avoid zip-slip, symlink escape, decompression bombs, extension-only trust, executable payloads and external resource fetching by default. Do not execute XML includes that point outside the project sandbox; resolve only safe relative references. Credential storage uses platform secure storage; redact logs. User-created themes do not contain scripts in V1.

## 9. Performance and quality
Quality presets: **Lite (low VRAM/handheld), Balanced, Cinematic (PC/cabinet)**, each with configurable image texture limits, total memory estimates, audio and video bitrate caps. Real limits based on device testing.

Targets for acceptance (project engineering objectives, not measured performance):
- Recover editable state after simulated crash in autosave tests.
- All P0 editor operations undo/redo without corruption.
- Valid simple project compiles to structurally valid output across every supported adapter/version fixture.
- Clean one-click rollback after forced partial installation failure.
- Offline export completes without network calls.
- Input preview supports keyboard and a common gamepad; focus never becomes trapped.
- No archive entry writes outside staging directory under malicious-package tests.
- Same source/config/environment produces identical build manifest and hashes.

## 10. Data and API contracts
- `ThemeProject`: id, metadata, schemaVersion, scenes, variants, tokens, assets, targetProfiles.
- `Scene`: id, role, logical resolution, nodes, navigationStates, override selectors.
- `Node`: id, type, transform, style, bindings, states, properties, children.
- `Asset`: id, sha256, localPath, mediaMetadata, attribution, license, source.
- `CapabilityIssue`: id, severity, nodeId, feature, target, outcome, fallback, userAction.
- `BuildManifest`: targetFingerprint, generatedFiles (path, hash, bytes), warnings, tool versions.
- `DeployRecord`: target, previousState, backupPath, installedVersion, status, timestamps, checksums.

Typical domain APIs (internal, not exposed publicly): `createProject`, `importAsset`, `updateScene`, `validateProject`, `previewScene`, `compileTarget`, `planInstall`, `commitInstall`, `verifyInstall`, `rollbackInstall`, `packageProject`.

## 11. Suggested repo organization
Adapt naming to existing RetroDrive repo; avoid duplicating current core services.
```
retrodrive/
  apps/desktop/                         # existing UI, extend
  modules/theme_studio/
    domain/                             # schema, scene, variant model
    project/                            # persistence, migration
    editor/                             # UI/workspace
    media/                              # assets, transcoding
    preview/                            # preview renderer
    compiler/                           # IR, validation, pipeline
    adapters/
      retrobat/
      batocera/
      es_de/                             # later
    deployment/                         # stage, backup, rollback
    packaging/                          # .rdtheme
    tests/
      fixtures/
      golden_outputs/
      integration/
  docs/theme-studio/
    requirements.md
    architecture.md
    UX.md
    adapters.md
    security.md
    test-plan.md
```

## 12. Roadmap and milestones
Estimates are planning ranges only and depend on current RetroDrive tech stack and developer availability. A small team can overlap phases; scope gates matter more than calendar dates.

### Phase 0: Technical discovery (1–2 weeks)
Audit existing source, current screen management, library/media schema and deployment services. Identify exact tested RetroBat/Batocera versions and paths. Create adapter proof-of-concepts for static system/gamelist views and boot splash as a separate feature. **Exit:** documented target profiles and verified export test matrix.

### Phase 1: Core visual designer (4–6 weeks)
Create project schema, storage, templates, scene/layer canvas, properties, style tokens, asset import, autosave, undo/redo, simple sample data preview. **Exit:** complete starter theme editable and restorable from disk.

### Phase 2: Initial export and installation (3–5 weeks)
Implement compiler IR, versioned adapter for the first actual tested frontend target, report/diff, backups, staging, install and rollback; then second adapter. **Exit:** one-click export/install/rollback on test targets, native screenshots compared.

### Phase 3: Rich media (4–6 weeks)
Video and music workflows, per-system media, audio cue binding, asset optimizer, intro composition with target-compatible export. **Exit:** themed intro and browsing experience, explicit unsupported feature UX.

### Phase 4: Cinematic animation and advanced layouts (4–7 weeks)
Timeline, keyframes, variants, aspect ratios, advanced game carousels, scene transitions, controller/navigation graph. **Exit:** build full advanced multi-system theme with reliable compatibility reporting.

### Phase 5: Product hardening and theme library (3–5 weeks)
Import assistance, local theme catalog, source packaging, user onboarding, documentation, performance test matrix, accessibility, malicious-archive tests and installer recovery. **Exit:** beta release quality gate.

### Phase 6: Expansion (after stable release)
Add ES-DE adapter after independent theme engine assessment; optional remote deployment, public gallery, community sharing, AI-assisted original assets, optional RetroDrive-rendered frontend.

**Suggested sequence:** Phase 0 → 1 and 2 with early adapter spikes → 3 → 4 → 5 → 6. First useful beta should aim to include Phases 0–2, not wait for the cinematic suite.

## 13. Testing strategy
### Unit tests
Scene graph serialization, migration, property precedence, asset hashing, path sanitization, interpolation, capability rules and deterministic compilation.
### Snapshot/golden file tests
Per adapter/version fixtures: project → generated XML/config/media manifest. Review and pin differences on each frontend update.
### Integration tests
Discover real/simulated RetroBat and Batocera installations, stage, install, rollback, interrupted install, blocked paths, insufficient disk space, invalid XML.
### Visual/manual tests
16:9, 4:3, portrait; low and higher performance hardware; video/audio playback; controller input; long lists; missing asset fallback; reduced motion.
### Security tests
Malformed XML, symlink traversal, ZIP path traversal, huge archives, injected scripts, hostile SVG, unsafe codec input, credential leaks and unlicensed asset warnings.
### Acceptance demos
1. Casual user changes a starter theme and deploys it with no code.
2. Power user builds NES/Genesis/Arcade overrides and previews each.
3. Deployment fails midway; previous theme remains recoverable.
4. Batocera adapter warns about unsupported animation while preserving source design.
5. User exports and reimports `.rdtheme` with identical project content.

## 14. Risk register
| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Frontend/theme schema differences | High | High | Exact versioned adapters and golden tests |
| WYSIWYG preview differs from frontend | High | High | Approximation labeling; native smoke testing |
| Arbitrary animation not deployable | High | Medium | Compile fallback, compatibility flags, future renderer |
| Media assets too large | Medium | High | Device profiles and optimizer |
| Unreliable installation/boot files | Medium | High | Allowlisted paths, no automatic boot partition writes, rollback |
| Copyrighted content in shared themes | Medium | High | Attribution manifest, export audit, no automatic redistribution |
| Feature scope creep | High | High | P0/P1/P2 gates, first shipped adapter and user journeys |
| Importing external themes round-trip failure | High | Medium | Best effort importer, edit copies, explain losses |

## 15. Decisions to confirm during Phase 0
- Which exact source code/framework and UI component system RetroDrive uses today?
- Which RetroBat and Batocera releases/platforms form the initial test matrix?
- Does RetroDrive already have a media/artwork database and thumbnail pipeline?
- Is Windows-only authoring sufficient for initial release?
- What are the target device performance classes and media encoding licenses?
- Which remote installation mechanisms already exist and are authorized?
- What should be the standard starter theme and representative sample games?

## 16. Initial engineering backlog, ordered
1. TS-001 Audit RetroDrive repo and map existing services.
2. TS-002 Capture RetroBat and Batocera target/version fixtures.
3. TS-003 Create universal theme JSON schema and migration tests.
4. TS-004 Create adapter contract and simple working sample export.
5. TS-005 Build project browser and new project wizard.
6. TS-006 Build visual canvas, selection and layered objects.
7. TS-007 Build property inspector, layout engine and style tokens.
8. TS-008 Integrate artwork/metadata sample-data bindings.
9. TS-009 Implement asset import, hashing and sandbox.
10. TS-010 Implement undo/redo, autosave and recovery.
11. TS-011 Implement target capability analyzer and diagnostics panel.
12. TS-012 Implement first tested frontend export adapter.
13. TS-013 Implement dry-run deployment and diff.
14. TS-014 Implement transactional backup/install/rollback.
15. TS-015 Add second tested frontend adapter.
16. TS-016 Add gamepad/keyboard interactive preview.
17. TS-017 Add audio/video editor and encoding profiles.
18. TS-018 Add timeline and export degradation strategy.
19. TS-019 Add breakpoints and per-system overrides.
20. TS-020 Add `.rdtheme` packaging, import and licensing audit.

## 17. External implementation references (verified November 8, 2026)
- Batocera official EmulationStation theme format: https://github.com/batocera-linux/batocera-emulationstation/blob/master/THEMES.md
- Batocera theme authoring guide: https://wiki.batocera.org/write_themes_for_emulationstation
- Batocera splash customization: https://www.wiki.batocera.org/splash_boot
- RetroBat folder structure and media locations: https://wiki.retrobat.org/get-started/retrobat-folder-structure
- ES-DE theme format, distinct from legacy ES: https://gitlab.com/es-de/emulationstation-de/-/blob/master/THEMES.md

---
**Design principle:** The source theme is always editable and complete. Target exports are compiled derivatives; incompatible details are never silently discarded from the original project.
