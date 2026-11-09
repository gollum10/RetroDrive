// Real Chromium input events; no browser automation dependency or production IPC.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');
const root = path.resolve(__dirname, '..');
const cache = path.join(root, 'node_modules', '.cache');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(read, description) {
  for (let i = 0; i < 150; i++) { const value = await read(); if (value) return value; await delay(100); }
  throw new Error(`Timed out: ${description}`);
}
async function main() {
  const executable = process.env.RETRODRIVE_TEST_BROWSER || 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
  assert.ok(fs.existsSync(executable), 'Set RETRODRIVE_TEST_BROWSER to an installed Chromium/Edge executable');
  fs.mkdirSync(cache, { recursive: true });
  const profile = fs.mkdtempSync(path.join(cache, 'theme-browser-'));
  const { createServer } = await import('vite');
  const server = await createServer({ root, server: { host: '127.0.0.1', port: 0 }, logLevel: 'error' });
  let browser; let socket;
  try {
    await server.listen();
    const port = server.httpServer.address().port;
    browser = spawn(executable, ['--headless=new', '--no-first-run', '--no-default-browser-check', '--disable-background-networking', '--disable-renderer-backgrounding', '--disable-background-timer-throttling', '--disable-backgrounding-occluded-windows', '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank'], { windowsHide: true, stdio: 'ignore' });
    browser.on('error', error => { console.error(error.message); });
    const debugPort = await until(() => { try { return Number(fs.readFileSync(path.join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0]); } catch { return null; } }, 'browser startup');
    const targets = await (await fetch(`http://127.0.0.1:${debugPort}/json/list`)).json();
    socket = new WebSocket(targets.find(target => target.type === 'page').webSocketDebuggerUrl);
    await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
    let sequence = 0; const pending = new Map();
    socket.onmessage = event => { const reply = JSON.parse(event.data); const request = pending.get(reply.id); if (request) { pending.delete(reply.id); reply.error ? request.reject(new Error(JSON.stringify(reply.error))) : request.resolve(reply.result); } };
    const send = (method, params = {}) => new Promise((resolve, reject) => {
      const id = ++sequence; pending.set(id, { resolve, reject }); socket.send(JSON.stringify({ id, method, params }));
      setTimeout(() => { if (pending.delete(id)) reject(new Error(`CDP timeout: ${method}`)); }, 10000).unref();
    });
    const evaluate = async expression => { const result = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }); if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails)); return result.result.value; };
    const frame = () => delay(100);
    const state = () => evaluate('({commits:window.canvasTest.commits,layout:window.canvasTest.document.scenes[0].nodes[0].layout})');
    const handle = () => evaluate("(() => {const r=document.querySelector('circle[role=button]').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()");
    const mouse = (type, x, y) => send('Input.dispatchMouseEvent', { type, x, y, button: type === 'mouseMoved' ? 'none' : 'left', buttons: type === 'mouseReleased' ? 0 : 1, clickCount: 1 });
    const key = async key => { await send('Input.dispatchKeyEvent', { type: 'keyDown', key }); await send('Input.dispatchKeyEvent', { type: 'keyUp', key }); await frame(); };
    await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1100, deviceScaleFactor: 1, mobile: false });
    await send('Page.enable');
    await send('Page.bringToFront');
    await send('Page.navigate', { url: `http://127.0.0.1:${port}/tests/theme-studio/browser.html` });
    await until(() => evaluate('!!window.canvasTest && !!document.querySelector("circle[role=button]")'), 'React canvas');
    await frame();
    const before = await state(); const point = await handle();
    await mouse('mousePressed', point.x, point.y);
    await mouse('mouseMoved', point.x + 60, point.y + 30); await frame();
    assert.equal((await state()).commits, 0, 'pointer preview must not persist');
    await mouse('mouseReleased', point.x + 60, point.y + 30); await frame();
    const resized = await state();
    assert.equal(resized.commits, 1, 'real pointer resize commits once after default browser focus events');
    assert.ok(resized.layout.width.value > before.layout.width.value, 'real pointer drag grows width');
    assert.ok(resized.layout.height.value > before.layout.height.value, 'real pointer drag grows height');
    assert.deepEqual(resized.layout.x, before.layout.x); assert.deepEqual(resized.layout.y, before.layout.y);
    const click = await handle(); await mouse('mousePressed', click.x, click.y); await mouse('mouseReleased', click.x, click.y); await frame();
    assert.equal((await state()).commits, 1, 'click without movement is a no-op');
    const cancel = await handle(); await mouse('mousePressed', cancel.x, cancel.y); await mouse('mouseMoved', cancel.x + 40, cancel.y + 20); await key('Escape'); await mouse('mouseReleased', cancel.x + 40, cancel.y + 20); await frame();
    assert.deepEqual(await state(), resized, 'Escape cancels preview');
    await evaluate("document.querySelector('circle[role=button]').focus()"); await key('ArrowRight');
    assert.equal((await state()).commits, 2, 'focused handle keyboard resize');
    const keyboard = await state();
    const blur = await handle(); await mouse('mousePressed', blur.x, blur.y); await mouse('mouseMoved', blur.x + 40, blur.y + 20); await evaluate("document.querySelector('input').focus()"); await mouse('mouseReleased', blur.x + 40, blur.y + 20); await frame();
    assert.deepEqual(await state(), keyboard, 'leaving canvas cancels preview');
    for (const width of [850, 650]) {
      await evaluate(`window.canvasTest.setWorkspaceWidth(${width})`); await frame();
      const rects = await evaluate("(() => {const r=s=>{const b=document.querySelector(s).getBoundingClientRect();return {x:b.x,y:b.y,width:b.width,right:b.right,bottom:b.bottom}};return {canvas:r('.ts-canvas'),inspector:r('.ts-inspector'),svg:r('.ts-canvas-svg')};})()");
      assert.ok(rects.inspector.y >= rects.canvas.bottom - 1, `${width}: inspector must stack below canvas at workspace width`);
      assert.ok(rects.svg.width >= width - (width === 850 ? 200 : 5), `${width}: canvas should use available space`);
      const start = await handle(); const count = (await state()).commits;
      await mouse('mousePressed', start.x, start.y); await mouse('mouseMoved', start.x + 15, start.y + 10); await mouse('mouseReleased', start.x + 15, start.y + 10); await frame();
      assert.equal((await state()).commits, count + 1, `${width}: resize still works after layout reflow`);
    }
    await send('Emulation.setDeviceMetricsOverride', { width: 1440, height: 1100, deviceScaleFactor: 1.5, mobile: false });
    await evaluate('window.canvasTest.resetDocument()'); await frame();
    await evaluate('window.canvasTest.setRotation(45)'); await frame();
    const rotatedBefore = await state(); const rotatedPoint = await handle();
    await mouse('mousePressed', rotatedPoint.x, rotatedPoint.y); await mouse('mouseMoved', rotatedPoint.x, rotatedPoint.y + 20); await mouse('mouseReleased', rotatedPoint.x, rotatedPoint.y + 20); await frame();
    const rotatedAfter = await state();
    assert.equal(rotatedAfter.commits, rotatedBefore.commits + 1, '45 degree resize at 150% device scale');
    assert.ok(rotatedAfter.layout.width.value > rotatedBefore.layout.width.value);
    assert.ok(rotatedAfter.layout.height.value > rotatedBefore.layout.height.value);
    assert.equal(rotatedAfter.layout.rotation, 45);
    await evaluate('window.canvasTest.setDisabled(true)'); await frame();
    assert.equal(await evaluate('!!document.querySelector("circle[role=button]")'), false);
    console.log('Theme browser: real pointer resize/default focus, preview, no-op, Escape, external blur, keyboard 850/650px workspace layout, rotated resize at 150% scale and disabled handle passed.');
  } finally {
    if (socket && socket.readyState === WebSocket.OPEN) { socket.send(JSON.stringify({ id: 999999, method: 'Browser.close' })); await delay(500); socket.close(); }
    if (browser) { browser.kill(); await delay(500); }
    await server.close();
    assert.equal(path.dirname(profile), cache); assert.ok(path.basename(profile).startsWith('theme-browser-'));
    try { fs.rmSync(profile, { recursive: true, force: true, maxRetries: 8, retryDelay: 200 }); } catch (error) { console.error(`Disposable browser profile cleanup pending: ${profile} (${error.code})`); }
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
