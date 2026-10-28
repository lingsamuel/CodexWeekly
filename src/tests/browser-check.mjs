import { chromium } from '@playwright/test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import { existsSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createServer } from 'node:http';
import { fileURLToPath } from 'node:url';

// 仅运行 Linux headless；不连接 Windows CDP，也不产生系统输入。
const cached = path.join(os.homedir(), '.cache/ms-playwright/chromium-1148/chrome-linux/chrome');
const executablePath = process.env.CHROMIUM_EXECUTABLE ?? (existsSync(cached) ? cached : undefined);
const output = process.env.BROWSER_CHECK_OUTPUT ?? '/tmp/codex-overlay-ui-013';
await fs.mkdir(output, { recursive: true });
// 发布页只从 dist 加载，响应头使用当前 Tauri CSP，覆盖 Vite 开发模式看不到的限制。
async function verifyProductionCsp(browser) {
  const repo = fileURLToPath(new URL('../../', import.meta.url));
  const config = JSON.parse(await fs.readFile(path.join(repo, 'src-tauri/tauri.conf.json'), 'utf8'));
  const csp = config.app.security.csp;
  assert.equal(typeof csp, 'string');
  const dist = path.resolve(repo, 'src-tauri', config.build.frontendDist);
  await fs.access(path.join(dist, 'index.html'));
  const mime = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.svg': 'image/svg+xml', '.png': 'image/png' };
  const server = createServer(async (request, response) => {
    try {
      const pathname = decodeURIComponent(new URL(request.url, 'http://127.0.0.1').pathname);
      const file = path.resolve(dist, `.${pathname === '/' ? '/index.html' : pathname}`);
      if (!file.startsWith(`${dist}${path.sep}`)) { response.writeHead(403); response.end(); return; }
      const content = await fs.readFile(file);
      response.writeHead(200, { 'Content-Type': mime[path.extname(file)] ?? 'application/octet-stream', 'Content-Security-Policy': csp, 'Cache-Control': 'no-store' });
      response.end(content);
    } catch {
      response.writeHead(404, { 'Content-Security-Policy': csp });
      response.end();
    }
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const context = await browser.newContext({ viewport: { width: 176, height: 176 } });
  try {
    await context.addInitScript(() => {
      const probe = { violations: [], renders: [], blobsCreated: 0, blobsRevoked: 0 };
      window.__trayCspProbe = probe;
      document.addEventListener('securitypolicyviolation', event => {
        probe.violations.push({ directive: event.effectiveDirective, blockedURI: event.blockedURI });
      });
      const create = URL.createObjectURL, revoke = URL.revokeObjectURL;
      URL.createObjectURL = function (blob) { probe.blobsCreated++; return create.call(this, blob); };
      URL.revokeObjectURL = function (url) { probe.blobsRevoked++; return revoke.call(this, url); };
      const read = CanvasRenderingContext2D.prototype.getImageData;
      CanvasRenderingContext2D.prototype.getImageData = function (...args) {
        const image = read.apply(this, args);
        if (this.canvas.width === 64 && this.canvas.height === 64) {
          let opaque = 0, darkCenter = 0;
          for (let index = 0; index < image.data.length; index += 4) {
            if (image.data[index + 3] > 0) opaque++;
            if (image.data[index + 3] > 0 && image.data[index] < 110 && image.data[index + 1] < 150) darkCenter++;
          }
          probe.renders.push({ bytes: image.data.length, transparentCorner: image.data[3] === 0, typed: image.data instanceof Uint8ClampedArray, opaque, darkCenter });
        }
        return image;
      };
    });
    const productionPage = await context.newPage();
    const runtimeErrors = [];
    productionPage.on('pageerror', error => runtimeErrors.push(error.message));
    const address = server.address();
    const response = await productionPage.goto(`http://127.0.0.1:${address.port}/?view=overlay&demo=1`);
    assert.equal(response.headers()['content-security-policy'], csp);
    await productionPage.getByRole('img', { name: '4 个正在工作中的会话' }).waitFor();
    // 不手动调用渲染 helper，必须由发布包 Overlay 自己生成真实 RGBA。
    await productionPage.waitForFunction(() => window.__trayCspProbe.renders.some(render => render.bytes === 16384 && render.darkCenter > 100));
    const probe = await productionPage.evaluate(() => window.__trayCspProbe);
    assert.deepEqual(probe.violations, []);
    assert.deepEqual(runtimeErrors, []);
    assert.ok(probe.blobsCreated > 0);
    assert.equal(probe.blobsCreated, probe.blobsRevoked);
    assert.ok(probe.renders.some(render => render.typed && render.transparentCorner && render.opaque > 1000));
    await productionPage.screenshot({ path: path.join(output, 'production-csp-overlay.png'), omitBackground: true });
    return { cspViolations: probe.violations.length, automaticRgbaBytes: probe.renders.at(-1).bytes, blobsRevoked: probe.blobsRevoked };
  } finally {
    await context.close();
    await new Promise(resolve => { server.close(resolve); server.closeAllConnections(); });
  }
}

const browser = await chromium.launch({ executablePath, headless: true, args: ['--no-sandbox'] });
try {
  const page = await browser.newPage({ viewport: { width: 1040, height: 760 }, deviceScaleFactor: 1 });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('http://127.0.0.1:1420/?demo=1');
  await page.getByRole('img', { name: /最近 24 小时/ }).waitFor();
  assert.equal(await page.locator('.legend-gap').count(), 0);
  assert.equal(await page.getByText('采样间隙', { exact: true }).count(), 0);
  const historyFixture = process.env.BROWSER_HISTORY_FIXTURE ? JSON.parse(await fs.readFile(process.env.BROWSER_HISTORY_FIXTURE, 'utf8')) : null;
  await page.evaluate(async historyFixture => {
    const reactModule = await import('/node_modules/.vite/deps/react.js');
    const React = reactModule.default ?? reactModule;
    const clientModule = await import('/node_modules/.vite/deps/react-dom_client.js');
    const { createRoot } = clientModule.default ?? clientModule;
    const { HistoryChart } = await import('/src/components/HistoryChart.tsx');
    document.querySelector('.dashboard').style.display = 'none';
    const container = document.createElement('section');
    container.id = 'gap-fixture';
    container.style.cssText = 'padding:30px;background:#fffefa;width:1040px';
    document.body.append(container);
    const now = Math.floor(Date.now() / 1000);
    const offsets = [[0, 65, 'a'], [180, 65, 'a'], [240, 65, 'b'], [300, 65, 'b'], [360, 64, 'c'], [420, 64, 'd'], [540, 64, 'd'], [600, 64, 'e'], [780, 64, 'e'], [840, 64, 'f'], [960, 64, 'f'], [1200, 62, 'f'], [1440, 61, 'f'], [1500, 60, 'f'], [1560, 60, 'g'], [1740, 60, 'g']];
    const points = historyFixture ?? offsets.map(([offset, remainingPercent, segment]) => ({ recordedAt: now - 1740 + offset, remainingPercent, segment, resetsAt: now + 86400, windowMinutes: 10080 }));
    createRoot(container).render(React.createElement(HistoryChart, { points, hours: 24, loading: false }));
  }, historyFixture);
  await page.locator('#gap-fixture .history-line').waitFor({ state: 'attached' });
  assert.equal(await page.locator('#gap-fixture .history-line').count(), 1);
  assert.equal(await page.locator('#gap-fixture .history-area').count(), 1);
  assert.equal(await page.locator('#gap-fixture .history-gap').count(), 0);
  assert.equal(await page.locator('#gap-fixture [stroke-dasharray]').count(), 0);
  const continuousCheck = await page.locator('#gap-fixture .history-line').evaluate(path => {
    const d = path.getAttribute('d');
    return { moves: (d.match(/M/g) ?? []).length, cubic: d.includes('C'), solid: getComputedStyle(path).strokeDasharray === 'none' };
  });
  assert.deepEqual(continuousCheck, { moves: 1, cubic: true, solid: true });
  await page.locator('#gap-fixture').screenshot({ path: path.join(output, 'history-continuous.png') });

  await page.setViewportSize({ width: 176, height: 176 });
  await page.goto('http://127.0.0.1:1420/?view=overlay&demo=1');
  await page.getByRole('img', { name: '4 个正在工作中的会话' }).waitFor();
  await page.getByRole('img', { name: '2 个已完成但尚未阅读的会话' }).waitFor();
  await page.screenshot({ path: path.join(output, 'overlay-badges.png'), omitBackground: true });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), 176);
  assert.equal(await page.evaluate(() => document.documentElement.scrollHeight), 176);
  const unrelatedRenders = await page.evaluate(async () => {
    await document.fonts.ready;
    await new Promise(resolve => setTimeout(resolve, 40));
    const originalCreate = URL.createObjectURL;
    let created = 0;
    URL.createObjectURL = blob => { created++; return originalCreate.call(URL, blob); };
    try {
      const api = await import('/src/lib/api.ts');
      await api.setLocked(true);
      await api.setLocked(false);
      await new Promise(resolve => setTimeout(resolve, 40));
      return created;
    } finally { URL.createObjectURL = originalCreate; }
  });
  assert.equal(unrelatedRenders, 0);
  const tray = await page.evaluate(async () => {
    const { renderTrayImage, serializeTraySvg } = await import('/src/lib/trayImage.ts');
    const source = document.querySelector('.quota-ring > svg');
    const svg = serializeTraySvg(source, 73, false);
    const rgba = await renderTrayImage(source, 73, false);
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 64;
    canvas.id = 'tray-preview';
    canvas.style.cssText = 'position:fixed;top:0;left:0;width:64px;height:64px;z-index:10';
    const context = canvas.getContext('2d');
    context.putImageData(new ImageData(rgba, 64, 64), 0, 0);
    document.body.append(canvas);
    let darkCenter = 0, greenRing = 0;
    for (let y = 0; y < 64; y++) for (let x = 0; x < 64; x++) {
      const offset = (y * 64 + x) * 4;
      if (rgba[offset + 3] === 0) continue;
      if (x > 10 && x < 54 && y > 15 && y < 49 && rgba[offset] < 110 && rgba[offset + 1] < 150) darkCenter++;
      if (rgba[offset + 1] > rgba[offset] + 25 && rgba[offset + 1] > rgba[offset + 2]) greenRing++;
    }
    const originalCreate = URL.createObjectURL, originalRevoke = URL.revokeObjectURL;
    let created = 0, revoked = 0, abortName = '';
    const controller = new AbortController();
    URL.createObjectURL = blob => { created++; const url = originalCreate.call(URL, blob); queueMicrotask(() => controller.abort()); return url; };
    URL.revokeObjectURL = url => { revoked++; return originalRevoke.call(URL, url); };
    try { await renderTrayImage(source, 73, false, controller.signal); }
    catch (error) { abortName = error.name; }
    finally { URL.createObjectURL = originalCreate; URL.revokeObjectURL = originalRevoke; }
    return { length: rgba.length, typed: rgba instanceof Uint8ClampedArray, alphaCorner: rgba[3], darkCenter, greenRing, svg, fontAvailable: document.fonts.check('700 82px Arial'), created, revoked, abortName };
  });
  assert.equal(tray.length, 64 * 64 * 4);
  assert.equal(tray.typed, true);
  assert.equal(tray.alphaCorner, 0);
  assert.ok(tray.darkCenter > 100);
  assert.ok(tray.greenRing > 100);
  assert.equal(tray.fontAvailable, true);
  assert.match(tray.svg, /url\(#/);
  assert.doesNotMatch(tray.svg, /url\(["']?https?:/);
  assert.match(tray.svg, />73<\/text>/);
  assert.equal(tray.created, 1);
  assert.equal(tray.revoked, 1);
  assert.equal(tray.abortName, 'AbortError');
  await page.locator('#tray-preview').screenshot({ path: path.join(output, 'tray-64.png'), omitBackground: true });
  await page.locator('#tray-preview').evaluate(element => element.remove());

  await page.evaluate(async () => {
    const reactModule = await import('/node_modules/.vite/deps/react.js');
    const React = reactModule.default ?? reactModule;
    const clientModule = await import('/node_modules/.vite/deps/react-dom_client.js');
    const { createRoot } = clientModule.default ?? clientModule;
    const { SessionBadge } = await import('/src/components/SessionBadge.tsx');
    const fixture = document.createElement('div');
    fixture.id = 'badge-fixture';
    document.body.append(fixture);
    createRoot(fixture).render(React.createElement(React.Fragment, null,
      React.createElement(SessionBadge, { count: 0, kind: 'working' }),
      React.createElement(SessionBadge, { count: null, kind: 'completed' }),
      React.createElement(SessionBadge, { count: 100, kind: 'working' })));
  });
  await page.locator('#badge-fixture .session-badge').waitFor();
  assert.equal(await page.locator('#badge-fixture .session-badge').count(), 1);
  assert.equal(await page.locator('#badge-fixture .session-badge').textContent(), '99+');
  await page.goto('http://127.0.0.1:1420/?view=overlay');
  await page.getByRole('img', { name: 'Weekly 额度尚未同步' }).waitFor();
  assert.equal(await page.locator('.session-badge').count(), 0);
  assert.deepEqual(errors, []);
  const productionCsp = await verifyProductionCsp(browser);
  console.log(JSON.stringify({ status: 'passed', productionCsp, continuousPaths: 1, trayBytes: tray.length, darkCenter: tray.darkCenter, greenRing: tray.greenRing, abortedBlobRevoked: true, output }));
} finally { await browser.close(); }
