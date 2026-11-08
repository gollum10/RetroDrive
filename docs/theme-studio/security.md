# Theme Studio security boundaries

PR 2 adds only managed-catalog writes via bounded validated source and transactional/revision-checked project commands. No caller may select a database or writable target path. Checkpoint/draft recovery and additive migration are tested in disposable catalogs. Uneditable source is returned intact rather than replaced with defaults. No installation, package, asset or game file is touched; [storage contract](persistence.md). The no-IPC statement below applies to PR 1 only. Work in this same isolated checkout now uses branch `feature/theme-studio-persistence`; PR 1 was merged by the owner, so PR 2 targets main directly.

PR 1 adds pure JSON domain code. No Tauri commands, schema migrations, installed-theme writers, extractors or rendering code. Existing Customize and disk services are unchanged.

Untrusted source enters bounded parse_project: version/shape, duplicate keys, size/depth/counts, absolute paths, graph/reference checks, typed bindings, finite layout and asset declarations. Unknown data stays inert. Constructed models need validate_project before persistence/serialization. No path fetches resources, evaluates expressions, renders SVG or executes theme content.

Later boundaries require separate tests: actual asset bytes/hash/codec/SVG checks; archive traversal/links/bomb/payload limits; safe preview properties; explicit compiler reporting of unsupported data; sharing privacy/license audit. Structural validation does not replace these gates.

Installation requires a dedicated scoped safety review and owner-authorized plan before target mutations. Reuse identity checks, stale-review SHA protection, no-replace renames, read-back verification, retained backups and journaled recovery where appropriate. Existing docs/18 authorization is not blanket approval for a new deployment service. No automatic boot/system writes or deletion of working themes/backups.

Development is isolated in .worktrees/theme-studio on feature/theme-studio-schema. The original checkout's branch and uncommitted planning documents, and the existing RetroInstaller-wip checkout, are preserved. A Git-local info/exclude entry hides .worktrees; no tracked ignore rule changes. Build outputs are isolated; Git refs/object storage remain shared. Do not reset/switch/force-push another checkout's branch.
