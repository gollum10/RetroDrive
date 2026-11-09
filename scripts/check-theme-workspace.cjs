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
  const files = ['tests/theme-studio/workspace.ts', 'tests/theme-studio/browser.tsx', 'src/App.tsx', 'src/ThemeStudio.tsx'];
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
  const { ProjectSession, project, form, record, newDocument, suggestCopyName, guardedWindowClose, EditHistory, addNode, moveNode, updateNode, rootNodes, pixels, positionFromPixels, pointerToScene, previewColor, canOrderLayers, orderLayer, transformNode, alignedNode, alignLayer, resizeDelta, resizeLayer, bounds, snapSize, snapPosition, guides, layerState, setLayerState, editableNode } = require(path.join(output, 'tests/theme-studio/workspace.js'));
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

  const transformSource = { ...movedNode, layout: { ...movedNode.layout, futurePivot: 'preserved' }, style: { ...movedNode.style, futureBlend: { mode: 'preserved' } } };
  const transformJson = JSON.stringify(transformSource);
  const rotated = transformNode(transformSource, 'rotation', -45.5);
  const faded = transformNode(rotated, 'opacity', 0.25);
  assert.equal(faded.layout.rotation, -45.5);
  assert.equal(faded.style.opacity, 0.25);
  assert.equal(faded.layout.futurePivot, 'preserved');
  assert.deepEqual(faded.style.futureBlend, { mode: 'preserved' });
  assert.equal(JSON.stringify(transformSource), transformJson);
  for (const invalid of [NaN, Infinity, -Infinity, 36001, -36001]) assert.equal(transformNode(transformSource, 'rotation', invalid), transformSource);
  for (const invalid of [NaN, Infinity, -0.1, 1.1]) assert.equal(transformNode(transformSource, 'opacity', invalid), transformSource);
  for (const value of [0, 1]) assert.equal(transformNode(faded, 'opacity', value).style.opacity, value);
  assert.equal(transformNode(transformSource, 'opacity', 1), transformSource, 'default opacity does not create an edit');
  const opaqueOpacity = { ...transformSource, style: { ...transformSource.style, opacity: { token: 'future' } } };
  assert.equal(transformNode(opaqueOpacity, 'opacity', 0.5), opaqueOpacity);
  const transformProject = updateNode(moved, scene.id, added.id, () => faded);
  const transformHistory = new EditHistory();
  transformHistory.record(moved, transformProject, 'Transform');
  assert.deepEqual(transformHistory.undo(transformProject), moved);
  assert.deepEqual(transformHistory.redo(moved), transformProject);

  const alignmentNode = { ...movedNode, layout: { ...movedNode.layout, x: { value: 17, unit: '%', futureAnchor: 'kept' }, y: { value: 31, unit: 'px' }, width: { value: 200, unit: 'px' }, height: { value: 100, unit: 'px' }, rotation: 0 } };
  const alignmentScene = { ...scene, nodes: [alignmentNode] };
  const expectedAlignment = { left: 0, 'center-x': 440, right: 880, top: 0, 'center-y': 910, bottom: 1820 };
  for (const [action, expected] of Object.entries(expectedAlignment)) {
    const aligned = alignedNode(alignmentScene, alignmentNode, action);
    const axis = ['left', 'center-x', 'right'].includes(action) ? 'x' : 'y';
    const extent = axis === 'x' ? 1080 : 1920;
    assert.ok(Math.abs(pixels(aligned.layout[axis], extent) - expected) < 0.00002, action);
    assert.equal(aligned.layout.x.unit, '%'); assert.equal(aligned.layout.x.futureAnchor, 'kept');
    assert.deepEqual(aligned.layout[axis === 'x' ? 'y' : 'x'], alignmentNode.layout[axis === 'x' ? 'y' : 'x']);
    assert.deepEqual(aligned.style, alignmentNode.style);
    assert.equal(alignedNode(alignmentScene, aligned, action), aligned, 'repeated alignment is a no-op');
  }
  for (const rotation of [90, -90, 45, 180, 360]) {
    const rotatedNode = { ...alignmentNode, layout: { ...alignmentNode.layout, rotation } };
    const rotatedScene = { ...alignmentScene, nodes: [rotatedNode] };
    for (const action of Object.keys(expectedAlignment)) {
      const aligned = alignedNode(rotatedScene, rotatedNode, action);
      const x = pixels(aligned.layout.x, 1080); const y = pixels(aligned.layout.y, 1920);
      const radians = rotation * Math.PI / 180;
      const corners = [[0,0],[200,0],[0,100],[200,100]].map(([cx, cy]) => [x + cx * Math.cos(radians) - cy * Math.sin(radians), y + cx * Math.sin(radians) + cy * Math.cos(radians)]);
      const axis = ['left', 'center-x', 'right'].includes(action) ? 0 : 1;
      const low = Math.min(...corners.map(point => point[axis])); const high = Math.max(...corners.map(point => point[axis]));
      const actual = action.startsWith('center') ? (low + high) / 2 : ['left', 'top'].includes(action) ? low : high;
      const expected = action.startsWith('center') ? (axis === 0 ? 540 : 960) : ['left', 'top'].includes(action) ? 0 : axis === 0 ? 1080 : 1920;
      assert.ok(Math.abs(actual - expected) < 0.00002, `${rotation} ${action}`);
    }
  }
  assert.equal(alignLayer(nested, scene.id, added.id, 'left'), nested);
  assert.equal(alignLayer(source, 'missing', added.id, 'left'), source);
  assert.equal(alignLayer(source, scene.id, 'missing', 'left'), source);
  const unknownAlign = { ...alignmentNode, type: 'future-widget' };
  assert.equal(alignedNode({ ...alignmentScene, nodes: [unknownAlign] }, unknownAlign, 'left'), unknownAlign);
  const hugeNode = { ...alignmentNode, layout: { ...alignmentNode.layout, x: { value: 0, unit: 'px' }, width: { value: 1_000_000, unit: '%' } } };
  assert.equal(alignedNode({ ...alignmentScene, nodes: [hugeNode] }, hugeNode, 'right'), hugeNode, 'unrepresentable target is refused');
  const alignSource = { ...moved, scenes: [alignmentScene, ...moved.scenes.slice(1)] };
  const alignJson = JSON.stringify(alignSource);
  const centered = alignLayer(alignSource, scene.id, added.id, 'center-x');
  assert.equal(JSON.stringify(alignSource), alignJson);
  assert.equal(centered.scenes[1], alignSource.scenes[1]);
  assert.deepEqual(centered.futureAnimation, alignSource.futureAnimation);
  const alignHistory = new EditHistory();
  alignHistory.record(alignSource, centered, 'Align');
  assert.deepEqual(alignHistory.undo(centered), alignSource);
  assert.deepEqual(alignHistory.redo(alignSource), centered);

  assert.deepEqual(resizeDelta(200, 100, 0, 20, 30), { width: 220, height: 130 });
  assert.deepEqual(resizeDelta(0.1, 100, 0, 0, 10), { width: 0.1, height: 110 }, 'unchanged tiny axis keeps its original size');
  const rotatedResize = resizeDelta(200, 100, 90, -30, 20);
  assert.ok(Math.abs(rotatedResize.width - 220) < 1e-8);
  assert.ok(Math.abs(rotatedResize.height - 130) < 1e-8);
  assert.deepEqual(resizeDelta(200, 100, 0, -1000, -1000), { width: 1, height: 1 });
  const resizeJson = JSON.stringify(alignSource);
  const resized = resizeLayer(alignSource, scene.id, added.id, 400, 250);
  const resizedNode = resized.scenes[0].nodes[0];
  assert.equal(resizedNode.layout.width.value, 400); assert.equal(resizedNode.layout.height.value, 250);
  assert.deepEqual(resizedNode.layout.x, alignmentNode.layout.x);
  assert.equal(JSON.stringify(alignSource), resizeJson);
  assert.equal(resized.scenes[1], alignSource.scenes[1]);
  assert.deepEqual(resized.futureAnimation, alignSource.futureAnimation);
  assert.deepEqual(resizedNode.style, alignmentNode.style);
  const percentNode = { ...alignmentNode, layout: { ...alignmentNode.layout, width: { value: 20, unit: '%', futureScale: 'kept' }, height: { value: 10, unit: '%' } } };
  const percentProject = { ...alignSource, scenes: [{ ...alignmentScene, nodes: [percentNode] }, ...alignSource.scenes.slice(1)] };
  const percentResized = resizeLayer(percentProject, scene.id, added.id, 540, 960).scenes[0].nodes[0];
  assert.equal(percentResized.layout.width.value, 50); assert.equal(percentResized.layout.height.value, 50);
  assert.equal(percentResized.layout.width.futureScale, 'kept');
  assert.equal(resizeLayer(alignSource, scene.id, added.id, 200, 100), alignSource, 'unchanged size creates no history');
  for (const [w, h] of [[0, 10], [-1, 10], [10, NaN], [Infinity, 10]]) assert.equal(resizeLayer(alignSource, scene.id, added.id, w, h), alignSource);
  assert.equal(resizeLayer(nested, scene.id, added.id, 300, 300), nested);
  assert.equal(resizeLayer(alignSource, scene.id, 'missing', 300, 300), alignSource);
  assert.equal(resizeLayer(alignSource, 'missing', added.id, 300, 300), alignSource);
  assert.equal(resizeLayer(alignSource, scene.id, added.id, 1e20, 1e20).scenes[0].nodes[0].layout.width.value, 1_000_000);
  const resizeHistory = new EditHistory(); resizeHistory.record(alignSource, resized, 'Resize');
  assert.deepEqual(resizeHistory.undo(resized), alignSource); assert.deepEqual(resizeHistory.redo(alignSource), resized);

  assert.deepEqual(snapSize(30, 49, 16), { width: 32, height: 48 });
  assert.deepEqual(snapSize(1, 1, 16), { width: 1, height: 1 });
  const gridBox = { x: 19, y: -9, width: 100, height: 50, rotation: 0 };
  assert.deepEqual(snapPosition(gridBox, 1920, 1080, 16, 0), { x: 16, y: -16 });
  assert.equal(gridBox.x, 19);
  assert.deepEqual(snapPosition({ ...gridBox, x: 905, y: 20 }, 1920, 1080, 16, 12), { x: 910, y: 16 });
  const rotatedBounds = bounds({ ...gridBox, x: 0, y: 0, rotation: 90 });
  assert.ok(Math.abs(rotatedBounds.left + 50) < 1e-8);
  assert.ok(Math.abs(rotatedBounds.bottom - 100) < 1e-8);
  assert.deepEqual(guides({ ...gridBox, x: 910, y: 515 },1920,1080), { x: [960], y: [540] });

  const lockedProject = setLayerState(added.project,scene.id,added.id,'locked',true);
  const lockedNode = lockedProject.scenes[0].nodes.find(node => node.id === added.id);
  assert.equal(layerState(lockedNode).locked,true);
  assert.equal(editableNode(lockedProject.scenes[0],lockedNode),false);
  assert.equal(setLayerState(lockedProject,scene.id,added.id,'locked',true),lockedProject);
  assert.deepEqual(lockedNode.layout, added.project.scenes[0].nodes.find(node => node.id === added.id).layout);
  assert.equal(moveNode(lockedProject,scene.id,added.id,20,20),lockedProject, 'move cannot edit a locked layer');
  assert.equal(resizeLayer(lockedProject,scene.id,added.id,200,200),lockedProject);
  assert.equal(alignLayer(lockedProject,scene.id,added.id,'left'),lockedProject);
  assert.equal(transformNode(lockedNode,'rotation',45),lockedNode);
  assert.equal(orderLayer(lockedProject,scene.id,added.id,'back'),lockedProject);
  assert.equal(orderLayer(lockedProject,scene.id,lockedProject.scenes[0].nodes[0].id,'front'),lockedProject,'cannot reorder across a protected layer');
  const hiddenProject = setLayerState(lockedProject,scene.id,added.id,'hidden',true);
  assert.equal(layerState(hiddenProject.scenes[0].nodes.find(node => node.id === added.id)).hidden,true);
  const unlockedProject = setLayerState(hiddenProject,scene.id,added.id,'locked',false);
  assert.equal(moveNode(unlockedProject,scene.id,added.id,20,20),unlockedProject,'hidden layer stays read-only');
  const shownProject = setLayerState(unlockedProject,scene.id,added.id,'hidden',false);
  assert.notEqual(moveNode(shownProject,scene.id,added.id,20,20),shownProject);
  const editorExtension = JSON.parse(JSON.stringify(added.project));
  editorExtension.scenes[0].nodes.find(node => node.id === added.id).editor = { futurePanel: { preserve: true } };
  const extensionLocked = setLayerState(editorExtension,scene.id,added.id,'locked',true);
  assert.deepEqual(extensionLocked.scenes[0].nodes.find(node => node.id === added.id).editor.futurePanel,{ preserve: true });
  for (const editor of [null, [], 'future', { locked: 'expression' }, { hidden: { future: true } }]) {
    const opaqueEditor = JSON.parse(JSON.stringify(added.project));
    opaqueEditor.scenes[0].nodes.find(node => node.id === added.id).editor = editor;
    assert.equal(setLayerState(opaqueEditor,scene.id,added.id,'locked',true),opaqueEditor);
    assert.equal(moveNode(opaqueEditor,scene.id,added.id,0,0),opaqueEditor);
  }
  assert.equal(setLayerState(nested,scene.id,added.id,'locked',true),nested);
  assert.equal(setLayerState(added.project,'missing',added.id,'hidden',true),added.project);
  assert.equal(added.project.scenes[0].nodes.find(node => node.id === added.id).editor,undefined);
  let flagStored = { ...record(), savedJson: JSON.stringify(added.project) };
  const flagSession = new ProjectSession(flagStored,added.project,async (id,revision,json,kind) => {
    assert.equal(revision,flagStored.revision);
    flagStored = { ...flagStored,revision:revision+1,savedJson:kind === 'checkpoint' ? json : flagStored.savedJson,draftJson:kind === 'checkpoint' ? null : json };
    return flagStored;
  });
  flagSession.edit(lockedProject,'Lock layer'); await flagSession.flush();
  assert.deepEqual(JSON.parse(flagStored.draftJson),lockedProject);
  flagSession.undo(); await flagSession.flush(); assert.deepEqual(flagSession.document,added.project);
  flagSession.redo(); await flagSession.save();
  assert.deepEqual(JSON.parse(flagStored.savedJson),lockedProject);
  assert.deepEqual(new ProjectSession(flagStored,JSON.parse(flagStored.savedJson)).document,lockedProject);

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
    const canvas = render('theme-studio/SceneCanvas', { scene: safeProject.project.scenes[0], selected: safeNode.id, disabled: false, onSelect() {}, onMove() {}, onResize() {}, onEndGroup() {} });
    assert.match(canvas, /&lt;script&gt;/); assert.ok(!canvas.includes('<script>'));
    assert.ok(!canvas.includes('https://untrusted'));
    assert.match(canvas, /aria-label="Resize Text"/);
    const disabledCanvas = render('theme-studio/SceneCanvas', { scene: safeProject.project.scenes[0], selected: safeNode.id, disabled: true, onSelect() {}, onMove() {}, onResize() {}, onEndGroup() {} });
    assert.ok(!disabledCanvas.includes('aria-label="Resize Text"'), 'disabled editor exposes no resize handle');
    const noSelectionCanvas = render('theme-studio/SceneCanvas', { scene: safeProject.project.scenes[0], selected: null, disabled: false, onSelect() {}, onMove() {}, onResize() {}, onEndGroup() {} });
    assert.ok(!noSelectionCanvas.includes('aria-label="Resize Text"'));

    const orderedScene = orderLayer(layered, scene.id, added.id, 'front').scenes[0];
    const orderedCanvas = render('theme-studio/SceneCanvas', { scene: orderedScene, selected: added.id, disabled: false, onSelect() {}, onMove() {}, onResize() {}, onEndGroup() {} });
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
    const transformProps = { scene: moved.scenes[0], node: transformSource, disabled: false, onChange(node, label, group) { renamedLayer = { node, label, group }; }, onEnd() {} };
    const transformForm = component('theme-studio/NodeInspector')(transformProps);
    const transformFields = transformForm.props.children.find(child => child?.type === 'fieldset').props.children;
    const rotationField = transformFields.find(child => child?.props?.label === 'Rotation (degrees)');
    rotationField.props.onChange(30);
    assert.equal(renamedLayer.node.layout.rotation, 30);
    assert.equal(renamedLayer.group, `transform:${transformSource.id}:rotation`);
    const opacityField = transformFields.find(child => child?.props?.label === 'Opacity (0 to 1)');
    opacityField.props.onChange(0.5);
    assert.equal(renamedLayer.node.style.opacity, 0.5);
    renamedLayer = null;
    const disabledTransform = component('theme-studio/NodeInspector')({ ...transformProps, disabled: true });
    disabledTransform.props.children.find(child => child?.type === 'fieldset').props.children.find(child => child?.props?.label === 'Rotation (degrees)').props.onChange(60);
    assert.equal(renamedLayer, null);
    assert.match(render('theme-studio/NodeInspector', { ...transformProps, node: opaqueOpacity }), /Unsupported opacity data is preserved read-only/);
    const transformedCanvas = render('theme-studio/SceneCanvas', { scene: transformProject.scenes[0], selected: added.id, disabled: false, onSelect() {}, onMove() {}, onResize() {}, onEndGroup() {} });
    assert.match(transformedCanvas, /rotate\(-45\.5\)/);
    assert.match(transformedCanvas, /opacity="0.25"/);
    let alignRequest = null;
    const alignProps = { scene: alignmentScene, node: alignmentNode, disabled: false, onAlign(action) { alignRequest = action; } };
    const alignmentControls = component('theme-studio/AlignmentControls')(alignProps);
    alignmentControls.props.children.find(Array.isArray).find(button => button.props.children === 'Align left').props.onClick();
    assert.equal(alignRequest, 'left');
    for (const overrides of [{ disabled: true }, { node: null }, { node: unknownAlign }, { node: alignedNode(alignmentScene, alignmentNode, 'left') }]) {
      alignRequest = null;
      component('theme-studio/AlignmentControls')({ ...alignProps, ...overrides }).props.children.find(Array.isArray).find(button => button.props.children === 'Align left').props.onClick();
      assert.equal(alignRequest, null);
    }
    assert.match(editor, /Align to canvas/);
    const lockedCanvasProps = { scene: lockedProject.scenes[0], selected: added.id, disabled: false, onSelect() {}, onMove() {}, onResize() {}, onEndGroup() {} };
    assert.ok(!render('theme-studio/SceneCanvas',lockedCanvasProps).includes('aria-label="Resize Text"'));
    const hiddenCanvas = render('theme-studio/SceneCanvas',{ ...lockedCanvasProps,scene:hiddenProject.scenes[0] });
    assert.ok(!hiddenCanvas.includes(`data-node-id="${added.id}"`));
    let flagRequest = null;
    const listProps = { scene:hiddenProject.scenes[0],selected:added.id,disabled:false,onSelect() {},onState(...args) { flagRequest=args; } };
    const layerList = component('theme-studio/LayerList')(listProps);
    const rows = layerList.props.children.find(child => child?.type === 'div').props.children;
    const row = rows.find(row => row.key === added.id);
    const toggles = row.props.children.find(child => child?.props?.className === 'ts-layer-toggles').props.children;
    toggles.find(button => button.props.children === 'Show').props.onClick();
    assert.deepEqual(flagRequest,[added.id,'hidden',false]);
    assert.match(render('theme-studio/LayerList',listProps),/hidden/);
    flagRequest = null;
    const disabledList = component('theme-studio/LayerList')({ ...listProps,disabled:true });
    const disabledRow = disabledList.props.children.find(child => child?.type === 'div').props.children.find(row => row.key === added.id);
    disabledRow.props.children.find(child => child?.props?.className === 'ts-layer-toggles').props.children[0].props.onClick();
    assert.equal(flagRequest,null);
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
