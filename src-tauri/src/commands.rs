use crate::{
    overlay_input::HitRegion,
    source,
    state::{broadcast, now, AppSnapshot, AppState},
    windows,
};
use limit_core::{settings::Settings, store::HistoryPoint};
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    state.snapshot()
}

#[tauri::command]
pub fn get_history(
    hours: u32,
    revision: u64,
    state: State<'_, AppState>,
) -> Result<Vec<HistoryPoint>, String> {
    if ![24, 168, 720].contains(&hours) {
        return Err("请选择 24 小时、7 天或 30 天。".into());
    }
    let data = state.lock()?;
    if data.history_revision != revision {
        return Ok(Vec::new());
    }
    let Some(quota) = &data.quota else {
        return Ok(Vec::new());
    };
    data.store
        .history(
            &quota.account_key,
            now() - i64::from(hours) * 3600,
            now(),
            1200,
        )
        .map_err(|_| "无法读取本地历史，请检查应用数据目录。".into())
}

#[tauri::command]
pub fn update_settings(
    settings: Settings,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppSnapshot, String> {
    settings.validate()?;
    let mut data = state.lock()?;
    let mut preferences = data.preferences.clone();
    preferences.settings = settings;
    data.store
        .save_preferences(&preferences)
        .map_err(|_| "设置保存失败，请检查应用数据目录。".to_string())?;
    data.preferences = preferences;
    let previous = data.source_revision;
    match state.resolve_source(&mut data) {
        Ok(_) => {
            data.status = "connecting";
            data.error = None;
        }
        Err(error) => {
            data.status = "offline";
            data.error = Some(error);
        }
    }
    data.next_sample_starts_segment = true;
    let changed = previous != data.source_revision;
    drop(data);
    if !changed {
        state.refresh.notify_one();
        state.activity_refresh.notify_one();
    }
    broadcast(&app);
    state.snapshot()
}

#[tauri::command]
pub fn refresh(state: State<'_, AppState>) {
    state.refresh.notify_one();
}

#[tauri::command]
pub fn set_locked(
    locked: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppSnapshot, String> {
    windows::set_locked(&app, locked)?;
    state.snapshot()
}

#[tauri::command]
pub fn set_overlay_visible(
    visible: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppSnapshot, String> {
    windows::set_visible(&app, visible)?;
    state.snapshot()
}

#[tauri::command]
pub fn begin_drag(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if state.lock()?.preferences.locked {
        return Ok(());
    }
    app.get_webview_window("overlay")
        .ok_or("悬浮窗尚未创建。")?
        .start_dragging()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_overlay_details(
    window: tauri::WebviewWindow,
    expanded: bool,
) -> Result<&'static str, String> {
    if window.label() != "overlay" {
        return Err("只能从悬浮窗设置会话详情。".into());
    }
    windows::set_details(window.app_handle(), expanded)
}

#[tauri::command]
pub fn set_overlay_hit_regions(
    window: tauri::WebviewWindow,
    regions: Vec<HitRegion>,
) -> Result<(), String> {
    if window.label() != "overlay" {
        return Err("只能从悬浮窗设置鼠标区域。".into());
    }
    crate::overlay_input::set_regions(regions)
}

#[tauri::command]
pub fn open_dashboard(app: AppHandle) -> Result<(), String> {
    windows::open_dashboard(&app)
}

#[tauri::command]
pub async fn list_distros() -> Result<Vec<String>, String> {
    source::list_distros()
}

#[tauri::command]
pub fn set_tray_image(
    rgba: Vec<u8>,
    revision: u64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if rgba.len() != 64 * 64 * 4 {
        return Err("托盘图像必须为 64×64 RGBA。".into());
    }
    if state.lock()?.history_revision != revision {
        return Ok(());
    }
    let tray = app.tray_by_id("main").ok_or("托盘尚未创建。")?;
    tray.set_icon(Some(tauri::image::Image::new_owned(rgba, 64, 64)))
        .map_err(|_| "更新托盘余量图标失败。".into())
}
