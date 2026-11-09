# Universal theme source contract v1

**PNG attachment:** declarations keep the existing content-addressed PNG path and byte/hash/MIME/license/attribution fields, with portable provenance text only. Image nodes reference one declared asset ID. Local BLOB storage is not embedded in JSON; future package export must supply those bytes. Duplicate imports reuse existing project metadata and create independent image nodes. No source-format version change.

**Editor extension:** node.editor may contain locked/hidden booleans with false defaults, plus preserved future fields. This additive use of the existing extension map does not change v1.0.0 or require storage migration. Non-object editor data or non-boolean known flags are not coerced/overwritten. Flags are editor-only; exporters are still unimplemented and must not infer native conditional visibility.

The first canvas editor now interprets a safe subset of existing layout and style fields (text, color/fill, pixel fontSize), with immutable graph edits and no format/catalog migration. Unknown/nested data stays preserved; unsupported style expressions and oversized text are read-only. [Editor contract](canvas.md).

PR 2 now implements the authoritative SQLite working store and background project IPC; see [persistence](persistence.md). Source schema/portability are unchanged. References below to PR 1's lack of storage describe that increment only.

Sprint 1 PR 1 adds Rust models and pure validation in `src-tauri/src/theme_studio/`, matching frontend types in `src/theme-studio/types.ts`. This is a JSON source contract, not an implemented ZIP format or target exporter. The [master design](../RetroDrive_Theme_Studio_Master_Design_v1.md) remains the full specification.

## Portable source and local working state

Required root fields: `format: "retrodrive.theme"`, `schemaVersion: "1.0.0"`, id, metadata, targets, scenes, assets, tokens and variants. See the [blank](../../src-tauri/tests/fixtures/theme_studio/blank.json) and [original starter](../../src-tauri/tests/fixtures/theme_studio/starter.json) fixtures.

SQLite remains the authoritative local working store, to be implemented in PR 2. Revisions, drafts/checkpoints, local installation bindings and local library locations are working state, separate from portable JSON. No database migration or persistence command ships in PR 1.

An empty `targets: []` explicitly means **Universal / No Target Yet**. Authoring requires no emulator or connected installation. Target intents contain frontendFamily and nullable frontendVersion; they are not verified capability profiles. No absolute installation root or drive identity belongs in source JSON.

The future wizard must explicitly display Universal / No Target Yet. Future editor views will separate scene/layer navigation, central canvas, properties inspector and collapsible timeline. They will edit this graph through an editor command layer; renderer, persistence, validation and compiler remain independent. PR 1 adds no UI.

## Scene and node model

- Scene IDs are unique; node IDs are project-wide unique and stable across edits. IDs contain 1–128 ASCII letters, digits, dots, underscores or dashes. Display names support Unicode.
- Scene design roles: startup, main_menu, system_select, game_list, game_detail, launch, idle and exit. These do not imply target support. Unknown roles survive with a diagnostic.
- Scenes declare logical pixel resolution (1–16,384 per axis) and a flat node arena. Ordered child IDs define hierarchy/sibling z-order. Unreferenced nodes are roots in arena order. Missing children, cycles, repeated child entries and multiple parents are invalid; children cannot cross scenes.
- Nodes declare id, type, name, layout, style, children, assets and bindings. Known vocabulary: group, text, image, panel, shape, metadata, game_art and navigation. Unknown types retain the common envelope and full extra payload and produce a diagnostic; never silently lower them.
- Layout x/y/width/height each contain value and unit (px or %). Percentages refer to parent content dimensions, or logical scene size for roots. Position can be negative; size must be positive. Values are finite and at most 1,000,000 in magnitude; rotation is degrees within ±36,000. Anchors/cropping/visibility and other properties stay inert extension data until implemented.
- Bindings declare source and result kind: game.title/game.description/system.name → text; game.year/game.players/game.rating → number; game.artwork → image. No expressions, scripts or paths. Additional typed sources, including video, require future tests.
- Tokens, variants, styles and extensions retain JSON; their semantic interpretation/reference resolution is not implemented. Planned override precedence: global styles → theme variant → aspect ratio → system override → component override. Resolver tests are required before these affect preview/builds.

## Assets and portability

Each asset declares id, lowercase SHA-256, byte count, mediaType, portable path, license, attribution and provenance text (source).

| Declared media type | Relative content-addressed path |
|---|---|
| image/png, image/jpeg, image/webp, image/svg+xml | `assets/images/<hash>.png`, .jpg/.jpeg, .webp, .svg respectively |
| video/mp4 | `assets/video/<hash>.mp4` |
| audio/ogg, audio/mpeg | `assets/audio/<hash>.ogg`, .mp3 respectively |

Checks validate declarations/references only: they do not verify file content/hashes, decode media, sanitize SVG or establish target compatibility. Those remain secure import/export gates. Fonts/additional formats need explicit profiles. Metadata and asset licenses must be explicit (e.g. original/custom/unknown); unknown grants no redistribution rights.

Nothing is fetched or executed. Resource paths must exactly match the hash layout; traversal, URLs, platform separators and executable paths fail. All strings/keys, including extensions, are conservatively screened for leading POSIX/Windows roots, UNC/device/drive prefixes and case-insensitive file URLs. Thus free text beginning with a slash is refused too. This structural portability rule is not a future privacy/license audit; prose can still contain private information.

Local file paths belong in a later nonportable binding store. Future .rdtheme export/import will place this source at project/theme.json with a separate package manifest, assets, preview and attributions. Native frontend output remains a separate derivative.

## Preservation and validation boundaries

Every modeled object retains unknown fields through flattened extension maps. Nested future data is inert JSON. Fixtures cover animation tracks/keyframes, navigation, audio/video, components/templates, sharing, metadata, scene and dimension extensions.

Known layout numbers can normalize from 40 to 40.0; JSON whitespace/key order is not preserved. Unknown values round-trip. Duplicate object keys (including escape-equivalent keys) and extension collisions with typed fields are rejected rather than discarded. Numbers must fit ±9,007,199,254,740,991 for JavaScript interchange.

Only schema 1.0.0 is accepted. Other versions return a diagnostic; no format migration exists yet. Callers must retain failed/unsupported original input and never replace it with an empty project. Format migrations are independent of SQLite migrations.

Limits: 2 MiB JSON, 64 JSON nesting levels, 128 scenes, 10,000 total nodes, 2,000 assets, 16 target intents. These are initial document limits, not measured renderer budgets. parse_project is the bounded untrusted-input boundary; validate_project also checks constructed models, NaN/infinity and field collisions. Diagnostics contain code/path/message. Preliminary unsupported-feature observations are not a full capability analyzer or export approval.

## Checks

From repository root:

```powershell
npm.cmd run test:theme-contract
npm.cmd run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
python scripts/check-licenses.py
```

The Node contract check uses existing TypeScript, checks positive/negative authoring types and compares emitted blank and starter JSON with Rust's shared fixtures. It adds no dependencies; Windows CI runs it.
