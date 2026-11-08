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

Existing rotation/opacity are approximated safely in preview; new rotation/opacity authoring is not claimed. Nested transforms, anchors, variants, rich typography and animations are not resolved. The renderer is an editor approximation, not a native frontend or a compatibility guarantee. Existing compatibility reports continue to flag unsupported/unknown features; all exports remain disabled.

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
