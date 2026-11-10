# Theme target matrix and capability boundary (Sprint 1 PR 4)

**Experimental RetroBat compiler:** a pure in-memory candidate compiler now produces bounded XML and used-PNG declarations for a strict subset. It does not enable export, resolve asset bytes or write target files. See [scope and evidence](retrobat-export-plan.md#compiler-spike-implementation).


**RetroBat export groundwork:** [pinned evidence and minimal exporter plan](retrobat-export-plan.md) now defines the pure compiler, exact installed-engine capture and native acceptance gates. This is planning only; no exporter or installation support is enabled.


This is a discovery/evidence matrix, not a supported-exporter list. No production theme adapter is enabled and export/deployment always remain disabled in this increment.

| Family | Evidence currently available | Missing before native support |
|---|---|---|
| RetroBat | Existing read-only adapter reads application version.info. Sanitized synthetic fixture uses the repository's historically observed 8.3.1.1-stable-win64 release string. | Actual bundled ES fork/engine version, exact theme format, target OS/architecture evidence, licensed representative source/output fixtures and native screenshots/smoke tests |
| Batocera | Existing installation detection; SHARE does not expose a verified version through the current adapter. | Exact release/platform/engine/format evidence and native tests; do not assume RetroBat's schema |
| ES-DE | Existing location detection; independent theme format assessment remains future work. | Separate versioned engine/schema assessment and fixtures after the first two exporters |

## What is implemented

TargetProfile separates applicationVersion from an optional exact engine key. Application-release detection never fills that key. Saved installation bindings are local-only report data and are not inserted into portable projects. Live detection reuses existing adapters; disconnected or unrecognized locations do not acquire invented release/engine versions. Detection does not update the catalog or create a share identity.

ThemeAdapter is independent of the existing game-copy PlatformAdapter. The initial trait exposes a pure capability query keyed by exact frontend family, fork, engine version, OS, architecture and theme-format version. There are no writable path parameters. Compiler/IR methods will be introduced alongside a real compiler rather than shipping fake exporter implementations.

Matching requires one unique complete exact key. Empty/unknown/wildcard keys, mismatched versions/platforms/forks/formats and duplicate registrations fail closed. A capability rule requires explicit evidence. Convert/degraded outcomes require a fallback and explicit future user consent. Unknown/unsupported outcomes are errors; conversion/degradation produces warnings; native is informational. Capability matching never authorizes installation.

Feature inventory covers scene roles, node types, layout/relative/rotation needs, style keys, typed bindings, declared media, tokens/variants and opaque extensions at modeled object boundaries. Source is borrowed and preserved; unsupported animation/media/navigation data is not discarded. A report is capped at 1,000 issues with an explicit omitted count. No truncated report permits export.

IPC commands theme_target_profiles and theme_compatibility run on background workers. Callers choose only a saved installation ID or project intent; they cannot assert a verified engine tuple. The overview's compatibility panel can inspect detected local profiles and report reasons/fallbacks. Intent, release detection, generic editor overview and verified native behavior remain distinct. All real profiles currently report unknown support.

## Fixture provenance

src-tauri/tests/fixtures/theme_studio/target-profiles.json contains exact, unknown and mismatched **synthetic** profile fixtures. test-es-fork, test-1 and test-format-1 are invented names. Mock capability outcomes are contract tests only and are never registered in production.

The read-only discovery test creates an empty synthetic RetroBat.exe marker, ordinary folders and version.info under a temporary directory. It confirms the application release is read without creating an engine profile, modifying catalog rows, changing the version file or creating .retrodrive. This is not a captured real RetroBat engine or a native export smoke test.

No upstream theme documentation, native frontend binary or exact engine fixture was revalidated in this increment. The first real exporter must pin official source/doc revisions, asset provenance, engine/version/platform fixtures and native behavior before registering support. Current historical app detection evidence alone is insufficient.

## Verification and remaining work

Tests cover all five outcome shapes, consent/fallback metadata, exact tuple mismatches, unknown versions, ambiguous/unevidenced matches, opaque source preservation, pure discovery and bounded reports. Full workspace/contract checks, TypeScript build, Rust suite/Clippy and license gate also run.

The four Sprint 1 PRs establish the foundation, not a completed visual authoring release. Native workspace interaction smoke checks, real target capture, canvas/layer editing, undo/redo, asset sandbox/import, library preview data, actual compiler/export, packaging, media/timeline/navigation authoring and reviewed deployment remain unimplemented or unverified. No real installation is changed.
