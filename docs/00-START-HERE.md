# RetroDrive | Documentation Index

**Theme Studio PNG import workflow:** [storage, safety and QA](theme-studio/image-import.md#managed-catalog-import-and-preview) explains catalog blobs, path-free import, preview authorization and native smoke checks.

**Theme Studio image import:** [PNG safety boundary](theme-studio/image-import.md) documents codec-only validation, resource limits and the managed-storage/preview work still required before UI import.

**Theme Studio layer ordering:** [canvas scope and QA](theme-studio/canvas.md#layer-ordering) covers back-to-front ordering of flat text/panel scenes, undo/redo and native validation limits.

**Theme Studio canvas:** [first authoring increment](theme-studio/canvas.md) documents supported text/panel editing, undo/redo and remaining native interaction checks. It does not enable frontend export.

**Theme Studio PR 4:** [target evidence matrix and adapter contracts](theme-studio/target-matrix.md). Application-release detection is not native theme compatibility; all real theme exports remain disabled.

**Theme Studio PR 3:** [workspace and save/recovery behavior](theme-studio/workspace.md); separate from Customize. Lifecycle/server-render tests do not replace a manual native interaction smoke test.

**Theme Studio PR 2:** [local project persistence/recovery](theme-studio/persistence.md) adds catalog v27 and background IPC. It remains separate from legacy designs and installed theme files; no workspace UI is shipped yet.

**Theme Studio:** [master specification](RetroDrive_Theme_Studio_Master_Design_v1.md), [Sprint 1 plan](theme-studio-sprint-1-plan.md), [source contract](theme-studio/project-format.md), [delivery tracking](theme-studio/requirements.md) and [security](theme-studio/security.md). PR 1 adds pure source types/validation only.

**Design baseline:** v1.0, 2026-11-04  
**Status:** Product concept approved; starter application scaffold exists; full suite is not implemented.  
**Product:** A Windows-first desktop application to create, catalog, manage, maintain, back up, and troubleshoot retro gaming environments.  

## Read in this order

| Document | Why it exists |
|---|---|
| [01 Product vision](01-PRODUCT-VISION.md) | Purpose, audiences, success criteria, positioning |
| [02 Scope and requirements](02-SCOPE-AND-REQUIREMENTS.md) | Feature inventory, priority, MVP boundaries, acceptance |
| [03 UX and screens](03-UX-AND-SCREENS.md) | Navigation, flows, screen states, usability |
| [04 Architecture](04-SYSTEM-ARCHITECTURE.md) | Technologies, services, trust boundaries and adapters |
| [05 Data model](05-DATA-MODEL.md) | Hybrid master library and relational schema |
| [06 Drives and installations](06-DRIVES-AND-INSTALLATIONS.md) | Safe discovery, Batocera, RetroBat, imaging rules |
| [07 Game library](07-GAME-LIBRARY.md) | Discovery, identity, metadata, duplicates, BIOS |
| [08 Deployment and sync](08-DEPLOYMENT-AND-SYNC.md) | Preview/copy/verify/commit, conflicts, saves |
| [09 Maintenance](09-MAINTENANCE-AND-PROFILES.md) | Profiles, updates, backups, diagnostics |
| [10 Security and legal](10-SECURITY-PRIVACY-LEGAL.md) | Safety, permissions, copyright, providers |
| [11 Roadmap](11-ROADMAP-AND-BACKLOG.md) | Releases, stories and definition of done |
| [12 Development setup](12-DEVELOPMENT-GUIDE.md) | VS Code, commands, branching and test plan |
| [13 Decisions and open questions](13-DECISION-LOG.md) | Decisions already made and items still unconfirmed |
| [14 AI implementation brief](14-AI-CODING-BRIEF.md) | Ready-to-use instructions for coding assistants |
| [15 QA matrix](15-QUALITY-AND-TESTS.md) | Repeatable functional and safety tests |
| [16 Legal and compliance](16-LEGAL-AND-COMPLIANCE.md) | Legal-risk assessment, LEGAL-001…014 backlog, sign-off decisions, engineering status |
| [17 Third-party components](17-THIRD-PARTY-COMPONENTS.md) | Manifest of shipped dependencies, platforms, assets and content (LEGAL-001) |
| [18 Safety review: first disk writes](18-SAFETY-REVIEW-RDS-010.md) | Hazards, mitigations, tests, review findings and owner sign-off for the staged copier (RDS-010) |
| [19 Light guns and light bars (research)](19-LIGHT-GUNS-RESEARCH.md) | What RetroBat and Batocera already do for light guns, facts RetroDrive can build on, and the step 3 proposal awaiting owner approval (RDS-028). Design only |

**Feature status:** see [`../Features.md`](../Features.md) for what is shipped vs planned.

## Where to start in VS Code

1. Unzip this package; open the **RetroDrive-Studio-Complete** folder in VS Code.
2. Read this index and `12-DEVELOPMENT-GUIDE.md`.
3. Run the existing scaffold using `npm install` then `npm run tauri dev` with Windows prerequisites installed.
4. Start with the Phase 1 issues in `11-ROADMAP-AND-BACKLOG.md`.
5. Use `14-AI-CODING-BRIEF.md` as the implementation guardrails for coding assistants.

**Important distinction:** The app today provides a UI, read-only volume discovery, a read-only folder indexer, and the normalized v1 catalog schema with migrations (RDS-001). It does **not** install or image Batocera, deploy ROMs, synchronize saves or perform repairs. Design documents describe the intended system, not proven features.

## Delivery contents
- `docs/`: design specifications and development handoff.
- `src/`, `src-tauri/`, configuration files: prior desktop starter source.
- `README.md`: scaffold capabilities and local prerequisites.

Keep the documents under version control, updated alongside implementation changes. Scope, data migrations and destructive functions require documented reviews.
