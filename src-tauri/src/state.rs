use crate::source::CodexSource;
use limit_core::{
    activity::SessionCounts,
    quota::{ParsedQuota, QuotaSample},
    settings::{Preferences, Settings},
    store::Store,
};
use serde::Serialize;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{watch, Notify};

pub struct AppState {
    pub data: Mutex<AppData>,
    pub refresh: Notify,
    pub activity_refresh: Notify,
    pub source_changed: watch::Sender<()>,
    pub shutdown: Notify,
}

pub struct AppData {
    pub store: Store,
    pub preferences: Preferences,
    pub source: Option<CodexSource>,
    pub source_revision: u64,
    pub history_revision: u64,
    pub quota: Option<ParsedQuota>,
    pub status: &'static str,
    pub error: Option<String>,
    pub next_sample_starts_segment: bool,
    pub sessions: Option<SessionCounts>,
    pub session_error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub source_revision: u64,
    pub history_revision: u64,
    pub quota: Option<QuotaSample>,
    pub status: &'static str,
    pub error: Option<String>,
    pub locked: bool,
    pub overlay_visible: bool,
    #[serde(skip)]
    pub completion_sound: bool,
    pub settings: Settings,
    pub version: &'static str,
    pub sessions: Option<SessionCounts>,
    pub session_error: Option<String>,
}

impl AppState {
    pub fn lock(&self) -> Result<MutexGuard<'_, AppData>, String> {
        self.data
            .lock()
            .map_err(|_| "应用状态不可用，请退出后重新启动。".into())
    }

    /// 在持有应用状态锁时更新实际来源，采集和会话循环只使用这个已解析的实例。
    pub fn resolve_source(&self, data: &mut AppData) -> Result<CodexSource, String> {
        let result = CodexSource::resolve(&data.preferences.settings);
        if data.source.as_ref() != result.as_ref().ok() {
            data.source = result.as_ref().ok().cloned();
            data.source_revision += 1;
            data.history_revision += 1;
            data.quota = data.source.as_ref().and_then(|source| {
                data.store
                    .latest_for_source(source.cache_key())
                    .ok()
                    .flatten()
            });
            data.sessions = None;
            data.session_error = None;
            data.status = "connecting";
            data.error = None;
            data.next_sample_starts_segment = true;
            // 即使切回同一来源，前一次连接的结果也属于不同的序号。
            self.source_changed.send_replace(());
        }
        result
    }

    pub fn snapshot(&self) -> Result<AppSnapshot, String> {
        let data = self.lock()?;
        Ok(AppSnapshot {
            source_revision: data.source_revision,
            history_revision: data.history_revision,
            quota: data.quota.as_ref().map(|quota| quota.sample.clone()),
            status: data.status,
            error: data.error.clone(),
            locked: data.preferences.locked,
            overlay_visible: data.preferences.overlay_visible,
            completion_sound: data.preferences.completion_sound,
            settings: data.preferences.settings.clone(),
            version: env!("CARGO_PKG_VERSION"),
            sessions: data.sessions.clone(),
            session_error: data.session_error.clone(),
        })
    }
}

pub fn broadcast(app: &AppHandle) {
    let app = app.clone();
    let handle = app.clone();
    // 在 UI 线程读取并发布当前快照，后台采集和设置切换的通知不会乱序覆盖托盘。
    let _ = handle.run_on_main_thread(move || {
        if let Ok(snapshot) = app.state::<AppState>().snapshot() {
            crate::tray::sync(&app, &snapshot);
            let _ = app.emit("state-changed", snapshot);
        }
    });
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
