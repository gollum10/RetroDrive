// Uses the existing TypeScript dependency and Node assertions; no new test packages.
const ts = require('typescript');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const root = path.resolve(__dirname, '..');
const output = fs.mkdtempSync(path.join(os.tmpdir(), 'retrodrive-theme-contract-'));
try {
  const program = ts.createProgram([path.join(root, 'tests/theme-studio/contract.ts')], {
    target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.CommonJS, strict: true,
    skipLibCheck: true, outDir: output, rootDir: root,
  });
  const diagnostics = ts.getPreEmitDiagnostics(program);
  if (diagnostics.length) {
    throw new Error(ts.formatDiagnosticsWithColorAndContext(diagnostics, {
      getCanonicalFileName: f => f, getCurrentDirectory: () => root, getNewLine: () => '\n',
    }));
  }
  assert.equal(program.emit().emitSkipped, false);
  const { blank, starter, futureNode } = require(path.join(output, 'tests/theme-studio/contract.js'));
  for (const [name, project] of Object.entries({ blank, starter })) {
    const rustFixture = JSON.parse(fs.readFileSync(path.join(root, `src-tauri/tests/fixtures/theme_studio/${name}.json`), 'utf8'));
    assert.deepEqual(JSON.parse(JSON.stringify(project)), rustFixture);
  }
  assert.equal(futureNode.futurePayload.preserved, true);
  console.log('Theme contract: TypeScript positive/negative checks and shared Rust JSON fixture passed.');
} finally {
  // Only this invocation's freshly created, absolute temp directory is removed.
  assert.equal(path.dirname(output), path.resolve(os.tmpdir()));
  assert.ok(path.basename(output).startsWith('retrodrive-theme-contract-'));
  fs.rmSync(output, { recursive: true, force: true });
}
