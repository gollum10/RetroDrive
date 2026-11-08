# Theme Studio security boundaries

PR 1 adds pure JSON domain code. No Tauri commands, schema migrations, installed-theme writers, extractors or rendering code. Existing Customize and disk services are unchanged.

Untrusted source enters bounded parse_project: version/shape, duplicate keys, size/depth/counts, absolute paths, graph/reference checks, typed bindings, finite layout and asset declarations. Unknown data stays inert. Constructed models need validate_project before persistence/serialization. No path fetches resources, evaluates expressions, renders SVG or executes theme content.

Later boundaries require separate tests: actual asset bytes/hash/codec/SVG checks; archive traversal/links/bomb/payload limits; safe preview properties; explicit compiler reporting of unsupported data; sharing privacy/license audit. Structural validation does not replace these gates.

Installation requires a dedicated scoped safety review and owner-authorized plan before target mutations. Reuse identity checks, stale-review SHA protection, no-replace renames, read-back verification, retained backups and journaled recovery where appropriate. Existing docs/18 authorization is not blanket approval for a new deployment service. No automatic boot/system writes or deletion of working themes/backups.

Development is isolated in .worktrees/theme-studio on feature/theme-studio-schema. The original checkout's branch and uncommitted planning documents, and the existing RetroInstaller-wip checkout, are preserved. A Git-local info/exclude entry hides .worktrees; no tracked ignore rule changes. Build outputs are isolated; Git refs/object storage remain shared. Do not reset/switch/force-push another checkout's branch.
