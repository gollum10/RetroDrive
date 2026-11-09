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
