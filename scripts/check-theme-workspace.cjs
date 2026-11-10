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
  const files = ['tests/theme-studio/workspace.ts', 'tests/theme-studio/browser.tsx', 'tests/theme-studio/preview-reopen.tsx', 'tests/theme-studio/image-controls.tsx', 'tests/theme-studio/sample-preview.tsx', 'src/App.tsx', 'src/ThemeStudio.tsx'];
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
  const { ProjectSession, project, form, record, newDocument, suggestCopyName, guardedWindowClose, EditHistory, addNode, moveNode, updateNode, rootNodes, pixels, positionFromPixels, pointerToScene, previewColor, canOrderLayers, orderLayer, transformNode, alignedNode, alignLayer, resizeDelta, resizeLayer, bounds, snapSize, snapPosition, guides, layerState, setLayerState, editableNode, attachImage, encodePngFile, safePngPreview, imagePresentation,setImageFit,setImageCrop,resetImageCrop,cropPixels, sampleText,setTextBinding,textBindingEditable,previewDatasets,textSources } = require(path.join(output, 'tests/theme-studio/workspace.js'));
  const { textAlignment,setTextAlignment,textPosition } = require(path.join(output, 'tests/theme-studio/workspace.js'));
  // Tool levels and the ready layout (ADR-108).
  const { LEVELS, shows } = require(path.join(output, 'tests/theme-studio/workspace.js'));
  assert.deepEqual(LEVELS.map(l => l.id), ['simple', 'standard', 'pro']);
  for (const tool of ['panel', 'order', 'numbers', 'layerName', 'details', 'units', 'transform', 'crop', 'check', 'timeline']) {
    assert.equal(shows('simple', tool), false, `Simple hides ${tool}`); assert.equal(shows('pro', tool), true, `Pro shows ${tool}`);
  }
  assert.ok(shows('standard', 'panel') && shows('standard', 'numbers') && !shows('standard', 'crop') && !shows('standard', 'transform'));
  const ready = newDocument({ ...form, template: 'ready' });
  const readyIds = ready.scenes.flatMap(s => s.nodes.map(n => n.id));
  assert.equal(new Set(readyIds).size, readyIds.length, 'ready layout ids are unique');
  assert.ok(ready.scenes.every(s => s.nodes[0].name === 'Background' && s.nodes[0].type === 'panel'), 'every screen starts with a background');
  const readyList = ready.scenes.find(s => s.role === 'game_list');
  assert.deepEqual(['game.title', 'game.description', 'game.year'].map(src => readyList.nodes.some(n => n.bindings.text?.source === src)), [true, true, true]);
  assert.ok(ready.scenes.flatMap(s => s.nodes).every(n => n.bindings.text === undefined || textSources.some(o => o.source === n.bindings.text.source && o.kind === n.bindings.text.kind)), 'ready layout uses known text links');
  const typographyNode = newDocument({ ...form, template: 'starter' }).scenes[0].nodes[0];
  assert.equal(textAlignment(typographyNode),'left');
  assert.equal(setTextAlignment(typographyNode,'left'),typographyNode);
  for (const value of ['center','right','left']) {
    const next=setTextAlignment({...typographyNode,style:{...typographyNode.style,textAlign:'right',opaque:{keep:true}}},value);
    assert.equal(textAlignment(next),value);
    assert.deepEqual(next.style.opaque,{keep:true});
    assert.equal(next.layout,typographyNode.layout);
    assert.equal(next.bindings,typographyNode.bindings);
    assert.deepEqual(textPosition(next,200),value==='center'?{x:100,anchor:'middle'}:value==='right'?{x:200,anchor:'end'}:{x:0,anchor:'start'});
  }
  for (const protectedNode of [ {...typographyNode,editor:{locked:true}}, {...typographyNode,editor:{hidden:true}}, {...typographyNode,style:{textAlign:{future:true}}}, {...typographyNode,type:'panel'}, {...typographyNode,children:[typographyNode]} ]) {
    assert.equal(setTextAlignment(protectedNode,'center'),protectedNode);
  }
  assert.equal(setTextAlignment(typographyNode,'justify'),typographyNode);
  assert.equal(JSON.stringify(typographyNode).includes('textAlign'),false);
  const typographyProject=newDocument({ ...form, template:'starter' });
  const typographyScene=typographyProject.scenes[0];
  const centeredProject=updateNode(typographyProject,typographyScene.id,typographyScene.nodes[0].id,node=>setTextAlignment(node,'center'));
  let typographyStored={...record(),savedJson:JSON.stringify(typographyProject)};
  const typographySession=new ProjectSession(typographyStored,typographyProject,async(id,revision,json,kind)=>{
    assert.equal(revision,typographyStored.revision);
    typographyStored={...typographyStored,revision:revision+1,savedJson:kind==='checkpoint'?json:typographyStored.savedJson,draftJson:kind==='checkpoint'?null:json};return typographyStored;
  });
  typographySession.edit(centeredProject,'Align text');await typographySession.flush();
  assert.deepEqual(JSON.parse(typographyStored.draftJson),centeredProject);
  typographySession.undo();assert.deepEqual(typographySession.document,typographyProject);
  typographySession.redo();await typographySession.save();
  assert.deepEqual(new ProjectSession(typographyStored,JSON.parse(typographyStored.savedJson)).document,centeredProject);
  typographySession.undo();await typographySession.flush();
  assert.deepEqual(JSON.parse(typographyStored.savedJson),centeredProject);
  assert.deepEqual(JSON.parse(typographyStored.draftJson),typographyProject);
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

  const pngInfo={sha256:'a'.repeat(64),path:`assets/images/${'a'.repeat(64)}.png`,bytes:70,width:100,height:50};
  const attached=attachImage(project,scene.id,pngInfo,'original','Creator');
  assert.equal(attached.project.assets.length,1);
  assert.equal(attached.project.assets[0].license,'original');
  assert.equal(attached.project.assets[0].source.includes(':\\'),false);
  const imageNode=attached.project.scenes[0].nodes.find(node=>node.id===attached.id);
  assert.equal(imageNode.type,'image');assert.deepEqual(imageNode.assets,[attached.project.assets[0].id]);
  assert.ok(editableNode(attached.project.scenes[0],imageNode));
  const repeatedImage=attachImage(attached.project,scene.id,pngInfo,'other','Other');
  assert.equal(repeatedImage.project.assets.length,1);assert.equal(repeatedImage.project.assets[0].license,'original');
  assert.notEqual(repeatedImage.id,attached.id);
  assert.throws(()=>attachImage(project,scene.id,{...pngInfo,path:'C:/private.png'},'original',''));
  assert.throws(()=>attachImage(project,scene.id,{...pngInfo,width:0},'original',''));
  // Game video layer (ADR-111).
  const { GAME_VIDEO, supportedNode } = require(path.join(output, 'tests/theme-studio/workspace.js'));
  const withVideo = addNode(project, scene.id, 'video');
  const videoNode = withVideo.project.scenes[0].nodes.find(n => n.id === withVideo.id);
  assert.equal(videoNode.type, 'video'); assert.equal(videoNode.name, 'Game video'); assert.deepEqual(videoNode.bindings.video, GAME_VIDEO);
  assert.ok(supportedNode(withVideo.project.scenes[0], videoNode) && editableNode(withVideo.project.scenes[0], videoNode));
  const videoFrame = attachImage(withVideo.project, scene.id, pngInfo, 'original', '', { placement: 'over', overId: videoNode.id });
  assert.deepEqual(videoFrame.project.scenes[0].nodes.find(n => n.id === videoFrame.id).layout, videoNode.layout, 'a frame fits over the game video');
  assert.ok(canOrderLayers(withVideo.project.scenes[0]), 'screens with a game video can still be reordered');
  // Game list layer (ADR-112).
  const withList = addNode(project, scene.id, 'game_list');
  const listNode = withList.project.scenes[0].nodes.find(n => n.id === withList.id);
  assert.equal(listNode.type, 'game_list'); assert.equal(listNode.name, 'Game list'); assert.deepEqual(listNode.bindings, {});
  assert.deepEqual([listNode.style.color, listNode.style.selectedColor, listNode.style.selectorColor, listNode.style.textAlign], ['c8d0e0', 'ffffff', '378cdd', 'left']);
  assert.ok(editableNode(withList.project.scenes[0], listNode) && canOrderLayers(withList.project.scenes[0]));
  const readyGames = newDocument({ ...form, template: "ready" }).scenes.find(s => s.role === "game_list");
  assert.equal(readyGames.nodes.filter(n => n.type === "game_list").length, 1, 'the Ready layout places a game list');
  assert.ok(readyGames.nodes.findIndex(n => n.type === "game_list") > readyGames.nodes.findIndex(n => n.name === 'Game list area'), 'the list sits on its backing panel');
  // Save for RetroBat planning and export document (ADR-113).
  const { planExport, exportDocument, liveLayer } = require(path.join(output, 'tests/theme-studio/workspace.js'));
  const readyTheme = newDocument({ ...form, template: 'ready' });
  const exportPlan = planExport(readyTheme);
  assert.deepEqual(exportPlan.screens.map(s => s.scene.role), ['system_select', 'game_list']);
  assert.ok(exportPlan.notes.some(n => /Startup/.test(n) && /aren't included/.test(n)), 'other screens are explained');
  assert.ok(exportPlan.notes.some(n => /System name/.test(n) && /System Select/.test(n)), 'system name on System Select is explained');
  const listPlan = exportPlan.screens[1].groups;
  assert.deepEqual(listPlan.map(g => g.kind), ['bake', 'live', 'bake', 'live', 'live', 'live', 'live'], JSON.stringify(listPlan.map(g => g.kind === 'live' ? g.node.name : g.nodes.map(n => n.name))));
  assert.deepEqual([listPlan[0].nodes.map(n => n.name), listPlan[2].nodes.map(n => n.name)], [['Background'], ['Game list area']], 'a live layer between static ones splits them, keeping the order');
  assert.ok(exportPlan.live.includes('the game list'));
  const turned = JSON.parse(JSON.stringify(readyTheme)); turned.scenes.find(s => s.role === 'game_list').nodes.find(n => n.name === 'Game title').layout.rotation = 10;
  assert.ok(planExport(turned).notes.some(n => /upright and solid/.test(n)));
  const listSceneReady = readyTheme.scenes.find(s => s.role === 'game_list');
  assert.ok(liveLayer(listSceneReady, listSceneReady.nodes.find(n => n.type === 'game_list')) && !liveLayer(readyTheme.scenes.find(s => s.role === 'system_select'), readyTheme.scenes.find(s => s.role === 'system_select').nodes[1]));
  const fakeBaked = new Map(exportPlan.screens.flatMap(s => s.groups).filter(g => g.kind === 'bake').map((g, i) => [g, { encoded: 'x', info: { sha256: String(i).repeat(64).slice(0, 64), path: `assets/images/${String(i).repeat(64).slice(0, 64)}.png`, bytes: 10, width: 1920, height: 1080 } }]));
  const exported = exportDocument(readyTheme, exportPlan, fakeBaked);
  assert.deepEqual(exported.scenes.map(s => s.role), ['system_select', 'game_list']); assert.deepEqual(exported.targets, []);
  assert.equal(exported.assets.length, fakeBaked.size); assert.ok(exported.assets.every(a => a.mediaType === 'image/png' && /combined by RetroDrive/.test(a.attribution)));
  const exportedList = exported.scenes[1].nodes;
  assert.equal(exportedList[0].type, 'image'); assert.deepEqual(exportedList[0].layout.width, { value: 100, unit: '%' }); assert.equal(exportedList[0].style.imageFit, 'contain');
  assert.ok(exportedList.every(n => n.type === 'image' ? n.layout.width.value === 100 && n.assets.length === 1 : ['game_list', 'text'].includes(n.type) && n.layout.rotation === 0 && !('opacity' in n.style) && !('editor' in n)));
  assert.deepEqual(exportedList.map(n => n.type), ['image', 'text', 'image', 'game_list', 'text', 'text', 'text']);
  assert.ok(!JSON.stringify(exported).includes('Startup'), 'nothing from other screens is exported');
  // Picture placement, AI credit and delete (ADR-110).
  const { deleteNode, creditFor, encodePictureFile } = require(path.join(output, 'tests/theme-studio/workspace.js'));
  assert.equal(creditFor('', true), 'Made with an AI tool'); assert.equal(creditFor('Sam', true), 'Sam (made with an AI tool)');
  assert.equal(creditFor('Sam', false), 'Sam'); assert.equal(creditFor('AI art by Sam', true), 'AI art by Sam');
  const readyDoc = newDocument({ ...form, template: 'ready' }); const listScene = readyDoc.scenes.find(s => s.role === 'game_list');
  const bg = attachImage(readyDoc, listScene.id, pngInfo, 'original', '', { placement: 'background', aiMade: true });
  const bgNodes = bg.project.scenes.find(s => s.id === listScene.id).nodes;
  assert.equal(bgNodes[1].id, bg.id, 'background picture goes just above the full-screen colour panel'); assert.equal(bgNodes[0].name, 'Background');
  assert.deepEqual([bgNodes[1].layout.width, bgNodes[1].layout.height], [{ value: 100, unit: '%' }, { value: 100, unit: '%' }]); assert.equal(bgNodes[1].style.imageFit, 'cover');
  assert.equal(bg.project.assets[0].attribution, 'Made with an AI tool'); assert.match(bg.project.assets[0].source, /AI tool/);
  const blankBg = attachImage(project, scene.id, pngInfo, 'original', '', { placement: 'background' });
  assert.equal(blankBg.project.scenes[0].nodes[0].id, blankBg.id, 'with no colour panel the background picture is at the bottom');
  const area = listScene.nodes.find(n => n.name === 'Game list area');
  const framed = attachImage(readyDoc, listScene.id, pngInfo, 'original', '', { placement: 'over', overId: area.id });
  const frameNodes = framed.project.scenes.find(s => s.id === listScene.id).nodes; const areaIndex = frameNodes.findIndex(n => n.id === area.id);
  assert.equal(frameNodes[areaIndex + 1].id, framed.id, 'a frame sits right above the layer it covers'); assert.deepEqual(frameNodes[areaIndex + 1].layout, area.layout); assert.equal(frameNodes[areaIndex + 1].name, 'Frame');
  assert.throws(() => attachImage(readyDoc, listScene.id, pngInfo, 'original', '', { placement: 'over', overId: 'missing' }));
  const removed = deleteNode(framed.project, listScene.id, framed.id);
  assert.ok(!removed.scenes.find(s => s.id === listScene.id).nodes.some(n => n.id === framed.id)); assert.equal(removed.scenes.find(s => s.id === listScene.id).nodes.length, listScene.nodes.length);
  const lockedFrame = setLayerState(framed.project, listScene.id, framed.id, 'locked', true);
  assert.equal(deleteNode(lockedFrame, listScene.id, framed.id), lockedFrame, 'locked layers are not deleted');
  assert.equal(deleteNode(framed.project, listScene.id, 'missing'), framed.project);
  assert.equal((await encodePictureFile(new File([Uint8Array.from([137,80,78,71,13,10,26,10,0,0,0,13,73,72,68,82,0,0,0,2,0,0,0,2])],'tiny.png',{type:'image/png'}))).note, '', 'small PNGs pass through unchanged');
  await assert.rejects(() => encodePictureFile(new File([], 'empty.jpg')));
  assert.ok(!safePngPreview('https://untrusted/image.png'));assert.ok(!safePngPreview('data:image/svg+xml;base64,AAAA'));
  assert.ok(safePngPreview('data:image/png;base64,iVBORw=='));
  const testFile=new File([Uint8Array.from([137,80,78,71,13,10,26,10,0])],'file.png',{type:'image/png'});
  assert.equal(await encodePngFile(testFile),'iVBORw0KGgoA');
  await assert.rejects(()=>encodePngFile(new File(['<svg/>'],'file.png')));
  await assert.rejects(()=>encodePngFile(new File([],'file.png')));
  const imageHistory=new EditHistory();imageHistory.record(project,attached.project,'Import image');
  assert.deepEqual(imageHistory.undo(attached.project),project);assert.deepEqual(imageHistory.redo(project),attached.project);

  assert.deepEqual(imagePresentation(imageNode).crop,{x:0,y:0,width:100,height:100});
  assert.equal(setImageFit(imageNode,'contain'),imageNode,'default fit is a no-op');
  const filledImage=setImageFit(imageNode,'cover');assert.equal(filledImage.style.imageFit,'cover');
  const cropImage=setImageCrop(filledImage,'x',50);
  assert.deepEqual(imagePresentation(cropImage).crop,{x:50,y:0,width:50,height:100});
  assert.equal(cropImage.layout,imageNode.layout);assert.equal(cropImage.assets,imageNode.assets);
  assert.deepEqual(cropPixels(imagePresentation(cropImage).crop,640,360),{x:320,y:0,width:320,height:360});
  for(const value of [NaN,Infinity,-1,100])assert.equal(setImageCrop(imageNode,'x',value),imageNode);
  assert.equal(setImageCrop(cropImage,'width',60),cropImage,'crop cannot extend outside original image');
  const tinyCrop=setImageCrop(imageNode,'x',99.9);assert.equal(imagePresentation(tinyCrop).crop.width,.1);
  assert.deepEqual(imagePresentation(resetImageCrop(cropImage)).crop,{x:0,y:0,width:100,height:100});
  assert.equal(resetImageCrop(imageNode),imageNode);
  const opaqueCrop={...imageNode,style:{imageCrop:{future:'expression'},imageFit:{token:'future'}}};
  assert.equal(setImageCrop(opaqueCrop,'x',10),opaqueCrop);assert.equal(setImageFit(opaqueCrop,'cover'),opaqueCrop);assert.equal(resetImageCrop(opaqueCrop),opaqueCrop);
  const extraCrop={...cropImage,style:{...cropImage.style,imageCrop:{...imagePresentation(cropImage).crop,futureAnchor:'kept'}}};
  assert.equal(setImageCrop(extraCrop,'y',10).style.imageCrop.futureAnchor,'kept');
  assert.equal(resetImageCrop(extraCrop).style.imageCrop.futureAnchor,'kept');
  const lockedCrop={...imageNode,editor:{locked:true}};assert.equal(setImageCrop(lockedCrop,'x',10),lockedCrop);assert.equal(setImageFit(lockedCrop,'cover'),lockedCrop);
  const cropHistory=new EditHistory();cropHistory.record(imageNode,cropImage,'Crop');assert.deepEqual(cropHistory.undo(cropImage),imageNode);assert.deepEqual(cropHistory.redo(imageNode),cropImage);

  const croppedProject=updateNode(attached.project,scene.id,attached.id,()=>cropImage);
  let cropStored={...record(),savedJson:JSON.stringify(attached.project)};
  const cropSession=new ProjectSession(cropStored,attached.project,async(id,revision,json,kind)=>{
    assert.equal(revision,cropStored.revision);cropStored={...cropStored,revision:revision+1,savedJson:kind==='checkpoint'?json:cropStored.savedJson,draftJson:kind==='checkpoint'?null:json};return cropStored;
  });
  cropSession.edit(croppedProject,'Crop image');await cropSession.save();
  const croppedCheckpoint=cropStored.savedJson;
  assert.deepEqual(JSON.parse(croppedCheckpoint),croppedProject);
  cropSession.undo();await cropSession.flush();assert.deepEqual(JSON.parse(cropStored.draftJson),attached.project);
  assert.equal(cropStored.savedJson,croppedCheckpoint,'undo does not rewrite a checkpoint');
  cropSession.redo();await cropSession.flush();assert.deepEqual(cropSession.document,croppedProject);
  assert.deepEqual(new ProjectSession(cropStored,JSON.parse(croppedCheckpoint)).document,croppedProject);

  const staticSample={...project.scenes[0].nodes[0],style:{text:'Kept static text'},bindings:{other:{source:'game.players',kind:'number'}}};
  for(const option of textSources){
    const bound=setTextBinding(staticSample,option.source);
    assert.equal(bound.bindings.text.kind,option.kind);assert.equal(bound.bindings.text.source,option.source);
    assert.deepEqual(bound.bindings.other,staticSample.bindings.other);assert.equal(bound.style,staticSample.style);
    assert.notEqual(sampleText(bound,'sample'),`{${option.source}}`);
    assert.equal(sampleText(bound,'placeholders'),`{${option.source}}`);
    const restored=setTextBinding(bound,'');assert.equal(sampleText(restored,'long'),'Kept static text');assert.deepEqual(restored.bindings,staticSample.bindings);
  }
  const boundTitle=setTextBinding(staticSample,'game.title');
  assert.equal(sampleText(boundTitle,'sample'),'Sample Adventure');
  assert.ok(sampleText(boundTitle,'long').includes('??'));
  assert.equal(sampleText(setTextBinding(staticSample,'game.year'),'missing'),'{game.year}');
  assert.equal(sampleText(setTextBinding(staticSample,'game.rating'),'sample'),'0.85');
  assert.equal(setTextBinding(staticSample,'eval(script)'),staticSample);
  assert.equal(setTextBinding(boundTitle,'game.title'),boundTitle);
  const futureBinding={...boundTitle,bindings:{...boundTitle.bindings,text:{...boundTitle.bindings.text,futureFormat:{keep:true}}}};
  assert.deepEqual(setTextBinding(futureBinding,'game.year').bindings.text.futureFormat,{keep:true});
  const imageText={...staticSample,bindings:{text:{source:'game.artwork',kind:'image'}}};
  assert.equal(textBindingEditable(imageText),false);assert.equal(setTextBinding(imageText,'game.title'),imageText);assert.equal(sampleText(imageText,'sample'),'{game.artwork}');
  assert.equal(setTextBinding({...staticSample,editor:{locked:true}},'game.title').bindings.text,undefined);
  const sampleCheckpoint={...project,scenes:project.scenes.map((scene,index)=>index?scene:{...scene,nodes:[boundTitle]})};
  const beforeSamples=JSON.stringify(sampleCheckpoint);
  for(const dataset of previewDatasets)sampleText(boundTitle,dataset.id);
  assert.equal(JSON.stringify(sampleCheckpoint),beforeSamples,'sample selection never mutates portable source');
  assert.ok(!beforeSamples.includes('Sample Adventure'));
  const bindingHistory=new EditHistory();bindingHistory.record(project,sampleCheckpoint,'Bind text');assert.deepEqual(bindingHistory.undo(sampleCheckpoint),project);assert.deepEqual(bindingHistory.redo(project),sampleCheckpoint);

  let bindingStored={...record(),savedJson:JSON.stringify(project)};
  const bindingSession=new ProjectSession(bindingStored,project,async(id,revision,json,kind)=>{
    assert.equal(revision,bindingStored.revision);bindingStored={...bindingStored,revision:revision+1,savedJson:kind==='checkpoint'?json:bindingStored.savedJson,draftJson:kind==='checkpoint'?null:json};return bindingStored;
  });
  bindingSession.edit(sampleCheckpoint,'Change text binding');await bindingSession.save();
  assert.deepEqual(JSON.parse(bindingStored.savedJson),sampleCheckpoint);
  assert.ok(!bindingStored.savedJson.includes('Sample Adventure'));
  bindingSession.undo();await bindingSession.flush();assert.deepEqual(JSON.parse(bindingStored.draftJson),project);
  assert.deepEqual(new ProjectSession(bindingStored,JSON.parse(bindingStored.savedJson)).document,sampleCheckpoint);

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
    assert.match(wizard, /Any system \(choose later\)/);
    assert.match(wizard, /You don&#x27;t need a system connected/);
    const editor = render('theme-studio/ProjectOverview', { session, onClose() {}, onCopy() {}, onReload() {} });
    for (const region of ['Select screen', 'Layer list', 'Scene canvas', 'Properties inspector']) assert.ok(editor.includes(region), region);
    assert.ok(!editor.includes('ts-timeline'), 'the animation placeholder is Pro only (Standard by default)');
    assert.match(editor, /Changes kept|Saved|Saving/);
    assert.match(editor, /any system \(choose later\)/); assert.ok(!/unverified|checkpoint|Universal/.test(editor), 'plain wording');
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
    assert.equal(component('theme-studio/LayerOrderControls')({ ...orderProps, selected: null }), null, 'no order buttons without a selected layer');
    for (const overrides of [{ disabled: true }, { scene: unknownLayered.scenes[0] }, { selected: originalOrder[2] }]) {
      orderRequest = null;
      const blockedControls = component('theme-studio/LayerOrderControls')({ ...orderProps, ...overrides });
      blockedControls.props.children.find(Array.isArray).find(button => button.props.children === 'Bring to front').props.onClick();
      assert.equal(orderRequest, null, 'disabled, unsupported, absent and boundary actions do not edit');
    }
    const inspector = render('theme-studio/NodeInspector', { scene: safeProject.project.scenes[0], node: safeNode, disabled: false, onChange() {}, onEnd() {} });
    assert.match(inspector, /X position/); assert.match(inspector, /Font size/);
    const longText = { ...safeNode, style: { ...safeNode.style, text: 'A'.repeat(501) } };
    assert.match(render('theme-studio/NodeInspector', { scene: safeProject.project.scenes[0], node: longText, disabled: false, onChange() {}, onEnd() {} }), /can&#x27;t be changed here yet; it&#x27;s kept/);
    const unknownText = { ...safeNode, style: { ...safeNode.style, text: { futureExpression: 'kept' } } };
    assert.match(render('theme-studio/NodeInspector', { scene: safeProject.project.scenes[0], node: unknownText, disabled: false, onChange() {}, onEnd() {} }), /can&#x27;t be changed here yet; it&#x27;s kept/);
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
    // Level-gated tools sit in fragments (ADR-108); look inside them.
    const flat = items => [items].flat(Infinity).flatMap(child => child?.type === React.Fragment ? flat(child.props.children) : [child]);
    const transformFields = flat(transformForm.props.children.find(child => child?.type === 'fieldset').props.children);
    const rotationField = transformFields.find(child => child?.props?.label === 'Rotation (degrees)');
    rotationField.props.onChange(30);
    assert.equal(renamedLayer.node.layout.rotation, 30);
    assert.equal(renamedLayer.group, `transform:${transformSource.id}:rotation`);
    const opacityField = transformFields.find(child => child?.props?.label === 'Opacity (0 to 1)');
    opacityField.props.onChange(0.5);
    assert.equal(renamedLayer.node.style.opacity, 0.5);
    renamedLayer = null;
    const disabledTransform = component('theme-studio/NodeInspector')({ ...transformProps, disabled: true });
    flat(disabledTransform.props.children.find(child => child?.type === 'fieldset').props.children).find(child => child?.props?.label === 'Rotation (degrees)').props.onChange(60);
    assert.equal(renamedLayer, null);
    assert.match(render('theme-studio/NodeInspector', { ...transformProps, node: opaqueOpacity }), /opacity can&#x27;t be changed here; it&#x27;s kept/);
    const transformedCanvas = render('theme-studio/SceneCanvas', { scene: transformProject.scenes[0], selected: added.id, disabled: false, onSelect() {}, onMove() {}, onResize() {}, onEndGroup() {} });
    assert.match(transformedCanvas, /rotate\(-45\.5\)/);
    assert.match(transformedCanvas, /opacity="0.25"/);
    let alignRequest = null;
    const alignProps = { scene: alignmentScene, node: alignmentNode, disabled: false, onAlign(action) { alignRequest = action; } };
    const alignmentControls = component('theme-studio/AlignmentControls')(alignProps);
    alignmentControls.props.children.find(Array.isArray).find(button => button.props['aria-label'] === 'Align left').props.onClick();
    assert.equal(alignRequest, 'left');
    assert.equal(component('theme-studio/AlignmentControls')({ ...alignProps, node: null }), null, 'no align buttons without a selected layer');
    for (const overrides of [{ disabled: true }, { node: unknownAlign }, { node: alignedNode(alignmentScene, alignmentNode, 'left') }]) {
      alignRequest = null;
      component('theme-studio/AlignmentControls')({ ...alignProps, ...overrides }).props.children.find(Array.isArray).find(button => button.props['aria-label'] === 'Align left').props.onClick();
      assert.equal(alignRequest, null);
    }
    assert.ok(!editor.includes('Align on the screen'), 'align buttons wait for a selected layer');
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
    toggles.find(button => button.props.title === 'Show').props.onClick();
    assert.deepEqual(flagRequest,[added.id,'hidden',false]);
    assert.match(render('theme-studio/LayerList',listProps),/hidden/);
    flagRequest = null;
    const disabledList = component('theme-studio/LayerList')({ ...listProps,disabled:true });
    const disabledRow = disabledList.props.children.find(child => child?.type === 'div').props.children.find(row => row.key === added.id);
    disabledRow.props.children.find(child => child?.props?.className === 'ts-layer-toggles').props.children[0].props.onClick();
    assert.equal(flagRequest,null);
    const imageCanvasProps={scene:attached.project.scenes[0],selected:attached.id,disabled:false,onSelect(){},onMove(){},onResize(){},onEndGroup(){}};
    const imageCanvas=render('theme-studio/SceneCanvas',{...imageCanvasProps,previewImages:{[imageNode.assets[0]]:'data:image/png;base64,iVBORw=='}});
    assert.match(imageCanvas,/Loading picture/);
    const rejectedImage=render('theme-studio/SceneCanvas',{...imageCanvasProps,previewImages:{[imageNode.assets[0]]:'https://untrusted/image.png'}});
    assert.ok(!rejectedImage.includes('https://untrusted'));assert.match(rejectedImage,/Image unavailable/);
    const simpleInspector = render('theme-studio/NodeInspector', { scene: safeProject.project.scenes[0], node: safeNode, disabled: false, onChange() {}, onEnd() {}, tools: { numbers: false, units: false, transform: false, layerName: false } });
    assert.ok(!/X position|Rotation|Opacity|Layer name/.test(simpleInspector), 'Simple inspector hides numbers'); assert.match(simpleInspector, /Drag the layer/);
    const standardInspector = render('theme-studio/NodeInspector', { scene: safeProject.project.scenes[0], node: safeNode, disabled: false, onChange() {}, onEnd() {}, tools: { numbers: true, units: false, transform: false, layerName: true } });
    assert.match(standardInspector, /X position/); assert.ok(!/units|Rotation/.test(standardInspector), 'Standard hides units and rotation');
    const fitOnly = render('theme-studio/ImageControls', { ...imageCanvasProps, node: imageNode, asset: undefined, url: undefined, showCrop: false, onChange() {}, onEnd() {} });
    assert.ok(!/Crop left/.test(fitOnly) && /Fit: show the whole picture/.test(fitOnly), 'crop numbers only in Pro');
    assert.match(editor, /role="radiogroup" aria-label="Tools"/);
    const importForm=render('theme-studio/ImageImportForm',{sceneName:'Main',async onImport(){},onCancel(){}});
    assert.match(importForm,/License/);assert.match(importForm,/Credit/);assert.match(importForm,/Add picture/);
    assert.match(importForm,/As the screen background/);assert.match(importForm,/Made with an AI tool/);assert.ok(!/as a frame/.test(importForm),'frame option needs a selected layer');assert.match(importForm,/image\/jpeg/);
    assert.match(render('theme-studio/ImageImportForm',{sceneName:'Main',overName:'Game list area',async onImport(){},onCancel(){}}),/Over “Game list area”, as a frame/);
    const videoCanvas = render('theme-studio/SceneCanvas', { scene: withVideo.project.scenes[0], selected: null, disabled: false, onSelect() {}, onMove() {}, onResize() {}, onEndGroup() {} });
    assert.match(videoCanvas, /Game video plays here/);
    assert.ok(!/Text color|Panel color/.test(render('theme-studio/NodeInspector', { scene: withVideo.project.scenes[0], node: videoNode, disabled: false, onChange() {}, onEnd() {} })), 'no colour for a video layer');
    assert.match(render('theme-studio/ImageImportForm',{sceneName:'Main',overName:'Game video',initialPlacement:'over',async onImport(){},onCancel(){}}), /value="on"[^>]*checked=""[^>]*\/> Over “Game video”|checked=""[^>]*\/> Over “Game video”/);
    assert.match(editor, /Add game video/);
    assert.match(editor, /Save for RetroBat…/);
    const listCanvas = render('theme-studio/SceneCanvas', { scene: withList.project.scenes[0], selected: null, disabled: false, onSelect() {}, onMove() {}, onResize() {}, onEndGroup() {} });
    assert.match(listCanvas, /Puzzle Quest/); assert.match(listCanvas, /fill="#378cdd"/);
    const listInspector = render('theme-studio/NodeInspector', { scene: withList.project.scenes[0], node: listNode, disabled: false, onChange() {}, onEnd() {} });
    assert.match(listInspector, /Highlight bar/); assert.match(listInspector, /Selected game name/); assert.match(listInspector, /Text alignment/); assert.ok(!/Text color|Panel color/.test(listInspector));
    assert.ok(!editor.includes('Add game list'), 'Add game list is only on the Game List screen');
    let alignedEdit=null;
    const typographyProps={node:typographyNode,disabled:false,onChange(next){alignedEdit=next;},onEnd(){}};
    const alignmentSelect=component('theme-studio/TextAlignmentControls')(typographyProps).props.children.find(child=>child?.type==='select');
    alignmentSelect.props.onChange({target:{value:'center'}});
    assert.equal(alignedEdit.style.textAlign,'center');
    alignedEdit=null;
    component('theme-studio/TextAlignmentControls')({...typographyProps,disabled:true}).props.children.find(child=>child?.type==='select').props.onChange({target:{value:'right'}});
    assert.equal(alignedEdit,null);
    assert.match(render('theme-studio/TextAlignmentControls',{...typographyProps,node:{...typographyNode,style:{textAlign:'future'}}}),/Other alignment \(kept\)/);
    let bindingEdit=null;
    const bindingProps={node:staticSample,disabled:false,onChange(next){bindingEdit=next;},onEnd(){}};
    const bindingControl=component('theme-studio/TextBindingControls')(bindingProps);
    const bindingSelect=bindingControl.props.children.find(child=>child?.type==='label').props.children.find(child=>child?.type==='select');
    bindingSelect.props.onChange({target:{value:'game.players'}});assert.equal(bindingEdit.bindings.text.kind,'number');
    bindingEdit=null;
    const blockedBinding=component('theme-studio/TextBindingControls')({...bindingProps,disabled:true});
    blockedBinding.props.children.find(child=>child?.type==='label').props.children.find(child=>child?.type==='select').props.onChange({target:{value:'game.title'}});
    assert.equal(bindingEdit,null);
    assert.match(render('theme-studio/TextBindingControls',{...bindingProps,node:imageText}),/can’t show \(kept\)/);
    const browser = render('theme-studio/ProjectBrowser', { projects: [], busy: false, offset: 0, onNew() {}, onOpen() {}, onDuplicate() {}, onPage() {}, onRefresh() {} });
    assert.match(browser, /No themes yet/);
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
    assert.match(fallback, /No system or account is needed/);
  } finally { Module._load = originalLoad; }
  console.log('Theme workspace: creation, saves/close/copy regressions, scene editing, bounded history/in-flight undo and safe React/SVG rendering passed.');
}
main().catch(error => { console.error(error); process.exitCode = 1; }).finally(() => {
  assert.equal(path.dirname(output), cache); assert.ok(path.basename(output).startsWith('theme-workspace-'));
  fs.rmSync(output, { recursive: true, force: true });
});
