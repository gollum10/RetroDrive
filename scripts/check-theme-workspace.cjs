const ts = require('typescript');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Module = require('node:module');
const root = path.resolve(__dirname, '..');
const cache = path.join(root, 'node_modules/.cache');
fs.mkdirSync(cache, { recursive: true });
const output = fs.mkdtempSync(path.join(cache, 'theme-workspace-'));

async function main() {
  const files = ['tests/theme-studio/workspace.ts', 'src/App.tsx', 'src/ThemeStudio.tsx'];
  const program = ts.createProgram(files.map(f => path.join(root, f)), {
    target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS, strict: true,
    jsx: ts.JsxEmit.ReactJSX, esModuleInterop: true, skipLibCheck: true,
    outDir: output, rootDir: root, resolveJsonModule: true,
    types: [],
  });
  // Vite's ambient asset declarations are included explicitly.
  const withAssets = ts.createProgram([...files.map(f => path.join(root, f)), path.join(root, 'src/vite-env.d.ts')], program.getCompilerOptions());
  const errors = ts.getPreEmitDiagnostics(withAssets);
  assert.equal(errors.length, 0, ts.formatDiagnosticsWithColorAndContext(errors, { getCanonicalFileName: f => f, getCurrentDirectory: () => root, getNewLine: () => '\n' }));
  assert.equal(withAssets.emit().emitSkipped, false);
  const { ProjectSession, project, form, record, newDocument, suggestCopyName, guardedWindowClose, EditHistory, addNode, moveNode, updateNode, rootNodes, pixels, positionFromPixels, pointerToScene, previewColor, canOrderLayers, orderLayer } = require(path.join(output, 'tests/theme-studio/workspace.js'));
  assert.deepEqual(project.targets, []);
  assert.equal(project.scenes[0].resolution.width, 1080);
  assert.equal(project.scenes[0].resolution.height, 1920);
  assert.equal(new Set(project.scenes.flatMap(s => s.nodes.map(n => n.id))).size, 8);
  assert.equal(newDocument({ ...form, target: 'retrobat', targetVersion: '' }).targets[0].frontendVersion, null);
  assert.equal(newDocument({ ...form, template: 'blank' }).scenes.flatMap(s => s.nodes).length, 0);
  assert.equal(suggestCopyName('QA Test'), 'QA Test (copy)');
  assert.equal(suggestCopyName('QA Test (copy)'), 'QA Test (copy 2)');
  assert.equal(suggestCopyName('QA Test (copy 9)'), 'QA Test (copy 10)');
  assert.equal(suggestCopyName('QA Test copy copy'), 'QA Test (copy 3)');
  assert.equal(suggestCopyName('QA Test', ['qa test (COPY)', 'QA Test (copy 2)']), 'QA Test (copy 3)');
  assert.equal(suggestCopyName('Copy'), 'Copy (copy)');
  assert.ok(Array.from(suggestCopyName('🎮'.repeat(120))).length <= 120);

  const source = { ...project, futureAnimation: { tracks: [{ opaque: true }] } };
  const sourceJson = JSON.stringify(source);
  const added = addNode(source, source.scenes[0].id, 'text');
  const second = addNode(added.project, source.scenes[1].id, 'text');
  assert.notEqual(added.id, second.id, 'new IDs must be project-wide unique');
  assert.equal(JSON.stringify(source), sourceJson, 'editing helpers do not mutate the source');
  const scene = added.project.scenes[0];
  const moved = moveNode(added.project, scene.id, added.id, 540, 960);
  const movedNode = moved.scenes[0].nodes.find(node => node.id === added.id);
  assert.equal(movedNode.layout.x.unit, '%');
  assert.equal(movedNode.layout.x.value, 50);
  assert.equal(movedNode.layout.y.value, 50);
  assert.deepEqual(moved.futureAnimation, source.futureAnimation);
  assert.deepEqual(moved, JSON.parse(fs.readFileSync(path.join(root, 'src-tauri/tests/fixtures/theme_studio/canvas-project.json'), 'utf8')), 'generated canvas document matches the fixture validated by Rust');
  assert.deepEqual(pointerToScene(60, 120, { left: 10, top: 20, width: 100, height: 200 }, scene), { x: 540, y: 960 });
  assert.equal(pointerToScene(0, 0, { left: 0, top: 0, width: 0, height: 200 }, scene), null);
  const dimension = { value: 50, unit: '%', futureAnchor: 'preserve me' };
  assert.equal(positionFromPixels(dimension, 540, 1080).futureAnchor, 'preserve me');
  assert.equal(pixels(dimension, 1080), 540);
  assert.deepEqual(moveNode(source, source.scenes[0].id, 'missing', 0, 0), source);
  const nested = JSON.parse(JSON.stringify(added.project));
  nested.scenes[0].nodes[0].children = [added.id];
  assert.equal(rootNodes(nested.scenes[0]).some(node => node.id === added.id), false);
  assert.equal(updateNode(nested, scene.id, added.id, node => ({ ...node, name: 'not allowed' })), nested, 'nested layers are retained read-only');
  assert.equal(previewColor('url(https://untrusted/image.svg)', '#ffffff'), '#ffffff');
  assert.equal(previewColor('00ff0080', '#ffffff'), '#00ff0080');

  const layered = addNode(added.project, scene.id, 'panel').project;
  const originalOrder = layered.scenes[0].nodes.map(node => node.id);
  const originalLayeredJson = JSON.stringify(layered);
  assert.equal(canOrderLayers(layered.scenes[0]), true);
  const expectedOrders = {
    back: [originalOrder[1], originalOrder[0], originalOrder[2]],
    backward: [originalOrder[1], originalOrder[0], originalOrder[2]],
    forward: [originalOrder[0], originalOrder[2], originalOrder[1]],
    front: [originalOrder[0], originalOrder[2], originalOrder[1]],
  };
  for (const [action, expected] of Object.entries(expectedOrders)) {
    const ordered = orderLayer(layered, scene.id, added.id, action);
    assert.deepEqual(ordered.scenes[0].nodes.map(node => node.id), expected);
    assert.deepEqual(ordered.scenes[0].nodes.find(node => node.id === added.id), layered.scenes[0].nodes[1], 'all node fields stay unchanged');
    assert.equal(ordered.scenes[1], layered.scenes[1], 'other scenes are untouched');
    assert.deepEqual(ordered.futureAnimation, layered.futureAnimation);
  }
  assert.equal(JSON.stringify(layered), originalLayeredJson);
  const fourLayers = addNode(layered, scene.id, 'panel').project;
  const fourIds = fourLayers.scenes[0].nodes.map(node => node.id);
  assert.deepEqual(orderLayer(fourLayers, scene.id, fourIds[3], 'back').scenes[0].nodes.map(node => node.id), [fourIds[3], ...fourIds.slice(0, 3)]);
  assert.deepEqual(orderLayer(fourLayers, scene.id, fourIds[0], 'front').scenes[0].nodes.map(node => node.id), [...fourIds.slice(1), fourIds[0]]);
  assert.deepEqual(orderLayer(fourLayers, scene.id, fourIds[3], 'backward').scenes[0].nodes.map(node => node.id), [fourIds[0], fourIds[1], fourIds[3], fourIds[2]]);
  assert.equal(orderLayer(layered, scene.id, originalOrder[0], 'backward'), layered);
  assert.equal(orderLayer(layered, scene.id, originalOrder[2], 'forward'), layered);
  assert.equal(orderLayer(layered, 'missing', added.id, 'front'), layered);
  assert.equal(orderLayer(layered, scene.id, 'missing', 'front'), layered);
  assert.equal(orderLayer(nested, scene.id, added.id, 'back'), nested);
  const unknownLayered = JSON.parse(originalLayeredJson);
  unknownLayered.scenes[0].nodes[0].type = 'future-widget';
  assert.equal(orderLayer(unknownLayered, scene.id, added.id, 'back'), unknownLayered);
  let orderStored = { ...record(), savedJson: originalLayeredJson };
  const orderSession = new ProjectSession(orderStored, layered, async (id, revision, json, kind) => {
    assert.equal(revision, orderStored.revision);
    orderStored = { ...orderStored, revision: revision + 1, savedJson: kind === 'checkpoint' ? json : orderStored.savedJson, draftJson: kind === 'checkpoint' ? null : json };
    return orderStored;
  });
  const frontLayered = orderLayer(layered, scene.id, added.id, 'front');
  orderSession.edit(frontLayered, 'Change layer order');
  await orderSession.flush();
  assert.deepEqual(JSON.parse(orderStored.draftJson), frontLayered);
  orderSession.undo(); await orderSession.flush();
  assert.deepEqual(orderSession.document, layered);
  orderSession.redo(); await orderSession.save();
  assert.deepEqual(JSON.parse(orderStored.savedJson), frontLayered);
  assert.equal(orderStored.draftJson, null);

  const history = new EditHistory();
  assert.ok(history.record(source, added.project, 'Add text'));
  assert.ok(history.record(added.project, moved, 'Move text', 'drag-1'));
  const movedAgain = moveNode(moved, scene.id, added.id, 700, 800);
  history.record(moved, movedAgain, 'Move text', 'drag-1');
  const undidDrag = history.undo(movedAgain);
  assert.deepEqual(undidDrag, added.project, 'a grouped operation undoes in one step');
  assert.deepEqual(history.undo(undidDrag), source);
  const redidAdd = history.redo(source); assert.deepEqual(redidAdd, added.project);
  history.record(redidAdd, second.project, 'Branch edit');
  assert.equal(history.canRedo, false, 'a new edit invalidates the redo branch');
  const bounded = new EditHistory(1);
  bounded.record(source, added.project, 'Add'); bounded.record(added.project, moved, 'Move');
  const previous = bounded.undo(moved); assert.deepEqual(previous, added.project);
  assert.equal(bounded.undo(previous), null);

  // Undo during a queued write must persist the reverted source as the next revision.
  let historyStored = record(); let unblockHistory;
  const held = new Promise(resolve => { unblockHistory = resolve; });
  let historyWrites = 0;
  const historySession = new ProjectSession(historyStored, project, async (id, revision, json, kind) => {
    if (++historyWrites === 1) await held;
    assert.equal(revision, historyStored.revision);
    historyStored = { ...historyStored, revision: revision + 1, savedJson: kind === 'checkpoint' ? json : historyStored.savedJson, draftJson: kind === 'checkpoint' ? null : json };
    return historyStored;
  });
  const historyAdd = addNode(project, project.scenes[0].id, 'panel');
  historySession.edit(historyAdd.project, 'Add panel');
  const historyFlush = historySession.flush(); await Promise.resolve(); await Promise.resolve();
  historySession.undo(); unblockHistory(); await historyFlush;
  assert.deepEqual(JSON.parse(historyStored.draftJson), project);
  historySession.redo(); await historySession.save();
  historySession.undo(); await historySession.flush();
  assert.deepEqual(JSON.parse(historyStored.savedJson), historyAdd.project, 'undo changes the draft, not the explicit checkpoint');
  assert.deepEqual(JSON.parse(historyStored.draftJson), project);

  let stored = record();
  const calls = [];
  let release;
  const firstWrite = new Promise(resolve => { release = resolve; });
  const writer = async (id, revision, json, kind) => {
    calls.push({ revision, json, kind });
    if (calls.length === 1) await firstWrite;
    assert.equal(revision, stored.revision);
    stored = { ...stored, revision: revision + 1, name: JSON.parse(json).metadata.name,
      savedJson: kind === 'checkpoint' ? json : stored.savedJson,
      draftJson: kind === 'checkpoint' ? null : json };
    return stored;
  };
  const session = new ProjectSession(stored, project, writer);
  const edit = (s, name) => s.edit({ ...s.document, metadata: { ...s.document.metadata, name } });
  edit(session, 'First edit');
  const leaving = session.flush();
  await Promise.resolve(); await Promise.resolve();
  assert.equal(session.busy, true);
  edit(session, 'Edited during write');
  release(); await leaving;
  assert.equal(session.document.metadata.name, 'Edited during write');
  assert.equal(JSON.parse(stored.draftJson).metadata.name, 'Edited during write');
  assert.equal(session.dirty, false);
  assert.deepEqual(calls.map(c => c.revision), [0, 1]);
  await session.save();
  assert.equal(stored.draftJson, null);
  assert.equal(JSON.parse(stored.savedJson).metadata.name, 'Edited during write');

  let fail = true;
  const failing = new ProjectSession(record(), project, async (id, revision, json) => {
    if (fail) throw { code: 'storage', message: 'disk unavailable' };
    return { ...record(), revision: revision + 1, draftJson: json };
  });
  edit(failing, 'Keep me');
  await assert.rejects(failing.flush());
  assert.equal(failing.dirty, true);
  assert.equal(failing.document.metadata.name, 'Keep me');
  await assert.rejects(failing.flush(), 'navigation must remain blocked until an explicit retry');
  fail = false; await failing.retry();
  assert.equal(failing.dirty, false);
  const conflict = new ProjectSession(record(), project, async () => { throw { code: 'conflict', message: 'newer source exists' }; });
  edit(conflict, 'Local copy'); await assert.rejects(conflict.flush());
  await assert.rejects(conflict.retry());
  assert.equal(conflict.document.metadata.name, 'Local copy');

  // Exercise the installed SDK's real close-event wrapper with a permission-aware IPC stub.
  // No actual window is closed and no running application/catalog is accessed.
  const previousWindow = global.window;
  const { Window } = require('@tauri-apps/api/window');
  const permissions = JSON.parse(fs.readFileSync(path.join(root, 'src-tauri/capabilities/default.json'), 'utf8'));
  const nativeCalls = [];
  let simulateMissingPermission = false;
  global.window = { __TAURI_INTERNALS__: { invoke: async (command, args) => {
    assert.equal(command, 'plugin:window|destroy', 'the handler must not recursively request close');
    assert.equal(args.label, 'main');
    assert.ok(permissions.windows.includes('main') && permissions.permissions.includes('core:window:allow-destroy'), 'SDK close completion requires explicit destroy permission');
    if (simulateMissingPermission) throw new Error('window.destroy not allowed');
    nativeCalls.push(command);
  } } };
  try {
    const appWindow = Object.create(Window.prototype); appWindow.label = 'main';
    let dispatch;
    appWindow.listen = async (event, callback) => { dispatch = () => callback({ event, id: 1, payload: null }); return () => {}; };
    let releaseClose;
    const pendingClose = new Promise(resolve => { releaseClose = resolve; });
    const errors = [];
    await appWindow.onCloseRequested(guardedWindowClose(() => pendingClose, error => errors.push(error)));
    const closing = dispatch();
    await Promise.resolve();
    assert.equal(nativeCalls.length, 0, 'never close before save acknowledgement');
    await dispatch();
    assert.equal(nativeCalls.length, 0, 'duplicate close while saving must be prevented');
    releaseClose(true); await closing;
    assert.equal(nativeCalls.length, 1, 'one close after successful save');

    let permitted = false;
    await appWindow.onCloseRequested(guardedWindowClose(async () => permitted, error => errors.push(error)));
    await dispatch(); assert.equal(nativeCalls.length, 1, 'failed/busy flush keeps the window open');
    permitted = true; await dispatch();
    assert.equal(nativeCalls.length, 2, 'closing works after retry');
    await appWindow.onCloseRequested(guardedWindowClose(async () => { throw new Error('save failed'); }, error => errors.push(error)));
    await dispatch(); assert.equal(nativeCalls.length, 2);
    assert.equal(errors.length, 1);
    assert.match(errors[0].message, /save failed/);
    // Demonstrate the original missing-permission failure without editing live configuration.
    simulateMissingPermission = true;
    await appWindow.onCloseRequested(guardedWindowClose(async () => true, error => errors.push(error)));
    await assert.rejects(dispatch(), /window.destroy not allowed/);
    assert.equal(nativeCalls.length, 2);
  } finally { if (previousWindow === undefined) delete global.window; else global.window = previousWindow; }

  // Render actual React components without a browser/native catalog. No theme content executes.
  const originalLoad = Module._load;
  Module._load = function(request, parent, isMain) {
    if (/\.(png|css)$/.test(request)) return 'test-asset';
    if (request.startsWith('@tauri-apps/')) return { isTauri: () => false, invoke: async () => [], listen: async () => () => {}, open: async () => null };
    return originalLoad.call(this, request, parent, isMain);
  };
  try {
    const React = require('react'); const { renderToStaticMarkup } = require('react-dom/server');
    const component = file => require(path.join(output, `src/${file}.js`)).default;
    const render = (file, props) => renderToStaticMarkup(React.createElement(component(file), props));
    const wizard = render('theme-studio/NewProjectWizard', { busy: false, onCreate() {}, onCancel() {} });
    assert.match(wizard, /Universal \/ No Target Yet/);
    assert.match(wizard, /No emulator or connected installation is needed/);
    const editor = render('theme-studio/ProjectOverview', { session, onClose() {}, onCopy() {}, onReload() {} });
    for (const region of ['Scenes and layers', 'Scene canvas', 'Properties inspector', 'Animation timeline']) assert.ok(editor.includes(region));
    assert.match(editor, /Autosaved|Saved checkpoint/);
    assert.match(editor, /Native support is unverified/);
    assert.match(editor, /Add text/); assert.match(editor, /Add panel/); assert.match(editor, />Undo</);
    const safeProject = addNode(project, project.scenes[0].id, 'text');
    const safeNode = safeProject.project.scenes[0].nodes.find(node => node.id === safeProject.id);
    safeNode.style.text = '<script>alert(1)</script>'; safeNode.style.color = 'url(https://untrusted)';
    const canvas = render('theme-studio/SceneCanvas', { scene: safeProject.project.scenes[0], selected: safeNode.id, disabled: false, onSelect() {}, onMove() {}, onEndGroup() {} });
    assert.match(canvas, /&lt;script&gt;/); assert.ok(!canvas.includes('<script>'));
    assert.ok(!canvas.includes('https://untrusted'));
    const orderedScene = orderLayer(layered, scene.id, added.id, 'front').scenes[0];
    const orderedCanvas = render('theme-studio/SceneCanvas', { scene: orderedScene, selected: added.id, disabled: false, onSelect() {}, onMove() {}, onEndGroup() {} });
    const paintedIds = [...orderedCanvas.matchAll(/data-node-id="([^"]+)"/g)].map(match => match[1]);
    assert.deepEqual(paintedIds, expectedOrders.front, 'actual SVG paint order follows source ordering');
    let orderRequest = null;
    const orderProps = { scene: layered.scenes[0], selected: added.id, disabled: false, onOrder(order) { orderRequest = order; } };
    const orderControls = component('theme-studio/LayerOrderControls')(orderProps);
    const orderButtons = orderControls.props.children.find(Array.isArray);
    orderButtons.find(button => button.props.children === 'Bring to front').props.onClick();
    assert.equal(orderRequest, 'front');
    for (const overrides of [{ disabled: true }, { selected: null }, { scene: unknownLayered.scenes[0] }, { selected: originalOrder[2] }]) {
      orderRequest = null;
      const blockedControls = component('theme-studio/LayerOrderControls')({ ...orderProps, ...overrides });
      blockedControls.props.children.find(Array.isArray).find(button => button.props.children === 'Bring to front').props.onClick();
      assert.equal(orderRequest, null, 'disabled, unsupported, absent and boundary actions do not edit');
    }
    const inspector = render('theme-studio/NodeInspector', { scene: safeProject.project.scenes[0], node: safeNode, disabled: false, onChange() {}, onEnd() {} });
    assert.match(inspector, /X position/); assert.match(inspector, /Font size/);
    const longText = { ...safeNode, style: { ...safeNode.style, text: 'A'.repeat(501) } };
    assert.match(render('theme-studio/NodeInspector', { scene: safeProject.project.scenes[0], node: longText, disabled: false, onChange() {}, onEnd() {} }), /preserved read-only/);
    const unknownText = { ...safeNode, style: { ...safeNode.style, text: { futureExpression: 'kept' } } };
    assert.match(render('theme-studio/NodeInspector', { scene: safeProject.project.scenes[0], node: unknownText, disabled: false, onChange() {}, onEnd() {} }), /preserved read-only/);
    let renamedLayer;
    const originalLayer = project.scenes[0].nodes[0];
    const layerForm = component('theme-studio/NodeInspector')({ scene: project.scenes[0], node: originalLayer, disabled: false, onChange(node) { renamedLayer = node; }, onEnd() {} });
    const propertyFields = layerForm.props.children.find(child => child?.type === 'fieldset');
    const nameLabel = propertyFields.props.children.find(child => child?.type === 'label');
    nameLabel.props.children.find(child => child?.type === 'input').props.onChange({ target: { value: 'Renamed layer' } });
    assert.equal(renamedLayer.name, 'Renamed layer');
    assert.equal(renamedLayer.style.text, originalLayer.name, 'renaming a legacy layer keeps its displayed caption');
    const browser = render('theme-studio/ProjectBrowser', { projects: [], busy: false, offset: 0, onNew() {}, onOpen() {}, onDuplicate() {}, onPage() {}, onRefresh() {} });
    assert.match(browser, /No projects yet/);
    let submitted = null; let cancelled = false;
    const copyProps = { name: ' New copy ', sourceName: 'Original', busy: false, onName() {}, onCreate(name) { submitted = name; }, onCancel() { cancelled = true; } };
    const copyForm = component('theme-studio/SaveCopyForm')(copyProps);
    const copyHtml = render('theme-studio/SaveCopyForm', copyProps);
    assert.match(copyHtml, /Save a separate copy/);
    assert.match(copyHtml, /does not rename the original project/);
    assert.match(copyHtml, /New copy name/);
    copyForm.props.onSubmit({ preventDefault() {} });
    assert.equal(submitted, 'New copy', 'confirmation uses the new name, not the source name');
    submitted = null;
    component('theme-studio/SaveCopyForm')({ ...copyProps, busy: true }).props.onSubmit({ preventDefault() {} });
    assert.equal(submitted, null);
    component('theme-studio/SaveCopyForm')({ ...copyProps, name: ' ' }).props.onSubmit({ preventDefault() {} });
    assert.equal(submitted, null);
    const buttons = copyForm.props.children.find(child => child?.type === 'div').props.children;
    buttons.find(button => button.props.type === 'button').props.onClick();
    assert.equal(cancelled, true);
    assert.equal(submitted, null, 'cancel must not create another project');
    const customize = render('Customize', { installs: [], native: true, busy: false, notify() {} });
    assert.match(customize, /Add a RetroBat or Batocera installation/);
    const shell = render('App', {});
    assert.ok(shell.indexOf('>Customize</button>') < shell.indexOf('>Theme Studio</button>'));
    assert.ok(shell.indexOf('>Theme Studio</button>') < shell.indexOf('>Settings</button>'));
    const fallback = render('ThemeStudio', { native: false, leaveGuard: { current: null } });
    assert.match(fallback, /No emulator installation or cloud account is required/);
  } finally { Module._load = originalLoad; }
  console.log('Theme workspace: creation, saves/close/copy regressions, scene editing, bounded history/in-flight undo and safe React/SVG rendering passed.');
}
main().catch(error => { console.error(error); process.exitCode = 1; }).finally(() => {
  assert.equal(path.dirname(output), cache); assert.ok(path.basename(output).startsWith('theme-workspace-'));
  fs.rmSync(output, { recursive: true, force: true });
});
