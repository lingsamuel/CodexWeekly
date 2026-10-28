use limit_core::{
    activity::SessionCounts,
    quota::{ParsedQuota, QuotaSample},
    settings::{Preferences, Settings},
    store::Store,
};
use serde::Serialize;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

pub struct AppState {
    pub data: Mutex<AppData>,
    pub refresh: Notify,
    pub activity_refresh: Notify,
    pub shutdown: Notify,
    pub position_save_pending: std::sync::atomic::AtomicBool,
}

pub struct AppData {
    pub store: Store,
    pub preferences: Preferences,
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
    pub quota: Option<QuotaSample>,
    pub status: &'static str,
    pub error: Option<String>,
    pub locked: bool,
    pub overlay_visible: bool,
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

    pub fn snapshot(&self) -> Result<AppSnapshot, String> {
        let data = self.lock()?;
        Ok(AppSnapshot {
            quota: data.quota.as_ref().map(|quota| quota.sample.clone()),
            status: data.status,
            error: data.error.clone(),
            locked: data.preferences.locked,
            overlay_visible: data.preferences.overlay_visible,
            settings: data.preferences.settings.clone(),
            version: env!("CARGO_PKG_VERSION"),
            sessions: data.sessions.clone(),
            session_error: data.session_error.clone(),
        })
    }
}

pub fn broadcast(app: &AppHandle) {
    if let Ok(snapshot) = app.state::<AppState>().snapshot() {
        crate::tray::sync(app, &snapshot);
        let _ = app.emit("state-changed", snapshot);
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
