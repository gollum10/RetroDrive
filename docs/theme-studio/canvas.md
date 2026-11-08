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
