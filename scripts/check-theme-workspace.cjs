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
  const { ProjectSession, project, form, record, newDocument, guardedWindowClose } = require(path.join(output, 'tests/theme-studio/workspace.js'));
  assert.deepEqual(project.targets, []);
  assert.equal(project.scenes[0].resolution.width, 1080);
  assert.equal(project.scenes[0].resolution.height, 1920);
  assert.equal(new Set(project.scenes.flatMap(s => s.nodes.map(n => n.id))).size, 8);
  assert.equal(newDocument({ ...form, target: 'retrobat', targetVersion: '' }).targets[0].frontendVersion, null);
  assert.equal(newDocument({ ...form, template: 'blank' }).scenes.flatMap(s => s.nodes).length, 0);

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
    const browser = render('theme-studio/ProjectBrowser', { projects: [], busy: false, offset: 0, onNew() {}, onOpen() {}, onDuplicate() {}, onPage() {}, onRefresh() {} });
    assert.match(browser, /No projects yet/);
    const customize = render('Customize', { installs: [], native: true, busy: false, notify() {} });
    assert.match(customize, /Add a RetroBat or Batocera installation/);
    const shell = render('App', {});
    assert.ok(shell.indexOf('>Customize</button>') < shell.indexOf('>Theme Studio</button>'));
    assert.ok(shell.indexOf('>Theme Studio</button>') < shell.indexOf('>Settings</button>'));
    const fallback = render('ThemeStudio', { native: false, leaveGuard: { current: null } });
    assert.match(fallback, /No emulator installation or cloud account is required/);
  } finally { Module._load = originalLoad; }
  console.log('Theme workspace: creation, queued saves, in-flight edits, failure/conflict protection, real SDK close/permission regression and React rendering passed.');
}
main().catch(error => { console.error(error); process.exitCode = 1; }).finally(() => {
  assert.equal(path.dirname(output), cache); assert.ok(path.basename(output).startsWith('theme-workspace-'));
  fs.rmSync(output, { recursive: true, force: true });
});
