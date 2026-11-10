# Theme Studio roadmap (from 2026-11-10)

Owner, 2026-11-10, handing Theme Studio to Claude: "make sure that Theme Studio is flexible and gives the user the ability to create their own truly customizable design. It should feel like they can add their own art everywhere, even if they generate images from AI. It would be nice if the studio offered the ability to connect to a Frontier model or their own Private AI to assist with design. The end goal is to allow the end user to control every aspect of the design and really make it feel like their own through tools matching their own skill level. It should feel seamless to the user." Also: panels that play videos, like the small videos inside arcade-cabinet art in the owner's installed RetroFE setup.

This replaces the "narrow the editor" idea: the editor stays flexible, and exporting adapts the design to each system (ADR-107).

## Principles

1. **Your design is never cut down.** The editor offers everything; when a theme is saved for a system, anything that system can't draw natively (panels, rotation, opacity, effects, stacked art) is **baked into pictures** at that step. A feature that can't be baked (for example something that must change per game) is explained before saving, never silently dropped.
2. **Your art everywhere.** Every screen background, panel, frame, logo spot and decoration can take your own picture or video. AI-made pictures are just pictures: the picture form asks for credit and license, and offers "made with an AI tool" so the theme's credits say so.
3. **Tools for your skill level.** One switch: **Simple** (start from a layout, swap art and colours, ask the assistant), **Standard** (layers, move/resize, text and pictures), **Pro** (every property, exact numbers, per-system tweaks). The switch only shows or hides tools; the theme is the same.
4. **Optional AI help, off until you turn it on.** Connect your own account with a frontier model (Claude by default) or a private model on your own PC or network (any OpenAI-compatible server such as Ollama or LM Studio). The assistant suggests changes you see on the canvas and accept or undo. Nothing is sent anywhere until you connect it; each request shows what is sent. RetroDrive itself stays free and has no account of its own.
5. **Seamless and safe.** Plain words, follows the app's look, autosave and undo everywhere. Saving a theme for a system writes only to a new folder; putting it on a system goes through Customize's existing apply, with backup and undo.

## Steps (one PR each)

| # | Step | What the user gets | Notes |
|---|---|---|---|
| 1 | **Plain language and looks** (done, #26) | Plain wording on every Theme Studio screen; the editor follows Classic/Galactic/Bridge/Cartoon/Versus; garbled characters fixed | No data or behaviour changes |
| 2 | **Skill levels** (done, ADR-108) | Simple / Standard / Pro switch, remembered; new themes start from a Ready layout | Followed by a layout pass (ADR-109): properties beside the canvas |
| 3 | **Art everywhere** (done, ADR-110) | Picture fills for screen backgrounds and panels; JPG and WebP pictures as well as PNG (converted to PNG on import); frames with see-through areas; "made with an AI tool" credit | Image store stays bounded and checked |
| 4 | **Videos** (4a game video done, ADR-111; 4b your own clips next) | A video layer: **your clip** (loops, muted) or **the selected game's video**, placed anywhere, with a frame picture over it (the arcade-cabinet effect) | MP4/WebM; size limits; preview in the editor |
| 5 | **Save for RetroBat** (5a game list layer and RetroBat mappings done, ADR-112; 5b baking and saving next) | Save the theme to a new folder you choose; unsupported features baked into pictures; a clear list of anything that changes | Builds on the existing compiler and packager; writes only to a new folder |
| 6 | **Try it in RetroBat** | RetroDrive opens RetroBat with the theme in a separate test home, so your setup isn't touched | First time RetroDrive runs RetroBat: owner go-ahead and a safety review first |
| 7 | **Use this theme** | Apply to your RetroBat through Customize (backup, undo) | Reuses Customize's apply path |
| 8 | **AI assistant** | Connect a frontier or private model; ask for layouts, colour schemes, text; suggestions appear as undoable edits | New network feature: privacy and safety review first; keys kept in Windows Credential Manager, never in theme files |
| 9 | **More systems** | Batocera and ES-DE; possibly RetroFE (the owner's OneSauce setup) | Each needs its own evidence and exporter |

Order can change with the owner. AI picture generation (asking the assistant to *make* an image) is possible later through providers that offer it or a local image model; until then, pictures made in any AI tool can simply be added.

## What exists (built 2026-11-08 to 11-10)

Projects with autosave, saved versions, recovery and copies; eight screens per theme; text, panel and picture layers with move, resize, rotate, opacity, align, snap, grid, lock, hide, order and undo; sample game data for previews; a checked PNG store; an off-by-default RetroBat compiler and packager for a small subset; a test kit for a separate RetroBat home that has never been run. Details: [requirements](requirements.md), [export plan](retrobat-export-plan.md), [native comparison](native-comparison.md).
