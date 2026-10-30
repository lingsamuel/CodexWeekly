use crate::overlay_geometry::{placement, WorkArea, OVERLAY_SIZE};
use crate::state::{broadcast, AppState};
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindowBuilder,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};

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
    .inner_size(OVERLAY_SIZE, OVERLAY_SIZE)
    .resizable(false)
    .minimizable(false)
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
    restore_overlay_geometry(app, false)?;
    crate::overlay_recovery::install(app, &overlay)?;
    overlay
        .set_ignore_cursor_events(preferences.locked)
        .map_err(|error| error.to_string())?;
    if preferences.overlay_visible {
        overlay.show().map_err(|error| error.to_string())?;
    }

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
    if visible {
        ensure_overlay_on_screen(app)?;
    }
    broadcast(app);
    Ok(())
}

// 启动、主动显示与系统恢复共用几何策略；系统回退不写入用户位置。
pub fn restore_overlay_geometry(app: &AppHandle, repaint: bool) -> Result<(), String> {
    let Some(overlay) = app.get_webview_window("overlay") else {
        return Ok(());
    };
    let preferences = app.state::<AppState>().lock()?.preferences.clone();
    let minimized = overlay.is_minimized().map_err(|error| error.to_string())?;
    if minimized {
        if !preferences.overlay_visible {
            return Ok(());
        }
        let hwnd = overlay.hwnd().map_err(|error| error.to_string())?.0;
        // 最小化窗口可仍被 IsWindowVisible 判为可见；改坐标无法恢复，也不能用会抢焦点的 SW_RESTORE。
        unsafe {
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        if overlay.is_minimized().map_err(|error| error.to_string())? {
            return Err("无法恢复已最小化的悬浮窗。".into());
        }
    }
    let repaint = repaint || minimized;
    let monitors = overlay
        .available_monitors()
        .map_err(|error| error.to_string())?;
    let primary = overlay
        .primary_monitor()
        .map_err(|error| error.to_string())?;
    let primary_index = monitors
        .iter()
        .position(|monitor| {
            primary
                .as_ref()
                .is_some_and(|primary| monitor.position() == primary.position())
        })
        .unwrap_or(0);
    let areas: Vec<_> = monitors
        .iter()
        .map(|monitor| {
            let area = monitor.work_area();
            WorkArea {
                x: area.position.x,
                y: area.position.y,
                width: area.size.width,
                height: area.size.height,
                scale: monitor.scale_factor(),
            }
        })
        .collect();
    let Some((point, (width, height))) =
        placement(preferences.overlay_position, &areas, primary_index)
    else {
        return Ok(());
    };
    let position = PhysicalPosition::new(point.x, point.y);
    let predicted_size = PhysicalSize::new(width, height);
    let moved = overlay
        .outer_position()
        .map_err(|error| error.to_string())?
        != position;
    if moved {
        overlay
            .set_position(position)
            .map_err(|error| error.to_string())?;
    }
    // 跨不同 DPI 的屏幕边缘时，以移动后 HWND 的真实 DPI 为准。
    let scale = overlay.scale_factor().map_err(|error| error.to_string())?;
    let logical = LogicalSize::new(OVERLAY_SIZE, OVERLAY_SIZE);
    let size: PhysicalSize<u32> = logical.to_physical(scale);
    let resized = overlay.inner_size().map_err(|error| error.to_string())? != size;
    if resized {
        overlay
            .set_size(logical)
            .map_err(|error| error.to_string())?;
    }
    // HWND 尺寸正确也不代表 WebView 客户区正确；显式恢复子窗口边界。
    if repaint || moved || resized || predicted_size != size {
        let size = overlay.inner_size().map_err(|error| error.to_string())?;
        overlay
            .as_ref()
            .set_bounds(tauri::Rect {
                position: PhysicalPosition::new(0, 0).into(),
                size: size.into(),
            })
            .map_err(|error| error.to_string())?;
        if repaint && preferences.overlay_visible {
            let handle = app.clone();
            overlay
                .with_webview(move |webview| {
                    let controller = webview.controller();
                    // 恢复合成表面而不重新加载页面，保留额度、气泡和前端状态。
                    let result = unsafe {
                        controller
                            .NotifyParentWindowPositionChanged()
                            .and_then(|_| controller.SetIsVisible(false))
                            .and_then(|_| controller.SetIsVisible(true))
                    };
                    if let Err(error) = result {
                        crate::overlay_recovery::report_error(&handle, &error.to_string());
                    }
                })
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub fn ensure_overlay_on_screen(app: &AppHandle) -> Result<(), String> {
    if crate::overlay_recovery::is_paused(app) {
        return Ok(());
    }
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if !crate::overlay_recovery::is_paused(&handle) {
            if let Err(error) = restore_overlay_geometry(&handle, false) {
                crate::overlay_recovery::report_error(&handle, &error);
            }
        }
    })
    .map_err(|error| error.to_string())
}
