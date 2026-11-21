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
const output = process.env.BROWSER_CHECK_OUTPUT ?? fileURLToPath(new URL('../../artifacts/browser-check/', import.meta.url));
await fs.mkdir(output, { recursive: true });

async function verifySessionDetails(browser) {
  const page = await browser.newPage({ viewport: { width: 176, height: 176 } });
  await page.exposeBinding('__resizeOverlay', async ({ page }, expanded) => {
    await page.setViewportSize({ width: expanded ? 464 : 176, height: 176 });
  });
  const detail = (id, title, project = null, workingDirectory = null, completionId = null, groupKey = null) =>
    ({ id, title, project, workingDirectory, completionId,
      projectGroupKey: groupKey ?? (project ? `project:${project}` : workingDirectory ? `directory:${workingDirectory}` : null) });
  const longTitle = '确认所有会话状态变更后更新部署交接记录'.repeat(8);
  const working = [detail('w0', '同步部署状态', 'Codex'), detail('w1', null, null, 'work\\overlay'),
    detail('long', longTitle, 'Codex'),
    detail('same-1', '检查前端配置', '同名项目', null, null, 'project:frontend'),
    detail('same-2', '检查后端配置', '同名项目', null, null, 'project:backend'),
    ...Array.from({ length: 5 }, (_, index) => detail(`extra-${index}`, `编写文档 ${index}`, '文档'))];
  const completed = [detail('c0', '检查旧日志', '运维', null, 'turn-0')];
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(({ working, completed }) => {
    const listeners = new Map();
    const callbacks = new Map();
    let callbackId = 0;
    let snapshot = {
      sourceRevision: 1, historyRevision: 1,
      sessions: { working: working.length, unreadCompleted: completed.length, workingDetails: working, unreadCompletedDetails: completed, observedAt: 1 },
      sessionError: null, quota: null, status: 'online', error: null, locked: false, overlayVisible: true,
      settings: { source: 'wsl', distro: '', pollIntervalSecs: 60 }, version: 'test',
    };
    const calls = [];
    const historyRequests = [];
    const emit = (event, payload) => {
      for (const id of listeners.get(event) ?? []) callbacks.get(id)?.({ event, id, payload });
    };
    window.isTauri = true;
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      transformCallback(callback) { callbacks.set(++callbackId, callback); return callbackId; },
      async invoke(command, args) {
        if (command === 'plugin:event|listen') {
          if (!listeners.has(args.event)) listeners.set(args.event, new Set());
          listeners.get(args.event).add(args.handler);
          return args.handler;
        }
        if (command === 'plugin:event|unlisten') {
          listeners.get(args.event)?.delete(args.eventId);
          callbacks.delete(args.eventId);
          return;
        }
        calls.push({ command, args });
        if (command === 'get_snapshot') return snapshot;
        if (command === 'list_distros') return [];
        if (command === 'get_history') return new Promise(resolve => { historyRequests.push({ revision: args.revision, resolve }); });
        if (command === 'update_settings') {
          snapshot = { ...snapshot, settings: args.settings, sourceRevision: 2, historyRevision: 2, quota: null, sessions: null, status: 'connecting' };
          emit('state-changed', snapshot);
          return snapshot;
        }
        if (command === 'set_overlay_details') {
          await window.__resizeOverlay(args.expanded);
          return 'right';
        }
      },
    };
    window.__sessionFixture = {
      calls, emit,
      hasPendingHistory: revision => historyRequests.some(request => request.revision === revision),
      completeHistory(revision, points) {
        for (let i = historyRequests.length - 1; i >= 0; i--) {
          if (historyRequests[i].revision === revision) historyRequests.splice(i, 1)[0].resolve(points);
        }
      },
      publish({ working, completed, sessions, ...state }) {
        const current = snapshot.sessions;
        snapshot = {
          ...snapshot, ...state,
          sessions: sessions === null ? null : {
            working: working?.length ?? current?.working ?? 0,
            unreadCompleted: completed === undefined ? current?.unreadCompleted ?? null : completed?.length ?? null,
            workingDetails: working ?? current?.workingDetails ?? [],
            unreadCompletedDetails: completed === undefined ? current?.unreadCompletedDetails ?? null : completed,
            observedAt: (current?.observedAt ?? 0) + 1,
          },
        };
        emit('state-changed', snapshot);
      },
    };
  }, { working, completed });
  const publish = state => page.evaluate(state => window.__sessionFixture.publish(state), state);
  const emit = (name, payload) => page.evaluate(([name, payload]) => window.__sessionFixture.emit(name, payload), [name, payload]);
  const panel = page.getByRole('region', { name: '会话详情' });
  const badge = kind => page.getByRole('img', { name: kind === 'working' ? /正在工作中/ : /回执未读/ });
  const regions = () => page.evaluate(() => window.__sessionFixture.calls.findLast(call => call.command === 'set_overlay_hit_regions')?.args.regions);
  const clockStart = new Date('2026-11-14T00:00:00Z');
  await page.clock.install({ time: clockStart });
  await page.clock.pauseAt(new Date(clockStart.getTime() + 1000));
  await page.goto('http://127.0.0.1:1420/?view=overlay');
  await badge('working').waitFor();
  assert.equal(await panel.count(), 0, '首个快照只建立基准');
  assert.deepEqual((await regions())?.map(region => region.target), ['working', 'completed']);

  await badge('working').hover({ force: true });
  await panel.getByText('同步部署状态').waitFor({ timeout: 2000 });
  assert.equal(page.viewportSize().width, 464);
  const codexProject = panel.getByRole('region', { name: 'Codex', exact: true });
  assert.equal(await panel.getByRole('heading', { name: 'Codex', exact: true }).count(), 1);
  assert.equal(await codexProject.getByRole('listitem').count(), 2);
  assert.equal(await panel.getByRole('heading', { name: '同名项目', exact: true }).count(), 2);
  assert.equal(await panel.getByText('未命名会话', { exact: true }).count(), 1);
  assert.equal(await panel.getByText('工作目录 · work\\overlay', { exact: true }).count(), 1);
  assert.ok((await panel.getByText(longTitle, { exact: true }).boundingBox()).height > 15);
  await panel.hover({ force: true });
  await page.clock.fastForward(500);
  assert.equal(await panel.count(), 1, '从气泡移入详情后保持展开');
  await emit('overlay-details-side', 'left');
  await page.waitForFunction(() => document.querySelector('.overlay-widget').getBoundingClientRect().x === 292);
  assert.ok((await panel.boundingBox()).x < (await badge('working').boundingBox()).x);
  await panel.hover({ force: true });
  await page.mouse.move(0, 0);
  await page.clock.fastForward(500);
  assert.equal(await panel.count(), 1, '左侧列表留出跨越圆环的时间');
  await panel.hover({ force: true });
  await emit('overlay-details-side', 'right');
  await page.waitForFunction(() => document.querySelector('.overlay-widget').getBoundingClientRect().x === 4);
  await panel.hover({ force: true });
  await page.mouse.move(0, 0);
  await page.clock.fastForward(260);
  await panel.waitFor({ state: 'hidden' });
  assert.equal(page.viewportSize().width, 464, '退场动画结束后才还原原生窗口尺寸');
  await page.clock.fastForward(500);
  await page.waitForFunction(() => window.innerWidth === 176);
  assert.deepEqual((await regions())?.map(region => region.target), ['working', 'completed']);

  for (const kind of ['working', 'completed']) {
    const box = await badge(kind).boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.up();
  }
  const mouseActions = await page.evaluate(() => window.__sessionFixture.calls.filter(
    call => ['begin_drag', 'open_dashboard'].includes(call.command)).map(call => call.command));
  assert.deepEqual(mouseActions, ['begin_drag', 'begin_drag']);
  await page.mouse.move(0, 0);
  await page.clock.fastForward(500);
  await page.clock.fastForward(500);

  await emit('overlay-region-hover', 'completed');
  await publish({ locked: true });
  await panel.getByText('检查旧日志').waitFor();
  assert.equal(await panel.getByText('同步部署状态').count(), 0);
  await emit('overlay-region-hover', null);
  await page.clock.fastForward(500);
  await page.clock.fastForward(500);
  await panel.waitFor({ state: 'hidden' });
  assert.equal(await badge('completed').textContent(), '1', '悬停和收起只改变详情显示');

  const replacement = [detail('w2', '验证新会话', 'Codex'), ...working.slice(1)];
  await publish({ working: replacement, completed: [detail('c0', '检查完成回执', 'Codex', null, 'turn-1')] });
  await panel.getByText('验证新会话').waitFor();
  assert.equal(await codexProject.getByText('检查完成回执').count(), 1, '同一项目中的两种状态共用一组');
  assert.ok((await regions())?.some(region => region.target === 'details'));
  await emit('overlay-region-scroll', { deltaY: 150 });
  assert.ok(await panel.evaluate(element => [...element.querySelectorAll('*')].some(child => child.scrollTop > 0)));
  await page.clock.fastForward(7900);
  await publish({ completed: [detail('c0', '检查第二个回执', '运维', null, 'turn-2')] });
  await panel.getByText('检查第二个回执').waitFor();
  await page.clock.fastForward(7900);
  assert.equal(await panel.count(), 1, '新增条目重新开始 8 秒计时');
  await emit('overlay-region-hover', 'details');
  await page.clock.fastForward(200);
  assert.equal(await panel.getByText('检查第二个回执').count(), 1, '计时结束后悬停仍保留详情');
  await emit('overlay-region-hover', null);
  await page.clock.fastForward(500);
  await panel.waitFor({ state: 'hidden' });
  await page.clock.fastForward(500);

  await publish({ working, overlayVisible: false });
  await badge('working').waitFor({ state: 'hidden' });
  assert.deepEqual(await regions(), []);
  await publish({ overlayVisible: true });
  await badge('working').waitFor();
  assert.equal(await panel.count(), 0, '隐藏期间的新增不会在再次显示时弹出');
  await publish({ working: replacement });
  await panel.getByText('验证新会话').waitFor();
  await page.clock.fastForward(7999);
  assert.equal(await panel.count(), 1);
  await page.clock.fastForward(1);
  await panel.waitFor({ state: 'hidden' });
  await page.clock.fastForward(500);
  await page.waitForFunction(() => window.innerWidth === 176);

  await publish({ working: replacement.slice(0, 1) });
  await emit('overlay-region-hover', 'working');
  await panel.getByText('验证新会话').waitFor();
  const priorRegion = (await regions()).find(region => region.target === 'details');
  await emit('overlay-region-hover', 'completed');
  await panel.getByText('检查第二个回执').waitFor();
  const nextRegion = (await regions()).find(region => region.target === 'details');
  assert.equal(nextRegion.height, priorRegion.height);
  assert.ok(nextRegion.y > priorRegion.y, '列表等高时，锁定模式仍同步切换气泡造成的位置变化');

  await emit('overlay-region-hover', null);
  await page.clock.fastForward(500);
  await page.clock.fastForward(500);
  await publish({ sessions: null });
  await badge('working').waitFor({ state: 'hidden' });
  await publish({ working: replacement.slice(0, 1), completed: [detail('c0', '检查第二个回执', '运维', null, 'turn-2')] });
  await badge('working').waitFor();
  assert.equal(await panel.count(), 0, '恢复连接后，同一批会话不会再次弹出');
  await publish({ sessions: null, sourceRevision: 2, settings: { source: 'windows', distro: '', pollIntervalSecs: 60 } });
  await badge('working').waitFor({ state: 'hidden' });
  await publish({ working, completed });
  await badge('working').waitFor();
  assert.equal(await panel.count(), 0, '切换到 Windows 后，现有会话只建立新基准');

  await publish({ working: replacement });
  await panel.getByText('验证新会话').waitFor();
  await emit('overlay-region-hover', 'details');
  await publish({ completed: [detail('c0', '悬停时新增回执', '运维', null, 'turn-3')] });
  await panel.getByText('悬停时新增回执').waitFor();
  await page.clock.fastForward(8100);
  assert.equal(await panel.getByText('验证新会话').count(), 1);
  assert.equal(await panel.getByText('悬停时新增回执').count(), 1, '悬停过程中新增的分类在计时结束后仍保持展开');

  await page.setViewportSize({ width: 1040, height: 760 });
  await page.goto('http://127.0.0.1:1420/');
  const source = page.getByLabel('Codex 来源');
  await source.waitFor();
  const quota = { recordedAt: Math.floor(clockStart.getTime() / 1000), remainingPercent: 73, resetsAt: null, windowMinutes: 10080 };
  const points = [{ ...quota, recordedAt: quota.recordedAt - 60, segment: 'test' }, { ...quota, segment: 'test' }];
  const waitHistory = revision => page.waitForFunction(revision => window.__sessionFixture.hasPendingHistory(revision), revision, { polling: 50 });
  const completeHistory = revision => page.evaluate(([revision, points]) => window.__sessionFixture.completeHistory(revision, points), [revision, points]);
  await publish({ quota });
  await waitHistory(1);
  await completeHistory(1);
  const chart = page.getByRole('img', { name: /最近 24 小时/ });
  await chart.waitFor();
  await publish({ quota: { ...quota, recordedAt: quota.recordedAt + 1 } });
  await waitHistory(1);

  await source.selectOption('windows');
  await page.getByRole('button', { name: '保存设置' }).click();
  await chart.waitFor({ state: 'hidden' });
  await completeHistory(1);
  assert.equal(await chart.count(), 0, '切换后不显示旧请求返回的历史');
  await publish({ quota, historyRevision: 3, status: 'online' });
  await waitHistory(3);
  await completeHistory(3);
  await chart.waitFor();
  const saved = await page.evaluate(() => window.__sessionFixture.calls.find(call => call.command === 'update_settings'));
  assert.equal(saved.args.settings.source, 'windows', '没有 WSL 发行版时也可以保存 Windows 本机来源');

  assert.deepEqual(errors, []);
  await page.close();
}
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
    await productionPage.clock.install({ time: new Date('2026-11-14T00:00:00Z') });
    await productionPage.clock.pauseAt(new Date('2026-11-14T00:00:01Z'));
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
    assert.equal(await productionPage.locator('.ring-caption').textContent(), '3天0小时后重置');
    await productionPage.clock.fastForward(3600_000);
    await productionPage.getByText('2天23小时后重置', { exact: true }).waitFor();
    assert.equal(await productionPage.evaluate(() => window.__trayCspProbe.renders.length), probe.renders.length);
    await productionPage.clock.fastForward(71 * 3600_000);
    await productionPage.getByText('等待新的额度窗口', { exact: true }).waitFor();
    return { cspViolations: probe.violations.length, automaticRgbaBytes: probe.renders.at(-1).bytes, blobsRevoked: probe.blobsRevoked };
  } finally {
    await context.close();
    await new Promise(resolve => { server.close(resolve); server.closeAllConnections(); });
  }
}

const browser = await chromium.launch({ executablePath, headless: true, args: ['--no-sandbox'] });
try {
  await verifySessionDetails(browser);
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
  await page.getByRole('img', { name: '2 个本次启动后完成且回执未读的会话' }).waitFor();
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
