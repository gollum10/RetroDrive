# Image import: PNG normalization boundary

This first image-import increment implements a pure native codec boundary only. It adds no filesystem reads/writes, IPC, import button, asset browser, image nodes or preview URL scheme. Those remain the next incremental PR, using this boundary before any managed storage writes. Existing projects, Customize, game media and target installations are untouched.

## Accepted input and output

The function accepts an already bounded byte slice, never a filename/URL. It verifies PNG signature, chunk lengths/count, CRC32 for every chunk, one leading IHDR, dimensions and a final IEND without trailing bytes. Animation control/frame chunks are rejected wherever they occur. Decoder errors reject invalid compressed pixel data and unknown critical chunks. SVG, JPEG, WebP, GIF and arbitrary renamed non-PNG data are refused in this first slice.

Accepted PNG samples are decoded to RGBA8; palette transparency, grayscale/alpha, RGB/alpha, low-bit-depth grayscale and 16-bit high-byte reduction are covered. A new encoder writes only normalized pixels, without source text, source profile, EXIF or other ancillary metadata. Hashing and the portable assets/images/<sha256>.png path use the newly encoded bytes, not the original container. The normalized result has no filename, absolute path or source provenance string. Future UI must collect license/attribution separately without leaking source paths.

Color profiles and gamma metadata are not carried into output; high-depth samples reduce to eight bits. This is not a color-managed archival conversion and appearance can differ from profile-aware viewing. Animation, original metadata preservation and more codecs are intentionally not accepted yet.

## Resource limits

- Input: 16 MiB maximum; future file readers must enforce this while reading, before allocating the full source.
- Dimensions: each axis 1..4096 and at most 4,194,304 pixels total.
- Framing: at most 4096 chunks with checked arithmetic and complete framing.
- Decoder scratch configuration: 16 MiB best-effort library budget. Output allocations are bounded separately and use fallible reservation.
- Pixel buffers: at most 16 MiB each for decoded output and RGBA8 conversion.
- Encoded output: 20 MiB writer limit.

These are explicit input/allocation limits, not a process sandbox or hard CPU/RSS deadline. The codec runs in-process; future IPC must schedule it on a worker and limit concurrency. No catch-unwind or claim of complete adversarial isolation is made. More aggressive media formats need a separate sandbox/resource design.

## Dependency evidence and tests

Uses pinned png 0.18.1 and crc32fast 1.5.4 alongside existing SHA-256. Both were already present transitively in the repository lock; direct runtime use is recorded in the component inventory and generated notices. APIs were checked against primary decoder/reader documentation:
- https://docs.rs/png/0.18.1/png/struct.Decoder.html
- https://docs.rs/png/0.18.1/png/struct.Reader.html
- https://docs.rs/png/0.18.1/png/struct.Transformations.html

Tests generate original tiny images in memory: color conversion/alpha, metadata stripping, palette transparency, malformed ignored profiles/text, low and high sample depth, hash/path stability and re-normalization, every truncation point, checksum corruption, invalid compressed pixels with valid CRC, duplicate/unknown critical chunks, wrong formats, oversized bytes/dimensions, animation markers, chunk floods and output-write limits. No downloaded or licensed artwork is added.

## Next boundary

Managed content-addressed storage must have scoped destinations, no-replace publication, read-back verification, link/reparse-point handling and disposable filesystem tests before IPC/UI wiring. Preview must authorize assets through saved project declarations and serve only normalized bytes with correct MIME/CSP; it must not expose arbitrary source paths. Native source-package import/export and target deployment remain separately reviewed work.

## Executed validation for this PR

- Full `cargo test --locked --quiet`: 313 passed, 7 ignored, including seven new PNG tests.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `npm.cmd run test:theme-workspace`, `npm.cmd run test:theme-contract`, `npm.cmd run build`: passed.
- License gate: 298 components, zero denied; generated dependency inventory/notices remain current, with no dependency version changes.
- `python scripts/third-party-notices.py --check` and `git diff --check`: passed.

No browser interaction suite was rerun because no frontend behavior changed. No native UI, real catalog, user image file or target installation was touched. Managed-store races/link handling and native end-to-end import remain unimplemented.

## Managed catalog import and preview

The next increment now connects static PNG import to the visual editor. The user selects a PNG through a browser File input, reviews its name/byte count and license/attribution, and explicitly chooses Import and add image. Only bounded file bytes enter native IPC; no absolute source filename/path is persisted or accepted by the native image commands. Header checking in JavaScript is convenience only; the native codec remains authoritative. File grants make the chosen bytes available without adding arbitrary filesystem read permissions. Original files are never changed.

### Scoped write and storage review

Catalog migration v29 creates theme_image_assets: SHA-256 primary key, normalized PNG BLOB, dimensions, byte count, imported_at. It reuses the existing backup-before-upgrade, transactional migration, foreign-key check and newer-version refusal. A v26 fixture removes the new additive table before simulating an old catalog; v28 upgrade/backup and conflict rollback have separate tests. No legacy designs/settings/installations are rewritten.

This intentionally replaces the proposed loose-file tree with immutable catalog blobs: there is no new destination path or symlink/reparse-point surface to secure. Both IPC commands accept IDs and/or bytes, never OS paths. A transaction inserts only absent hashes under 256 MiB and 2,000-entry content quotas. Duplicates compare exact bytes and reuse content; conflicts are errors, never replacements. The stored bytes are read back and rehashed before commit. The table's creation timestamp/hash records the managed import; license/attribution remain project-specific JSON. Base64 0.23.1 is already locked transitively and becomes a direct dependency without changing versions.

A try-lock permits one native image operation at a time; busy replies are retriable rather than queuing unlimited decodes. The command runs on a blocking worker. Base64 length is bounded before decoding, with the codec's input/pixel/output limits still applied. This is not a process sandbox or total IPC-memory/RSS guarantee; only the legitimate picker flow bounds bytes before invocation. SQLite may still fail for disk space/locks; errors preserve existing rows and pending source edits. Quotas cover content bytes/count, not WAL/backup overhead or a disk-space reservation.

### Attachment and preview authority

The import command verifies the local project exists, stores normalized bytes and returns dimensions/hash/byte count/portable path. React validates the descriptor and creates an image node/asset declaration using current source, keeping the chosen license/attribution and a fixed portable provenance string. Duplicate imports within a project retain existing declaration metadata and create another independently editable node. The attachment is one undoable edit and autosaves through ProjectSession. Shared blobs let Save As copies reference identical pixels while source metadata/layout stay independent.

Undo or an attachment failure does not delete bytes. Leaving the editor while an import is pending discards attachment after unmount; a completed blob can remain unreferenced. No automatic garbage collection/delete is introduced; retained bytes count against quotas. Project/source validation failures leave the original checkpoint intact and use existing save-error/retry behavior. A later cleanup feature must be separately scoped and user reviewed.

Preview accepts only saved project ID plus asset ID. It parses current draft-or-checkpoint, requires a declared PNG, finds the exact stored hash/byte count under the output limit, and verifies bytes/hash/signature before returning data:image/png;base64. A missing/corrupt/unauthorized asset is refused. There is no arbitrary path or remote URL. Source declarations not saved yet retry after autosave/revision changes or the Retry previews action. The cache is project/hash scoped, discards unused scene entries, skips hidden layers and bounds active references to eight PNGs/32 MiB; decoded image memory is additional (up to eight codec-sized images). Leaving/reopening clears the cache. The existing img-src data policy suffices; CSP/permissions and game-media protocols do not change.

Canvas image rendering accepts only bounded PNG data URLs from that map, never style hrefs/asset path strings. SVG image rendering fits pixels inside the layer rectangle. Existing graph guards now include image nodes for movement, sizing, alignment, ordering, lock/hide and common inspector properties; no image crop/effect tools are claimed.

### Validation and native acceptance

Native disposable-catalog tests cover dedup/reuse, unrelated project and traversal-like asset IDs, authorization before/after draft persistence, checkpoint copy preview, corrupt content refusal without replacement, base64/format/byte/count/quota rejection, read-back failure rollback, v28 backup/preservation/idempotence and migration-conflict rollback. Workspace tests cover descriptor/path guards, metadata reuse, unique image IDs, undo/redo, header/empty-file rejection, safe/malicious URL rendering and form labels. Real browser tests select a generated PNG through the actual form, prevent duplicate submit, confirm source filename exclusion, decode the image and resize/hide/show it. The browser fixture stubs the native storage service; it is not a Tauri IPC end-to-end test. Original codec tests remain in the full Rust suite.

Native owner smoke check still required in the same disposable QA catalog:
1. Import an original static PNG; set license/attribution; verify pixels, move/resize/rotate, lock/unlock and hide/show.
2. Undo/redo the import, checkpoint, close/reopen and confirm the image/metadata persist.
3. Import the same PNG again: two layers, one declaration/content blob; metadata stays with the existing declaration.
4. Save As and modify the copy, verifying the original layout/metadata remain independent.
5. Try malformed/non-PNG/oversized inputs; confirm clear rejection, original file/project unchanged and retry possible.

JPEG/WebP/SVG, portable asset packages, replacement/deletion/garbage collection, advanced asset browsing/cropping, media and native theme export remain future work.

Automated result for this workflow PR: full Rust suite 325 passed / 7 ignored; strict Clippy passed; workspace, source-contract and production build checks passed; real Edge fixture import/layer regressions passed; license gate and generated notices/inventory checks passed (298 components, zero denied). Browser tests may leave a generated cache profile locked during cleanup. No user browser profile, native catalog or target installation was used.

## Saved-preview lifecycle regression

Native owner testing confirmed two imports displayed, but reopening showed image layers with Image unavailable placeholders. A read-only inspection of the disposable QA catalog found the declared PNG blob present with matching SHA-256, byte count and PNG signature. Source/bytes were not repaired or rewritten.

The earlier browser import fixture passed preview URLs directly to the canvas and did not exercise the actual loading hook on mount. A new fixture starts with saved declarations, mounts useImagePreviews under React.StrictMode and models the real native one-worker busy guard. It reproduced zero image elements: the first effect's request was cancelled for publication, while the repeated effect made another request that was rejected as busy.

The hook now shares in-flight promises by project ID, asset ID, hash and byte count across effect cleanup/setup. Cancelled consumers cannot publish; the active consumer puts completed URLs into React state. Render uses that immutable state filtered against current declarations, rather than reading mutable ref cache contents. Cache entries remain bounded and project-scoped, and removed/changed declarations cannot reuse stale URLs.

Only image_busy replies auto-retry: at most four attempts with 100/200/300ms delays. Other errors surface immediately, and exhausted retries stop with Retry previews available. There is no infinite polling or change to native authorization, saved source, BLOBs, schema, CSP or permissions.

Executed browser checks cover the exact pre-fix failure, shared request count, actual component reopen, transient busy recovery, authorization after project change, declaration removal/restoration, permanent-error no-retry/manual recovery and bounded busy exhaustion/manual recovery, together with existing import/editor regressions. The source-only native service is stubbed in this browser test; no real catalog or user browser profile is used by it. Native owner recheck remains: reopen the existing QA project without reimporting and verify both saved images appear, then checkpoint/close/reopen again. Rust is unchanged and its suite was not rerun for this frontend-only fix.
