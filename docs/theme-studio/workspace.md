# Theme Studio workspace (Sprint 1 PR 3)

Theme Studio is a separate sidebar workspace immediately below Customize. It uses the project service from PR 2 and does not replace Customize or its saved designs. No installed theme files are written.

The browser supports paged projects, refresh, create, resume/open and explicit duplication of a saved checkpoint. The wizard offers Universal / No Target Yet by default, optional frontend/version intent, project metadata, three display presets and original starter or blank scenes. No emulator or cloud account is needed in the desktop application. The browser-only development preview cannot persist projects.

Editor composition is separate components for scene/layer navigation, central scene overview, project-properties inspector and a collapsible future timeline. Scene selection works; metadata is editable. Canvas manipulation, layer manipulation, animation editing, native preview and export are not implemented. The overview does not execute styles, load media paths or pretend to render a native frontend.

## Persistence lifecycle

ProjectSession owns a serialized write queue outside the renderer. Each request uses the preceding acknowledged revision. Edits made during an in-flight request stay in memory and are subsequently persisted. Changes debounce into autosaved drafts; Save checkpoint is explicit. A pending request never overwrites newer local edits with its response.

Opening a project with a draft requires an explicit recovery/discard choice. Recovery resumes the draft without promoting it; discard returns to the checkpoint. Rust validates source via read-only validate_theme_document before the UI renders an editable model. Unsupported/corrupt source is shown read-only and remains unchanged.

Save failures retain local data and show retry controls. Conflicts cannot automatically retry; Save As preserves the local document as an independent project, while an explicitly confirmed reload discards local edits and reopens stored source. Save As uses create on the current local source, including edits not yet persisted; browser duplication explicitly uses the stored checkpoint.

App navigation calls the workspace leave guard before switching pages. It waits for queued writes and flushes remaining edits; a failed flush keeps the editor open. Project operations such as creation/opening temporarily block leaving. Back to projects also flushes. Native close requests are prevented until a successful flush; browser unload uses its standard unsaved-change prompt because unload cannot await IPC. These mechanisms do not guarantee recovery of edits killed before an autosave was acknowledged.

No new dependencies or catalog migrations. The isolated worktree remains in use.

## Validation

npm.cmd run test:theme-workspace compiles the real UI and exercises the document factory, queued writes/revisions, edits during a blocked save, explicit checkpoints, failure/retry/conflict retention and React server rendering of wizard, editor regions, browser empty state, sidebar order and existing Customize empty state. npm.cmd run test:theme-contract checks the portable wire contract.

These are lifecycle and server-render tests, not browser interaction, visual screenshot, gamepad or a native window-close smoke test. A manual Tauri run with a disposable catalog is still required before declaring the workspace fully validated on Windows. Build output, Rust tests/Clippy and license gate are checked separately. No live catalog or real installation is used.
