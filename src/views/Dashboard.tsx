import { useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import { demoMode, desktopMode, getHistory, listDistros, refresh, setLocked, setOverlayVisible, updateSettings } from '../lib/api';
import { dateTime, errorMessage, resetLabel } from '../lib/format';
import type { HistoryPoint, Settings } from '../lib/types';
import { useSnapshot } from '../lib/useSnapshot';
import { Icon } from '../components/Icon';
import { QuotaRing } from '../components/QuotaRing';
import { HistoryChart } from '../components/HistoryChart';

export function Dashboard() {
  const { snapshot, setSnapshot } = useSnapshot();
  const [hours, setHours] = useState(24);
  const [history, setHistory] = useState<HistoryPoint[]>([]);
  const [historyLoading, setHistoryLoading] = useState(true);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [distros, setDistros] = useState<string[]>([]);
  const [settings, setSettings] = useState<Settings>(snapshot.settings);
  const [busy, setBusy] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const available = desktopMode || demoMode;
  const dirty = settings.distro !== snapshot.settings.distro || settings.pollIntervalSecs !== snapshot.settings.pollIntervalSecs;
  const statusLabel = snapshot.status === 'online' ? '连接正常' : snapshot.status === 'connecting' ? '正在连接' : '连接已断开';

  useEffect(() => { setSettings(snapshot.settings); }, [snapshot.settings.distro, snapshot.settings.pollIntervalSecs]);
  useEffect(() => {
    if (!available) { setHistoryLoading(false); return; }
    let active = true;
    setHistoryLoading(true);
    setHistoryError(null);
    void getHistory(hours).then(points => { if (active) setHistory(points); }).catch(error => {
      if (active) { setHistory([]); setHistoryError(errorMessage(error)); }
    }).finally(() => { if (active) setHistoryLoading(false); });
    return () => { active = false; };
  }, [hours, snapshot, available]);
  useEffect(() => {
    if (!available) return;
    let active = true;
    void listDistros().then(values => { if (active) setDistros(values); }).catch(error => { if (active) setActionError(`无法读取 WSL 发行版：${errorMessage(error)}`); });
    return () => { active = false; };
  }, [available]);
  async function act(name: string, action: () => Promise<unknown>, success?: string) {
    if (busy) return;
    setBusy(name); setActionError(null); setNotice(null);
    try { await action(); if (success) setNotice(success); }
    catch (error) { setActionError(errorMessage(error)); }
    finally { setBusy(null); }
  }
  function save(event: FormEvent) {
    event.preventDefault();
    if (!Number.isInteger(settings.pollIntervalSecs) || settings.pollIntervalSecs < 30 || settings.pollIntervalSecs > 3600) { setActionError('刷新间隔应为 30～3600 秒的整数。'); return; }
    void act('save', async () => { setSnapshot(await updateSettings(settings)); }, '设置已保存，下次采样将使用新设置。');
  }
  const selectedDistroMissing = settings.distro && !distros.includes(settings.distro);
  return <main className="dashboard">
    <header className="app-header">
      <a className="brand" href={demoMode ? '?demo=1' : '?'} aria-label="Codex Weekly 首页"><span className="brand-mark"><Icon name="leaf" size={22} /></span><span>Codex <span className="brand-light">Weekly</span></span></a>
      <div className="header-right">{demoMode && <span className="demo-badge">演示模式</span>}<span className={`connection-pill status-${snapshot.status}`}><span className="status-dot" />{statusLabel}</span></div>
    </header>

    <section className="page-heading"><div><p className="eyebrow">YOUR WEEK, IN VIEW</p><h1>本周额度概览<span className="heading-dot">.</span></h1><p className="subtitle">让每一次专注，都心中有数。</p></div><button className="button button-secondary refresh-button" disabled={!available || busy !== null || snapshot.status === 'connecting'} onClick={() => void act('refresh', refresh, '已请求刷新，连接完成后会自动更新。')}><Icon name="refresh" />{busy === 'refresh' ? '正在刷新' : '立即刷新'}</button></section>

    {(!available || snapshot.error || actionError) && <div className="alert" role="alert"><Icon name="info" /><div><strong>{!available ? '请在桌面应用中打开' : actionError ? '操作未完成' : '暂时无法同步额度'}</strong><p>{actionError ?? snapshot.error ?? '浏览器无法访问本地 WSL。请启动 Codex Weekly 桌面程序。'}</p>{available && snapshot.error && <p>请确认所选 WSL 发行版可启动，且其中的 Codex 已登录，再点击“立即刷新”。{snapshot.quota ? ' 当前显示上次成功同步的读数。' : ''}</p>}</div></div>}
    {notice && <div className="notice" role="status"><Icon name="check" size={15} />{notice}<button aria-label="关闭提示" onClick={() => setNotice(null)}>×</button></div>}

    <section className="overview-card" aria-label="本周额度与同步摘要">
      <div className="quota-summary"><QuotaRing value={snapshot.quota?.remainingPercent} stale={snapshot.status !== 'online'} /><div><span className="card-eyebrow">WEEKLY LIMIT</span><h2>本周剩余额度</h2><p>{snapshot.quota ? '7 天额度窗口' : '等待第一次成功同步'}</p><span className={`soft-label ${snapshot.status !== 'online' ? 'soft-label--muted' : ''}`}><span className="status-dot" />{snapshot.status === 'online' ? '已与 Codex 同步' : snapshot.quota ? '上次有效读数' : '尚无有效读数'}</span></div></div>
      <div className="overview-stat"><span className="stat-icon"><Icon name="clock" /></span><span className="stat-label">下一次重置</span><strong>{resetLabel(snapshot.quota?.resetsAt)}</strong><span className="stat-note">以 Codex 返回时间为准</span></div>
      <div className="overview-stat"><span className="stat-icon"><Icon name="refresh" /></span><span className="stat-label">最近同步</span><strong>{dateTime(snapshot.quota?.recordedAt, true)}</strong><span className="stat-note">每 {snapshot.settings.pollIntervalSecs} 秒自动检查</span></div>
    </section>

    <section className="panel trend-panel" aria-label="额度变化趋势">
      <div className="panel-heading"><div className="section-title"><h2>额度趋势</h2><span className="legend"><span />剩余额度</span>{historyLoading && history.length > 0 && <span className="loading-label" role="status">更新中</span>}</div><div className="range-control" role="group" aria-label="历史时间范围">{[{ hours: 24, label: '24 小时' }, { hours: 168, label: '7 天' }, { hours: 720, label: '30 天' }].map(range => <button key={range.hours} aria-pressed={hours === range.hours} onClick={() => { setHours(range.hours); setHistory([]); }}>{range.label}</button>)}</div></div>
      {historyError ? <div className="chart-empty" role="alert"><Icon name="info" size={24} /><strong>历史记录暂时不可用</strong><p>{historyError}</p></div> : <HistoryChart points={history} hours={hours} loading={historyLoading} />}
      <div className="chart-footer"><span><span className="tiny-dot" />{demoMode ? '当前展示演示数据' : '仅记录真实采样 · 重置与断线自动分段'}</span><span>时间均为本地时间</span></div>
    </section>

    <section className="panel settings-panel" aria-labelledby="settings-title">
      <div className="settings-intro"><span className="settings-icon"><Icon name="settings" size={19} /></span><div><h2 id="settings-title">连接设置</h2><p>复用 WSL 中现有的 Codex 登录</p></div></div>
      <form className="settings-form" onSubmit={save}>
        <label className="field distro-field"><span>WSL 发行版</span><select value={settings.distro} disabled={!available || busy === 'save'} onChange={event => setSettings(current => ({ ...current, distro: event.target.value }))}><option value="">系统默认发行版</option>{selectedDistroMissing && <option value={settings.distro}>{settings.distro}（当前配置）</option>}{distros.map(distro => <option key={distro} value={distro}>{distro}</option>)}</select></label>
        <label className="field interval-field"><span>刷新间隔</span><div className="input-with-unit"><input type="number" min="30" max="3600" step="1" required value={settings.pollIntervalSecs || ''} disabled={!available || busy === 'save'} onChange={event => setSettings(current => ({ ...current, pollIntervalSecs: event.target.valueAsNumber }))} /><span>秒</span></div></label>
        <button className="button button-primary save-button" disabled={!available || !dirty || busy !== null} type="submit">{busy === 'save' ? '保存中…' : '保存设置'}<Icon name="arrow" size={16} /></button>
      </form>
    </section>

    <footer className="app-footer"><div className="overlay-actions"><span className="footer-label">悬浮窗</span><button disabled={!available || busy !== null} aria-pressed={snapshot.locked} onClick={() => void act('lock', async () => { setSnapshot(await setLocked(!snapshot.locked)); }, snapshot.locked ? '悬浮窗已解锁，可以拖动。' : '悬浮窗已锁定并穿透鼠标，可从托盘解锁。')}><Icon name={snapshot.locked ? 'lock' : 'unlock'} size={15} />{snapshot.locked ? '已锁定' : '未锁定'}</button><button disabled={!available || busy !== null} aria-pressed={snapshot.overlayVisible} onClick={() => void act('visible', async () => { setSnapshot(await setOverlayVisible(!snapshot.overlayVisible)); })}><Icon name={snapshot.overlayVisible ? 'eye' : 'hidden'} size={16} />{snapshot.overlayVisible ? '已显示' : '已隐藏'}</button></div><span className="version">本地记录，安心专注 <span>v{snapshot.version}</span></span></footer>
  </main>;
}
