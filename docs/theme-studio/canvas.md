# Canvas editing: first authoring increment

This increment turns the scene overview into a real, restricted authoring canvas. It adds no native exporter/installer, package/media importer, runtime dependency, database table or source schema migration.

## Supported operations

- Add original text or panel layers to any scene, with project-wide unique IDs.
- Select from the layer list or canvas; selection bounds render above the other shapes.
- Drag a top-level text/panel layer. A completed pointer release commits one undoable operation. A click alone changes selection without moving source coordinates. Escape, lost capture, focus loss or undo cancels the uncommitted drag preview.
- Move the selection with canvas-focused arrow keys; Shift moves ten logical pixels. Repeated key events coalesce until key-up.
- Edit layer names, X/Y/width/height with explicit px/% units, static text, hex colors and pixel font size. Numeric sizing is implemented; resize handles and rotation controls are later work.
- Undo/redo committed project edits using toolbar buttons or Ctrl/Cmd+Z, Ctrl+Y/Cmd+Shift+Z outside text inputs. Text inputs retain native editing shortcuts.

Layers are a flat arena shown back-to-front. Nested/group/unknown node types are retained and selectable for inspection, but are not editable/rendered by this first slice. Text bindings display typed placeholders without querying libraries. No arbitrary styles, scripts, assets, fonts or external resources are executed/loaded.

## Source and preview contract

SVG positions/sizes use the existing layout dimensions. Pointer coordinates are converted through the actual canvas rectangle into logical scene space; moves preserve px/% units and dimension extension fields. Position values are rounded to six decimal units and bounded by existing document limits. Unit changes preserve physical geometry within that precision; positive sizes remain positive.

Supported style fields are text (string), color/fill (RRGGBB or RRGGBBAA), and fontSize ({value, unit: px}); additional properties are preserved. Preview uses safe literal text, strict hex colors and system sans-serif, bounded font sizes and bounded rendered text. Unknown color/font/text expressions and static text longer than 500 characters are preserved read-only in the inspector. Renaming a legacy text layer without an explicit text field initializes its prior caption so its visible text does not unexpectedly change.

Existing rotation/opacity are approximated safely in preview; numeric rotation/opacity authoring is described below. Nested transforms, anchors, variants, rich typography and animations are not resolved. The renderer is an editor approximation, not a native frontend or a compatibility guarantee. Existing compatibility reports continue to flag unsupported/unknown features; all exports remain disabled.

## History and persistence

EditHistory stores JSON snapshots, capped at 50 entries and 8,388,608 UTF-16 code units (roughly 16 MiB of string content before overhead). Older entries are evicted; history is session-only and resets when reopening or copying a project. Whole snapshots preserve future fields. New edits invalidate redo, no-op edits do not create history, and grouped field/keyboard changes undo together.

Undo/redo changes the current editable document and is autosaved as a draft through the existing serialized revision queue. It does not roll back an explicit SQLite checkpoint or installation history. Tests cover undo while another write is pending and ensure the reverted source is persisted at the next revision without overwriting the checkpoint. Save failure/conflict/recovery/close behavior is unchanged.

Canvas additions/mutations are blocked during copy operations or blocked project transitions. Pointer previews are local until committed, and are not claimed to survive process termination before release.

## Validation

Automated workspace checks cover pure creation/movement, unit/coordinate conversion, unique IDs, nested-source protection, opaque fields, history grouping/branching/eviction, undo during in-flight persistence, checkpoint/draft distinction, safe actual SVG rendering, and preservation of long/unknown source properties. The generated canvas-project.json fixture is compared with real JavaScript output and parsed/validated by Rust.

These checks do not simulate actual native pointer capture, DPI scaling or keyboard event dispatch. Manual checks remain required in the disposable QA catalog:

1. Create a blank universal project. Add a panel and text; select each through the layer list.
2. Drag text, then Undo/Redo once. Verify one complete drag is one operation; Escape during a drag leaves coordinates unchanged.
3. Edit X/Y and switch px/%; verify geometry remains consistent. Edit numeric width/height, font size and colors.
4. Focus the canvas and move with arrow keys/Shift. Verify nested/unsupported layers stay read-only.
5. Edit while autosave is pending, undo, save a checkpoint, leave/reopen and recover/discard the draft as before.
6. Save As and change the copy; verify the original remains independent. Reopen after native close in the same QA catalog.
7. Resize the app and check pointer scaling and keyboard focus. Native frontend export remains unavailable.

Snapping/guides, resize handles, layer grouping/reordering/lock/hide, image assets, advanced scene inheritance, library sample data, media/timeline/navigation editing, templates and actual native compilation remain later increments.

## Layer ordering

The Layer order controls move the selected layer one step backward/forward, or directly to the back/front. The layer list and SVG both use scene.nodes order from back to front. Boundary actions are disabled and produce no history entry. Selection stays attached to the same node ID after a move.

This increment permits ordering only when every layer in the scene is a leaf text/panel node. Nested or unknown types keep ordering read-only because their stacking semantics are unresolved. Controls also block edits during copy/transition operations. Node payloads, IDs, bindings, geometry, opaque extensions and other scenes stay unchanged. No schema migration, dependency, target writer or native export behavior is added.

Each order change is a separate undoable operation, autosaved through ProjectSession. Automated workspace checks cover source preservation, one-step and endpoint moves, no-op boundaries, missing selections/scenes, nested/unknown guards, actual rendered SVG order, callback guards and undo/redo through draft/checkpoint persistence. Contract checks and the production build also pass. Rust production code is unchanged and the Rust suite was not rerun for this increment.

Manual QA still required: overlap a panel and text, select text in the layer list, send it behind the panel, then bring it forward. Check the list/canvas agree, selection stays on text, Undo/Redo restores each order, Save checkpoint/reopen retains the order, and Save As keeps the original independent. Verify keyboard focus and disabled controls in the native app. Native compiler ordering remains unverified.

## Rotation and opacity

The inspector now edits rotation in degrees (-36,000..36,000, matching the existing document limit) and opacity (0..1). Rotation is about the top-left corner, matching the current SVG transform. Layer geometry units, style/layout extensions and other source fields are preserved. Missing opacity previews as 1 without materializing an edit when unchanged. Unknown opacity expressions or out-of-range stored opacity remain read-only. Nonfinite and out-of-range inputs do not commit. Existing leaf-layer restrictions and copy/transition disabled states still apply.

Changes use grouped field history and existing draft/checkpoint persistence. A fully transparent layer remains selectable from Layers. This is a numeric-property increment, without rotation handles, pivot/anchor controls or native export promises.

Executed checks: test:theme-workspace, test:theme-contract, production build and diff check. Tests include value boundaries, no-op/invalid rejection, extension preservation, opaque opacity read-only, actual inspector callbacks/disabled guards, undo/redo and SVG attributes. Rust is unchanged and was not rerun. Native QA remains: rotate overlapping text/panel layers, test opacity 0/0.5/1, select transparent layers from the list, undo/redo, checkpoint/reopen and verify a copy is independent. Check rotated hit testing and appearance at different window sizes; those interactions are not covered by server-render tests.

## Align to canvas

Six inspector buttons align a selected supported leaf text/panel rectangle to the left, right, top or bottom canvas edge, or center it horizontally/vertically. Bounds account for rotation around the current top-left pivot. Alignment uses the entire rectangular layer including transparent space, rather than text glyph bounds. Oversized layers may extend beyond the opposite canvas edge; alignment does not resize them.

Only the selected position axis changes, preserving its px/% unit and extension fields. Other geometry, style, IDs, opaque payload and scenes remain unchanged. Results round to six source-unit decimals as existing movement does. An unrepresentable target beyond +/-1,000,000 source units is refused. Already aligned or unsupported actions are disabled and produce no history entry. Each command otherwise creates one undoable edit through existing draft/checkpoint persistence. Copy/transition busy guards remain in effect. Nested/unknown layer types remain read-only.

Automated checks executed successfully: test:theme-workspace, test:theme-contract, production build and diff check. Cases cover all six alignments at 0, 90, -90, 45, 180 and 360 degrees, mixed dimensions, extension/source preservation, no-ops, unknown/nested layers, invalid targets, undo/redo and actual control callback guards. Rust production code is unchanged and the Rust suite was not rerun.

Native QA remains: align a text and a panel on each axis; rotate one 45 degrees and check its rectangular edge/center against the canvas; try portrait and widescreen projects; undo/redo, checkpoint/reopen and confirm copy independence. Check keyboard focus and disabled controls. Multi-selection/distribution, snapping/guides, resize handles, anchor semantics and native engine output remain future increments.

## Bottom-right resize handle

The selected supported leaf text/panel exposes one bottom-right resize handle. Dragging projects canvas pointer displacement into the layer's rotated local axes, keeping the top-left position and rotation fixed. Width/height retain original px/% units and dimension extensions. Changed sizes stop at one logical pixel and the existing maximum source value; unchanged axes retain their original precision, including existing subpixel dimensions. Text font size is unchanged; resizing changes its clipping rectangle. Handles may lie outside the canvas for off-canvas/oversized layers; numeric inspector sizing remains available.

Pointer motion is a local preview. A completed release creates one history edit; clicking without moving produces no source change. Escape, focus loss, pointer cancellation/lost capture and undo shortcuts cancel the preview. Keyboard arrows on a focused handle adjust local width/height by one logical pixel, Shift by ten; repeated key events group until key-up/blur. Arrow input during an active pointer preview cancels that preview. Copy/transition states remove handles and block commits. Unknown/nested layers stay read-only. Changes reuse existing serialized draft/checkpoint persistence.

Executed successfully: test:theme-workspace, test:theme-contract, production build and diff check. Tests cover inverse rotation math, positivity/limits, px/% conversion, untouched-axis precision and source/extension preservation, no-op/invalid/nested/missing guards, undo/redo and actual rendered handle visibility/disabled states. These tests do not simulate native pointer capture, focus or keyboard dispatch. Rust production code is unchanged and its suite was not rerun.

Manual QA: resize text/panel at 0/45/90 degrees, confirm the top-left stays fixed, shrink past the origin without negative sizes, click without moving, Escape/cancel mid-drag, undo/redo once per completed drag, focus the handle and use arrows/Shift, resize the app to test DPI scaling, checkpoint/reopen and check copy independence. Repeat in portrait/widescreen disposable QA projects. Native export remains disabled. Other handles, aspect locking, resize snapping and anchor/group semantics remain later work.

## Resize focus regression and workspace layout

Owner native QA reported that the visible resize handle did not resize, while Undo/Redo and checkpoint status worked. The earlier model/server-render checks did not exercise default browser focus events. A real Edge pointer regression reproduced zero committed resizes before the fix. Default pointer-down focus could move from the canvas to the focusable handle after the handler had focused the canvas, and the bubbled blur handler cancelled the gesture.

Resize pointer-down now prevents that default transfer, explicitly focuses the handle, and ignores internal SVG focus transitions in the canvas blur handler. Leaving the SVG still cancels an in-flight preview. Container queries use actual workspace width: below 1050px the inspector moves below the canvas; below 700px scenes and canvas stack. This accounts for the application sidebar and avoids retaining a tiny middle column merely because the overall window is wide. The canvas still fits its available area and project text scales with it; this does not add zoom or change source geometry on window resize.

Run `npm.cmd run test:theme-browser` with Node 22+ and installed Edge (default Windows location), or set RETRODRIVE_TEST_BROWSER to an installed Chromium executable. The script starts a loopback-only Vite fixture, creates a uniquely scoped temporary browser profile beneath node_modules/.cache, drives real CDP mouse/keyboard events and closes its own browser/server. It does not launch the native app, read a user browser profile or invoke app IPC. No new dependency or browser download is required. The fixture is not included in production builds. Existing workspace checks also type-check its TSX.

Executed: browser regression passes pointer resize/default focus, local preview without persistence, click/no-op, Escape, external blur, keyboard resize, workspace widths 850/650px, rotation at 150% device scale, and disabled handle; workspace, contract and production build checks also pass. The native Tauri app and catalog were not modified by the browser test. Native owner recheck remains: reopen the same QA project and verify dragging the handle, one Undo per release, keyboard sizing and ordinary-window layout; checkpoint/reopen remains the persistence check. Off-canvas handles still require numeric inspector sizing. New snapping/guides work remains deferred until this regression is validated.

## Grid, snapping and canvas guides

Native owner testing confirmed the earlier resize focus and normal-window layout fixes. This next increment adds separate Show grid and Snap to grid and canvas controls with 8/16/32/64 logical-pixel spacing. They are session-only aids and create no edit/history/checkpoint changes; they reset on project reopen. The pointer-transparent SVG pattern uses a per-canvas ID and bounded pattern geometry rather than thousands of grid elements.

Pointer movement first rounds the layer's top-left position to the grid, then prefers a nearby canvas edge or center when a rotated rectangular bound/center is within six screen pixels. Pointer resizing rounds local width/height to the grid, preserving the existing one-pixel gesture minimum. Alt bypasses snapping for that pointer event; final release uses the current Alt state. Clicking without moving still creates no edit. Keyboard arrows and numeric inspector edits retain their existing precision, unaffected by snapping. Committed edits retain existing px/% units and source extensions through the established move/resize services.

Gold guide lines mark exact canvas edge/center matches of the current preview's rotated rectangle, including transparent space. They do not query libraries or snap to other layers. Escape, focus loss and existing cancellation handling discard the preview and guides. Aid controls are disabled during an active preview or blocked transition. No new source fields, database migration, dependencies, IPC, Customize changes or target-file writes.

Executed checks: test:theme-workspace (rounding, negative coordinates, minimum sizes, rotated bounds and guide positions plus existing workspace regressions), test:theme-browser (real input, controls without document mutation, snapped resize, Alt bypass, center guides and cancelled snapped movement, plus resize/focus/DPI/responsive regressions), test:theme-contract, production build and diff check. Browser test runs installed Edge with a disposable profile; occasional locked-profile cleanup warnings leave only that generated cache profile for later cleanup. No user browser profile or native catalog is read. Native QA remains required: toggle grid/snap independently; move and resize text/panel in portrait/widescreen and at 45 degrees; hold Alt; verify exact arrows, Escape, undo/redo and checkpoint/reopen. The aids should not create a recoverable draft on their own.

Other-layer snapping, ruler/custom guides, persistent workspace preferences, multi-selection/distribution and layer locking/hiding remain later increments. Asset import and native export remain separately scoped work.

## Layer locking and hiding

LayerList now provides Lock/Unlock and Hide/Show for supported top-level leaf text/panels. Hidden layers remain in the list and selected IDs remain stable. Locked layers stay visible/selectable for inspection but cannot be moved, resized, aligned, renamed or restyled. Hidden layers are omitted from the canvas and stay read-only until shown. Ordering cannot move or cross locked/hidden layers. Flag controls remain available to unlock/show even when a layer is protected; they are disabled during copy/transition operations and for unsupported/nested layers.

Optional node.editor.locked/hidden booleans persist through existing draft/checkpoint/Save As services and use meaningful undoable edits. Absent flags default false. Other editor fields, source IDs, layout/style data and scenes remain intact. Unrecognized editor objects or non-boolean known flags stay preserved read-only instead of being coerced. These are editor preferences: Hide omits the layer from this editor preview, without promising native export visibility or implementing conditional visibility. Native export is still disabled. The source's existing generic extension map accepts this additive object without a source/database migration.

Changing protected state cancels an active pointer preview. Shared graph eligibility and commit guards protect content even if stale controls attempt an edit. Canvas selection and the layer-list controls remain separate from content editing. No new dependency, target-file writer, Customize/design record change or background external action.

Executed checks: test:theme-workspace covers flag no-ops, preservation/unknown-state guards, content-operation protection, reorder barriers, history and draft/checkpoint reopen through the persistence model, plus actual list callbacks/disabled states and rendered canvas omissions. test:theme-browser covers real pointer/keyboard attempts on locked layers, no resize handle, unlock restoration, hidden-layer restoration and locking mid-resize, in addition to existing grid/focus/DPI/layout regressions. Contract, build and whitespace checks pass. Rust production code is unchanged and its suite was not rerun. Browser profile cleanup can report a locked generated cache directory; user profiles/catalog are never used.

Native smoke check remains: lock text/panel and try drag/resize/alignment/property edits, unlock and repeat; hide/show from the list; undo/redo flags; save checkpoint, close/reopen the same QA catalog and verify flags persist; Save As and confirm flag changes in the copy are independent. Grid native QA remains pending too. Secure asset import and richer previews follow separately, before evidence-gated native export.

## Synthetic game/system text previews

Native owner checks confirmed image Fit/Fill, crop/reset/history and checkpoint/reopen persistence from the preceding increment. This next step adds original in-memory sample text/number metadata with no emulator installation or personal library requirement.

Canvas Preview data selects Sample game (default), Long text / Unicode, Missing metadata or Binding placeholders. These are session-only aids; selection never changes source/history, imports game metadata, writes a library record or saves sample values. Existing static layers remain literal. Recognized bound layers render the selected sample; missing/null values retain the visible {source} placeholder. Preview strings still cap at 500 characters and 20 rendered lines; newline text uses the existing literal tspan renderer. There is no automatic text wrapping/layout engine in this increment.

Text content source in the inspector selects retained Static text or one existing scalar source. game.title, game.description and system.name use kind text; game.year, game.players and game.rating use kind number. Numeric samples render plain numbers (rating 0..1), without new formatting expressions. Source changes preserve other binding slots and extension fields; static style.text is retained and reappears when unbound (or the layer name if no explicit static text existed). Explicit unbinding removes only bindings.text. Unknown or image-kind text-slot bindings are preserved read-only, not evaluated or fetched.

Binding changes are normal graph edits, subject to locks/hidden/nested/copy/transition guards, and use existing undo/redo, serialized draft/checkpoint and Save As services. No database/format version, backend command, permissions or dependency change. Samples are original fictional text and numbers, not licensed game data or supplied artwork. Actual native-engine support remains unknown; no exporter is enabled.

Executed checks: test:theme-workspace covers all source/kind mappings, fallback/Unicode/static restoration, unknown/image refusal, extra fields, lock/disabled/no-op guards, source immutability, history and checkpoint/draft reopening via the existing persistence model. test:theme-browser exercises actual selectors under StrictMode, normal/long/missing/placeholders, number kinds, static restoration, multiline output and no sample data in portable JSON; previous resize/grid/image/pixel/reopen regressions remain passing. Contract/build/license/whitespace checks pass. Browser native services remain stubbed; generated cache-profile cleanup warnings may occur. Rust was not rerun for frontend-only work.

Native smoke check: open a starter Game List scene (already bound), or add/select text and choose Game title. Switch all Preview data options; choose Year and Missing metadata; verify a visible placeholder. Return to Static text and verify the original caption. Test font/geometry, lock/unlock, undo/redo; checkpoint/close/reopen and confirm only the binding persisted, not a game record or selected dataset. No real library or emulator should be required.

Sample artwork/media, real catalog selection, navigation simulation, list/carousel rendering, automatic wrapping, custom data/fallback editors and native performance/compatibility previews remain later increments.
