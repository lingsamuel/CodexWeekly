use crate::state::{broadcast, AppState};
use limit_core::settings::OverlayPosition;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, WindowEvent};

// 仅按采样更新 SVG，软件合成可减少空闲 GPU 资源。
// 同一 WebView2 环境内的窗口须保持一致参数，避免创建选项冲突。
const RENDERER_ARGS: &str = "--disable-gpu --disable-gpu-compositing";

pub fn open_dashboard(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("dashboard") {
        window.unminimize().map_err(|error| error.to_string())?;
        window.show().map_err(|error| error.to_string())?;
        return window.set_focus().map_err(|error| error.to_string());
    }
    WebviewWindowBuilder::new(app, "dashboard", WebviewUrl::App("index.html".into()))
        .title("Codex Weekly · 额度与趋势")
        .additional_browser_args(RENDERER_ARGS)
        .inner_size(1080.0, 780.0)
        .min_inner_size(800.0, 600.0)
        .center()
        .build()
        .map_err(|error| error.to_string())?;
    // 默认 CloseRequested 销毁主视图；由托盘继续维持应用生命周期。
    Ok(())
}

pub fn create_overlay(app: &AppHandle) -> Result<(), String> {
    let preferences = app.state::<AppState>().lock()?.preferences.clone();
    let overlay = WebviewWindowBuilder::new(
        app,
        "overlay",
        WebviewUrl::App("index.html?view=overlay".into()),
    )
    .title("Codex Weekly Overlay")
    .additional_browser_args(RENDERER_ARGS)
    .inner_size(176.0, 176.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .focusable(!preferences.locked)
    .visible(false)
    .build()
    .map_err(|error| error.to_string())?;
    let saved = preferences.overlay_position;
    let monitors = overlay
        .available_monitors()
        .map_err(|error| error.to_string())?;
    let size = overlay.outer_size().map_err(|error| error.to_string())?;
    let valid = saved.is_some_and(|point| {
        monitors.iter().any(|monitor| {
            let area = monitor.work_area();
            point.x >= area.position.x
                && point.y >= area.position.y
                && i64::from(point.x) + i64::from(size.width)
                    <= i64::from(area.position.x) + i64::from(area.size.width)
                && i64::from(point.y) + i64::from(size.height)
                    <= i64::from(area.position.y) + i64::from(area.size.height)
        })
    });
    let position = if valid {
        saved
    } else {
        overlay.primary_monitor().ok().flatten().map(|monitor| {
            let area = monitor.work_area();
            OverlayPosition {
                x: area.position.x + area.size.width as i32 - size.width as i32 - 32,
                y: area.position.y + 40,
            }
        })
    };
    if let Some(point) = position {
        overlay
            .set_position(PhysicalPosition::new(point.x, point.y))
            .map_err(|error| error.to_string())?;
    }
    overlay
        .set_ignore_cursor_events(preferences.locked)
        .map_err(|error| error.to_string())?;
    if preferences.overlay_visible {
        overlay.show().map_err(|error| error.to_string())?;
    }
    let handle = app.clone();
    overlay.on_window_event(move |event| {
        if let WindowEvent::Moved(position) = event {
            let state = handle.state::<AppState>();
            if let Ok(mut data) = state.lock() {
                data.preferences.overlay_position = Some(OverlayPosition {
                    x: position.x,
                    y: position.y,
                });
            }
            // 拖动可高频触发 Moved；合并写盘，避免每个鼠标事件都执行 SQLite I/O。
            if !state
                .position_save_pending
                .swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                let app = handle.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                    let state = app.state::<AppState>();
                    if let Ok(mut data) = state.lock() {
                        if data.store.save_preferences(&data.preferences).is_err() {
                            data.error = Some("悬浮窗位置保存失败，请检查应用数据目录。".into());
                        }
                    }
                    state
                        .position_save_pending
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                });
            }
        }
    });
    Ok(())
}

pub fn set_locked(app: &AppHandle, locked: bool) -> Result<(), String> {
    let overlay = app
        .get_webview_window("overlay")
        .ok_or("悬浮窗尚未创建。")?;
    let state = app.state::<AppState>();
    let mut data = state.lock()?;
    let previous = data.preferences.locked;
    overlay
        .set_ignore_cursor_events(locked)
        .map_err(|error| error.to_string())?;
    if let Err(error) = overlay.set_focusable(!locked) {
        let _ = overlay.set_ignore_cursor_events(previous);
        return Err(error.to_string());
    }
    let mut preferences = data.preferences.clone();
    preferences.locked = locked;
    if let Err(error) = data.store.save_preferences(&preferences) {
        let _ = overlay.set_ignore_cursor_events(previous);
        let _ = overlay.set_focusable(!previous);
        return Err(format!("无法保存锁定状态：{error}"));
    }
    data.preferences = preferences;
    drop(data);
    broadcast(app);
    Ok(())
}

pub fn set_visible(app: &AppHandle, visible: bool) -> Result<(), String> {
    if visible {
        ensure_overlay_on_screen(app)?;
    }
    let overlay = app
        .get_webview_window("overlay")
        .ok_or("悬浮窗尚未创建。")?;
    let state = app.state::<AppState>();
    let mut data = state.lock()?;
    let previous = data.preferences.overlay_visible;
    if visible {
        overlay.show()
    } else {
        overlay.hide()
    }
    .map_err(|error| error.to_string())?;
    let mut preferences = data.preferences.clone();
    preferences.overlay_visible = visible;
    if let Err(error) = data.store.save_preferences(&preferences) {
        let _ = if previous {
            overlay.show()
        } else {
            overlay.hide()
        };
        return Err(format!("无法保存悬浮窗可见状态：{error}"));
    }
    data.preferences = preferences;
    drop(data);
    broadcast(app);
    Ok(())
}

pub fn ensure_overlay_on_screen(app: &AppHandle) -> Result<(), String> {
    let Some(overlay) = app.get_webview_window("overlay") else {
        return Ok(());
    };
    let position = overlay
        .outer_position()
        .map_err(|error| error.to_string())?;
    let size = overlay.outer_size().map_err(|error| error.to_string())?;
    let monitors = overlay
        .available_monitors()
        .map_err(|error| error.to_string())?;
    let reachable = monitors.iter().any(|monitor| {
        let area = monitor.work_area();
        // 保留至少 48 个物理像素可见；允许用户将圆环跨放在两个显示器之间。
        i64::from(position.x) + i64::from(size.width) >= i64::from(area.position.x) + 48
            && i64::from(position.y) + i64::from(size.height) >= i64::from(area.position.y) + 48
            && i64::from(position.x) + 48 <= i64::from(area.position.x) + i64::from(area.size.width)
            && i64::from(position.y) + 48
                <= i64::from(area.position.y) + i64::from(area.size.height)
    });
    if !reachable {
        if let Some(monitor) = overlay
            .primary_monitor()
            .map_err(|error| error.to_string())?
        {
            let area = monitor.work_area();
            overlay
                .set_position(PhysicalPosition::new(
                    area.position.x + area.size.width as i32 - size.width as i32 - 32,
                    area.position.y + 40,
                ))
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}
