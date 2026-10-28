use crate::{
    state::{AppSnapshot, AppState},
    windows,
};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

pub struct TrayControls {
    locked: CheckMenuItem<tauri::Wry>,
    visible: CheckMenuItem<tauri::Wry>,
}

pub fn create(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = app.state::<AppState>().snapshot()?;
    let open = MenuItem::with_id(app, "open", "打开额度面板", true, None::<&str>)?;
    let locked = CheckMenuItem::with_id(
        app,
        "locked",
        "锁定悬浮窗（鼠标穿透）",
        true,
        snapshot.locked,
        None::<&str>,
    )?;
    let visible = CheckMenuItem::with_id(
        app,
        "visible",
        "显示悬浮窗",
        true,
        snapshot.overlay_visible,
        None::<&str>,
    )?;
    let refresh = MenuItem::with_id(app, "refresh", "立即刷新", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 Codex Weekly", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&open, &separator, &locked, &visible, &refresh, &quit],
    )?;
    TrayIconBuilder::with_id("main")
        .icon(tauri::image::Image::from_bytes(include_bytes!(
            "../icons/icon.png"
        ))?)
        .tooltip("Codex Weekly · 正在连接 WSL")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                report(
                    tray.app_handle(),
                    windows::open_dashboard(tray.app_handle()),
                );
            }
        })
        .on_menu_event(|app, event| {
            let result = match event.id.as_ref() {
                "open" => windows::open_dashboard(app),
                "locked" => app
                    .state::<AppState>()
                    .snapshot()
                    .and_then(|snapshot| windows::set_locked(app, !snapshot.locked)),
                "visible" => app
                    .state::<AppState>()
                    .snapshot()
                    .and_then(|snapshot| windows::set_visible(app, !snapshot.overlay_visible)),
                "refresh" => {
                    app.state::<AppState>().refresh.notify_one();
                    Ok(())
                }
                "quit" => {
                    app.exit(0);
                    Ok(())
                }
                _ => Ok(()),
            };
            report(app, result);
        })
        .build(app)?;
    app.manage(TrayControls { locked, visible });
    Ok(())
}

fn report(app: &AppHandle, result: Result<(), String>) {
    if let Err(error) = result {
        if let Ok(mut data) = app.state::<AppState>().lock() {
            data.error = Some(error);
        }
        crate::state::broadcast(app);
    }
}

pub fn sync(app: &AppHandle, snapshot: &AppSnapshot) {
    if let Some(controls) = app.try_state::<TrayControls>() {
        let _ = controls.locked.set_checked(snapshot.locked);
        let _ = controls.visible.set_checked(snapshot.overlay_visible);
    }
    if let Some(tray) = app.tray_by_id("main") {
        let tooltip = match &snapshot.quota {
            Some(quota) => format!(
                "Codex Weekly · 剩余 {:.0}%{}",
                quota.remaining_percent,
                if snapshot.status == "online" {
                    ""
                } else {
                    " · 数据待更新"
                }
            ),
            None => "Codex Weekly · 等待额度数据".into(),
        };
        let _ = tray.set_tooltip(Some(tooltip));
    }
}
