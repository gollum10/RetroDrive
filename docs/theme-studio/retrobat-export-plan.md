# First RetroBat exporter: evidence and delivery plan

Status: research/implementation plan, not a supported exporter. Reviewed 2026-11-09. No frontend binaries, themes or artwork were downloaded, installed or launched. No target installation was inspected or changed during this investigation.

## Evidence pinned for reproducibility

| Primary source | Revision | What it establishes |
|---|---|---|
| [Official engine source](https://github.com/RetroBat-Official/emulationstation/tree/22d8dfbe44fa324d448fea01b3f6a00b5ae76d68) | 22d8dfbe44fa324d448fea01b3f6a00b5ae76d68 (master) | Candidate source snapshot, not proof of the engine bundled with any RetroBat application release. |
| [Theme reference](https://github.com/RetroBat-Official/emulationstation/blob/22d8dfbe44fa324d448fea01b3f6a00b5ae76d68/THEMES.md) | Same source revision | XML format 7; named views/elements; additional elements; normalized geometry. These are candidate mappings only. |
| [Theme parser](https://github.com/RetroBat-Official/emulationstation/blob/22d8dfbe44fa324d448fea01b3f6a00b5ae76d68/es-core/src/ThemeData.cpp) | Same source revision | Property vocabulary must be checked against parser behavior before compiler implementation. |
| [Text renderer](https://github.com/RetroBat-Official/emulationstation/blob/22d8dfbe44fa324d448fea01b3f6a00b5ae76d68/es-core/src/components/TextComponent.cpp) | Same source revision | Vertical alignment defaults to center; font sizing uses the smaller screen dimension. The editor currently uses top-aligned SVG text and logical pixels. |
| [Branch sync metadata](https://github.com/RetroBat-Official/emulationstation/blob/6f9a918e6e82a03bfcee79df383b3780c738b211/.github/pull.yml) | 6f9a918e6e82a03bfcee79df383b3780c738b211 (autopull) | Default branch is sync/build metadata. It maps master/stable to upstream branches; it does not attest a shipped executable. |

Only links, revision identifiers and engineering findings are retained here. No upstream code, assets or theme XML is copied into shipped artifacts. Future fixture licensing must be recorded in docs/17 before vendoring material.

## Gate 1: exact installed target evidence

The historically observed application string 8.3.1.1-stable-win64 remains a candidate, not an engine identity. Ask the owner which existing installation to validate; do not scan arbitrary drives or install a frontend. Keep absolute installation paths local, outside universal project JSON and published fixtures.

Read-only capture checklist:

- Record application version, displayed engine version/build, Windows version and executable architecture. Unknown fields remain null; do not infer architecture from the application filename alone.
- Hash the selected frontend executable without running it. Hashing identifies the artifact but does not establish its source revision.
- Establish the relationship between that executable and an official source/build revision using release/build metadata. Record evidence and unresolved gaps separately.
- Record the theme-format declaration and supported view/property behavior. An existing theme declaration alone is not parser-version proof.
- Use original test text and generated PNGs. Do not copy installed commercial artwork, game metadata, credentials, ROMs, BIOS, usernames or absolute paths into Git fixtures.
- Native execution, theme activation and settings changes are a separate owner-scoped step. Capture can remain incomplete while a pure compiler is developed behind the disabled registry.

Evidence record fields: applicationVersion, frontendFork, frontendVersion, os, architecture, themeFormatVersion, sourceRevision, executableSha256, captureDate, provenanceLinks, unresolvedQuestions, nativeChecks. This is a proposed test-evidence record, not a database migration or trusted IPC input. No caller-supplied record can enable production support.

## Minimal vertical slice

Aim for an original system-select and game-list theme with static text and managed PNGs, plus game-title and description bindings where exact built-in components are verified. Retain working built-in navigation; do not label a static screenshot a complete usable theme. First output is an in-memory file manifest tested against golden fixtures, with no installation path argument.

Candidate mappings to verify:

| Universal concept | Candidate output / decision |
|---|---|
| system_select / game_list | system / detailed views; reject duplicate roles and unsupported scene overrides initially. |
| x, y, width, height | Normalize each axis against its scene resolution. Explicit top-left origin; refuse unverified pivot/rotation semantics. |
| Static text / PNG | Generated stable element names from source IDs, extra elements, escaped literal content, managed relative asset paths. |
| game.title / game.description | Candidate built-in md_name / md_description. Binding placement is view-specific; do not export placeholder/sample strings. |
| fontSize / textAlign | Explicit conversion and top vertical alignment. Native font metrics, wrapping, clipping and line spacing require comparison; no pixel-equivalence claim. |
| Panels / crop / rotation / opacity | Block until mapped and tested, or offer a separately documented explicit conversion. No silent omission or flattening. |
| Lock / hide | Lock is editor-only. Hide currently means editor-preview visibility, so do not invent native visibility semantics; unresolved non-default hide blocks export. |
| Other scenes, media, bindings, tokens and unknown fields | Preserve source; report blockers unless explicitly analyzed as non-rendering editor metadata. |

Do not export a starter project by silently dropping its startup/main-menu/launch/idle/exit scenes. Initial compiler tests should use deliberately scoped documents. Broader scene support or explicit scene-selection/fallback consent must precede export of full starter projects.

## Small PR sequence

1. **This PR: evidence and plan.** Pin primary sources, document capture fields, semantic gaps and acceptance gates. Registry remains empty; no runtime behavior changes.
2. **Pure compiler spike.** New Rust theme_studio/export/model.rs and export/retrobat.rs plus original fixture/golden tests. Add module wiring only. Borrow validated source; produce bounded deterministic paths/bytes/diagnostics in memory. No filesystem, IPC exporter or registered adapter yet. Test exact geometry, XML escaping, stable IDs/order, missing assets, unsupported features, nonfinite inputs and unchanged source. Define explicit limits before implementation.
3. **Managed asset resolution and manifest validation.** Reuse authorized immutable PNG storage; verify declared hash/length, references and licenses. Include only used assets; reject stale/mismatched blobs and unsafe/colliding paths. Add repeat-build hash tests and deterministic manifest metadata. Do not include timestamps or machine paths in reproducible output. No new database schema is expected.
4. **Native evidence comparison.** On the scoped exact target, manually test original fixtures for system navigation, selected-game updates, text geometry, images, aspect ratios and reload errors. Record screenshots/log summaries with provenance; distinguish passes, approximations and unsupported features. Do not enable a profile with unresolved critical evidence.
5. **User-facing offline build/export.** Add background IPC and UI only after the format/asset boundary is tested. Exact trusted profile matching, complete diagnostics, user-selected new output and preview of produced files. Existing directory collisions refuse writes; source packages remain separate. Review the output-writer safety scope before filesystem implementation.
6. **Installation later.** Dedicated theme deployment safety review and owner-authorized scope: dry-run, identity/free-space recheck, backup, staging, verification, recovery and rollback. Existing Customize paths/services remain unchanged. Batocera and ES-DE need independent evidence and exporters.

File locations are proposed, not implemented stubs. Each code PR updates Features, architecture, QA and target matrix; tests run against original disposable fixtures. Rust/compiler changes require targeted tests, full cargo test --locked and cargo clippy --locked --all-targets -- -D warnings, plus applicable frontend build/contract/workspace and license checks.

## Acceptance gates and current result

- Current result: pinned research and reviewed local contracts only; no exact installed engine verified, no compiler implemented, no native output produced.
- Compiler gate: deterministic bounded output; parseable escaped XML; all requested features accounted for; source and managed asset declarations preserved; unsupported data blocks output.
- Runtime gate: exact trusted profile, original native fixtures and navigation/metadata checks pass; documented editor/native differences are surfaced before export.
- Release gate: complete diagnostics, portable packaging/privacy/license checks, accessible build UI, filesystem safety tests and owner native acceptance. A passing synthetic unit test alone does not enable native support.

Validation for this documentation-only PR: inspect pinned upstream source via read-only GitHub API, compare current ThemeAdapter/ProfileKey/compatibility boundary and existing theme services, validate Markdown links and git diff --check. No application or Rust tests are claimed for this planning change. The next implementation task is the pure in-memory compiler spike; installed-engine evidence can proceed alongside it without weakening production gates.
