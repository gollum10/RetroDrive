# RetroDrive changelog

What changed, newest first. For what RetroDrive does today, see [FEATURES.md](FEATURES.md); for the reasons behind each change, the [decision log](docs/13-DECISION-LOG.md) (ADR numbers).

## 2026-11-10

- **Downloadable releases (ADR-124):** pushing a version tag builds the installer and MSI on GitHub, with SHA-256 checksums, as a draft pre-release the owner reviews and publishes.

- **80s look (2026-11-10, ADR-123):** a sixth RetroDrive look in the logo's synthwave colours: purple night, hot pink neon, cyan glow and sunset-chrome headings. Pick it in the Dashboard's Look switch.

- **AI pictures (2026-11-10, ADR-122):** in Theme Studio's Add picture, describe a picture and make it with OpenAI or a picture program on your PC (Automatic1111, Forge or ComfyUI), then use it as a background or anywhere on the screen. It's credited as AI-made.

- **Other AI services (2026-11-10, ADR-121):** besides Claude and a private AI, connect OpenAI (ChatGPT), Google Gemini, OpenRouter, Mistral, Groq, DeepSeek, xAI, Together AI or Fireworks AI with your own key, and pick the model; Pro adds any service by address. Ask AI lets you choose which one to ask.

- **Ask AI (2026-11-10, ADR-120):** in Theme Studio, ask Claude or your private AI for ideas ("give it an arcade feel", "make the text easier to read"); see it on the screen at once, then Keep or Discard. Only the screen's layout and your words are sent.

- **AI helpers: connect (2026-11-10, ADR-119):** Settings → AI helpers. Connect your own Claude account (Claude Opus 5.5 by default, cheaper models selectable) or a free private AI on your PC or network. Your key stays in Windows Credential Manager; nothing is sent until you connect. Design help comes next.

- **Theme Studio clips (2026-11-10, ADR-117):** add your own looping MP4 clips anywhere, including as a moving background; they play in the editor and on RetroBat.

- **Theme Studio: Put on RetroBat (2026-11-10, ADR-115):** after saving a theme, put it on your RetroBat and switch to it in one click (never replaces a theme; Undo in Customize). Game videos play in your design on RetroBat, including inside a cabinet frame.

- **Theme Studio System Select (2026-11-10, ADR-116):** style RetroBat's system carousel (band colour and see-through, name colour, direction, how many systems show) and place the number of games; checked on RetroBat.

- **Theme Studio checked in RetroBat (2026-11-10, ADR-114):** saved themes were tried on RetroBat itself: pictures, game list, game details and hidden extras all appear as designed. A theme without a Game list layer now always gets RetroBat's list on the left.

- **Theme Studio: Save for RetroBat (2026-11-10, ADR-113):** save your design as a RetroBat theme in a new folder. Your art, panels, text, turning and fading are drawn into pictures exactly as you see them; the game list, game video and game details stay live. Credits for every picture are included.

- **Theme Studio game list (2026-11-10, ADR-112):** place and colour the game list (game names, selected name, highlight bar) on the Game List screen; the Ready layout includes one. Groundwork for saving themes for RetroBat.

- **Theme Studio game video (2026-11-10, ADR-111):** add a game video layer (plays the selected game's own video on the system) and put a frame picture over it, such as an arcade cabinet with a see-through screen.

- **Theme Studio: your art everywhere (2026-11-10, ADR-110):** add PNG, JPG or WebP pictures (AI-made ones too, credited as such) on the screen, as a full-screen background, or as a frame over another layer; delete layers with undo.

- **Theme Studio layout (2026-11-10, ADR-109):** the canvas, layers and properties now fit on one screen; screens are tabs across the top.

- **Theme Studio tool levels (2026-11-10, ADR-108):** choose **Simple**, **Standard** or **Pro** tools in the editor; the theme is the same in each. New themes start from a **Ready layout** with the game's title, year and description already placed.

- **Theme Studio in plain words (2026-11-10, ADR-107):** Theme Studio speaks plainly (Save version, Save a copy, Add picture, Made for: any system) and follows the app's look (Classic, Galactic, Bridge, Cartoon, Versus). Coming next, per the [roadmap](docs/theme-studio/roadmap.md): skill levels, your art and videos everywhere, saving for RetroBat, and an optional AI assistant.

## 2026-11-08 to 2026-11-09: Theme Studio groundwork

- **Native comparison preparation:** an opt-in test now generates original compiler-produced themes, inert game markers and a disposable home configuration without launching RetroBat. [Preparation and pending native checks](docs/theme-studio/native-comparison.md). Native behavior and launch isolation remain unverified.

- **Candidate package assembly:** a read-only catalog snapshot now resolves used managed PNGs, checks exact length/hash/signature and assembles bounded deterministic XML/assets/credits/manifest files in memory. Native verification, full licensing/privacy audit and user-facing export remain disabled. [Details](docs/theme-studio/retrobat-export-plan.md#managed-asset-package-spike).

- **Experimental RetroBat compiler:** a pure in-memory candidate compiler now produces bounded XML and used-PNG declarations for a strict subset. It does not enable export, resolve asset bytes or write target files. See [scope and evidence](docs/theme-studio/retrobat-export-plan.md#compiler-spike-implementation).

- **RetroBat export groundwork:** [pinned evidence and minimal exporter plan](docs/theme-studio/retrobat-export-plan.md) now defines the pure compiler, exact installed-engine capture and native acceptance gates. This is planning only; no exporter or installation support is enabled.

- **VIS-004 partial typography:** text layers now support left/center/right alignment within their existing rectangle, including sample-bound multiline captions. Unknown values stay preserved read-only; source edits use existing history and draft/checkpoint persistence. Wrapping, vertical alignment, custom fonts and native engine output remain unimplemented.

- **Theme Studio sample previews prepared:** synthetic normal/long/Unicode/missing text metadata plus placeholder mode, and typed text-source selection for title/description/year/players/rating/system name. Sample choices are session-only; binding edits use existing history/save. No emulator or personal library is read. Native acceptance remains pending.

- **Theme Studio image controls prepared:** Fit/Fill, non-destructive percentage cropping/reset and asset dimensions/format/size/license/attribution/hash details. Existing images default to Fit/full image. Pixel-level browser checks pass; native visual/save/reopen confirmation remains pending.

- **CI test reliability:** the copier confirmation test now gives each file pair a separate modification second and pins its confirmation to that pair. This removes a runner-timing assumption without changing production copying behavior.

- **Saved PNG preview fix prepared:** shared in-flight requests prevent StrictMode setup/cleanup from racing the native image-worker guard on reopen. Bounded busy retries and state-based preview publication restore saved image layers without touching stored source/bytes. Native owner recheck remains pending.

- **Theme Studio PNG workflow prepared:** user file picker with license/attribution, normalized immutable SQLite blobs, PNG image layers and authorized hash-checked previews. Images use normal move/resize/align/lock/hide/history services. Catalog v29 is additive; native import/save/reopen validation remains pending.

- **Theme Studio image-import boundary prepared:** bounded native PNG decode/normalization, metadata stripping, canonical byte hashing and portable image path generation. This is codec-only infrastructure; file import/storage, asset UI and image rendering are not implemented yet.

- **Theme Studio layer state prepared:** layer-list Lock/Unlock and Hide/Show controls persist optional editor flags with undo/redo and existing draft/checkpoint services. Locked/hidden layers reject content edits; hidden layers remain listed for recovery. Browser checks pass; native validation remains pending.

- **Theme Studio grid/snapping prepared:** separate grid visibility and optional snapping controls, 8/16/32/64 logical-pixel spacing, pointer move/resize snapping, Alt bypass and canvas edge/center guides. Controls are session-only and preserve the universal source format. Native QA remains pending.

- **Theme Studio resize regression fix:** handle pointer-down now owns focus and internal focus changes no longer cancel gestures. Workspace-width container breakpoints keep the canvas larger in normal windows. Real headless Edge input regression checks pass; native Tauri recheck remains pending.

- **Theme Studio resize:** selected leaf text/panel layers now have a bottom-right resize handle and handle-focused arrow/Shift-arrow sizing. Rotated local-axis math and existing undo/save services are reused. Native pointer/keyboard QA remains pending; export stays disabled.

- **Theme Studio alignment:** selected leaf text/panel layers can align left/right/top/bottom or center on either canvas axis, using rotated rectangular bounds and existing undo/save services. Native visual checks remain pending; export stays disabled.

- **Theme Studio transforms:** text/panel inspectors now author rotation and numeric opacity with undo/redo. Unknown opacity expressions remain read-only. Native visual verification remains pending; export stays disabled.

- **Theme Studio layer ordering:** selected text/panel layers can move backward/forward or to the back/front in flat supported scenes, with undo/redo and existing draft/checkpoint persistence. Nested/unknown scenes remain read-only for ordering. Native visual QA is pending; export remains disabled.

- **Theme Studio first canvas increment:** add/select top-level text and panels, pointer/keyboard movement, numeric sizing/unit conversion, basic text/color/font properties and bounded session undo/redo. Editor preview remains approximate; nested/unknown content stays read-only/preserved. No native export, asset import or installation. [Behavior and manual checks](docs/theme-studio/canvas.md).

- **Theme Studio project UX cleanup:** scoped dark controls replace native white inputs/buttons; Save As has a separate named confirmation form, disables original-property editing while open and announces copy creation. Suggested names use numbered copy suffixes, including legacy copy-copy names; identities/storage behavior are unchanged. Owner confirmed creation, checkpoint/draft recovery/discard, native close after restart, Save As creation and copy independence during disposable-catalog QA. Native visual recheck of this styling update remains pending.

- **Theme Studio native-close fix (2026-11-08):** stop re-requesting close from its own handler; await persistence and let the SDK complete the original close with explicit main-window destroy permission. Save failures still keep the app open. SDK/permission regression tested; actual owner close/reopen recheck pending. Existing checkpoint/recovery/discard behavior was confirmed by the owner.

- **Theme Studio PR 4: capability foundation implemented.** Read-only target profiles, exact theme-adapter contracts, five diagnostic outcomes, source-preserving feature reports and workspace compatibility panel. Production registry remains empty: every real target is unverified; no exporter/installer is enabled. [Evidence matrix](docs/theme-studio/target-matrix.md).

- **Theme Studio PR 3: partial workspace implemented.** Separate project browser and no-target wizard; metadata editing, scene/layer overview, future canvas/inspector/timeline regions, autosave/checkpoints, recovery and guarded navigation. No drag-and-drop, media editing, native preview/export or installation. [Workspace behavior and verification limits](docs/theme-studio/workspace.md).

- **Theme Studio PR 2 (2026-11-08): backend persistence implemented.** Additive catalog v27 stores universal checkpoints/drafts separately with revision conflicts, recovery/discard, rename and source duplication. Background IPC and typed frontend wrappers are available; workspace/wizard and UI autosave remain PR 3. [Storage contract](docs/theme-studio/persistence.md). Existing Customize, design records and target-write code remain unchanged.

- **Theme Studio (2026-11-08, Sprint 1 PR 1): partial foundation only.** Portable source types, validation and fixtures exist: no-target projects, scene/node graphs, typed bindings, asset declarations and future-field preservation. [Contract](docs/theme-studio/project-format.md) and [delivery tracking](docs/theme-studio/requirements.md). Workspace, SQLite project storage, visual editor, compiler, packages and deployment remain planned. Customize is unchanged.

## Before 2026-11-08

The library, drives, copying, sync, backups, controllers, Customize, setup wizards and app looks were built from 2026-11-04 on. Their history is in the [decision log](docs/13-DECISION-LOG.md) and the git history.
