use crate::{
    overlay_geometry::DragState,
    state::{broadcast, AppState},
};
use limit_core::settings::OverlayPosition;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex,
};
use tauri::{AppHandle, Manager, WebviewWindow};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    System::{
        Power::{
            RegisterPowerSettingNotification, UnregisterPowerSettingNotification, HPOWERNOTIFY,
            POWERBROADCAST_SETTING,
        },
        SystemServices::GUID_CONSOLE_DISPLAY_STATE,
    },
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GetWindowRect, DEVICE_NOTIFY_WINDOW_HANDLE, PBT_APMRESUMEAUTOMATIC,
            PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, PBT_POWERSETTINGCHANGE, SIZE_MINIMIZED,
            WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_NCDESTROY,
            WM_POWERBROADCAST, WM_SETTINGCHANGE, WM_SIZE,
        },
    },
};

#[derive(Default)]
struct RecoveryState {
    generation: AtomicU64,
    display_off: AtomicBool,
    drag: Mutex<DragState>,
}

struct HookData {
    app: AppHandle,
    power: HPOWERNOTIFY,
}

pub fn install(app: &AppHandle, overlay: &WebviewWindow) -> Result<(), String> {
    app.manage(RecoveryState::default());
    let hwnd = overlay.hwnd().map_err(|error| error.to_string())?.0;
    // 注册与 subclass 均在创建窗口的 UI 线程执行；销毁 HWND 时配对释放。
    unsafe {
        let power = RegisterPowerSettingNotification(
            hwnd,
            &GUID_CONSOLE_DISPLAY_STATE,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        );
        if power == 0 {
            return Err(format!(
                "无法监听屏幕电源状态：{}",
                std::io::Error::last_os_error()
            ));
        }
        let data = Box::into_raw(Box::new(HookData {
            app: app.clone(),
            power,
        }));
        if SetWindowSubclass(hwnd, Some(window_proc), 1, data as usize) == 0 {
            UnregisterPowerSettingNotification(power);
            drop(Box::from_raw(data));
            return Err("无法安装悬浮窗恢复处理。".into());
        }
    }
    Ok(())
}

pub fn is_paused(app: &AppHandle) -> bool {
    app.try_state::<RecoveryState>().is_some_and(|state| {
        state.display_off.load(Ordering::Relaxed)
            || state.drag.lock().map(|drag| drag.busy()).unwrap_or(true)
    })
}

fn request(app: &AppHandle) {
    let state = app.state::<RecoveryState>();
    let generation = state.generation.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // 显示器重连会连续发送电源、工作区和 DPI 通知，只应用最后一轮布局。
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if handle
                .state::<RecoveryState>()
                .generation
                .load(Ordering::Relaxed)
                != generation
                || is_paused(&handle)
            {
                return;
            }
            if let Err(error) = crate::windows::restore_overlay_geometry(&handle, true) {
                report_error(&handle, &error);
            }
        });
    });
}

pub fn report_error(app: &AppHandle, error: &str) {
    if let Ok(mut data) = app.state::<AppState>().lock() {
        data.error = Some(format!("悬浮窗恢复失败：{error}"));
    }
    broadcast(app);
}

fn save_drag(app: &AppHandle, position: OverlayPosition, revision: u64) {
    let app = app.clone();
    // 不在同步 Windows 消息中执行数据库 I/O 或跨进程 WebView 调用。
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let result = (|| {
            let mut data = state.lock()?;
            let recovery = app.state::<RecoveryState>();
            let current = recovery
                .drag
                .lock()
                .map(|drag| drag.is_current(revision))
                .unwrap_or(false);
            if data.preferences.locked || !current {
                return Ok(());
            }
            let mut preferences = data.preferences.clone();
            preferences.overlay_position = Some(position);
            data.store
                .save_preferences(&preferences)
                .map_err(|error| error.to_string())?;
            data.preferences = preferences;
            Ok::<_, String>(())
        })();
        if let Ok(mut drag) = app.state::<RecoveryState>().drag.lock() {
            drag.saved();
        }
        request(&app);
        if let Err(error) = result {
            report_error(&app, &error);
        }
    });
}

// data 由 install 创建，生命周期严格绑定 HWND；只在 WM_NCDESTROY 释放一次。
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    let hook = &*(data as *const HookData);
    if message == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(window_proc), id);
        UnregisterPowerSettingNotification(hook.power);
        drop(Box::from_raw(data as *mut HookData));
        return DefSubclassProc(hwnd, message, wparam, lparam);
    }
    let state = hook.app.state::<RecoveryState>();
    match message {
        WM_SIZE if wparam == SIZE_MINIMIZED as usize => {
            // 系统可能在拖动结束前最小化，不能让 iconic 坐标进入用户位置或一直阻塞恢复。
            if let Ok(mut drag) = state.drag.lock() {
                drag.interrupt();
            }
            request(&hook.app);
        }
        WM_ENTERSIZEMOVE => {
            state.generation.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut drag) = state.drag.lock() {
                drag.begin();
            }
        }
        WM_EXITSIZEMOVE => {
            let mut rect = RECT::default();
            let valid = GetWindowRect(hwnd, &mut rect) != 0;
            let completed = state.drag.lock().ok().and_then(|mut drag| {
                if valid {
                    drag.finish()
                } else {
                    drag.interrupt();
                    None
                }
            });
            if let Some(revision) = completed {
                save_drag(
                    &hook.app,
                    OverlayPosition {
                        x: rect.left,
                        y: rect.top,
                    },
                    revision,
                );
            }
        }
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_DPICHANGED => {
            // 用户跨 DPI 显示器拖动时，保留拖动生命周期并由 Windows 正常缩放。
            let dragging = state.drag.lock().map(|drag| drag.active()).unwrap_or(true);
            if message != WM_DPICHANGED || !dragging {
                if let Ok(mut drag) = state.drag.lock() {
                    drag.interrupt();
                }
                request(&hook.app);
            }
        }
        WM_POWERBROADCAST => {
            let mut changed = true;
            match wparam as u32 {
                PBT_APMSUSPEND => state.display_off.store(true, Ordering::Relaxed),
                PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => {
                    state.display_off.store(false, Ordering::Relaxed)
                }
                PBT_POWERSETTINGCHANGE if lparam != 0 => {
                    // POWERBROADCAST_SETTING 的尾部是变长 DWORD；先校验再按非对齐值读取。
                    let setting = lparam as *const POWERBROADCAST_SETTING;
                    let guid = (*setting).PowerSetting;
                    if guid.data1 == GUID_CONSOLE_DISPLAY_STATE.data1
                        && guid.data2 == GUID_CONSOLE_DISPLAY_STATE.data2
                        && guid.data3 == GUID_CONSOLE_DISPLAY_STATE.data3
                        && guid.data4 == GUID_CONSOLE_DISPLAY_STATE.data4
                        && (*setting).DataLength == 4
                    {
                        let display =
                            std::ptr::read_unaligned((*setting).Data.as_ptr().cast::<u32>());
                        state.display_off.store(display == 0, Ordering::Relaxed);
                    } else {
                        changed = false;
                    }
                }
                _ => changed = false,
            }
            if changed {
                if let Ok(mut drag) = state.drag.lock() {
                    drag.interrupt();
                }
                request(&hook.app);
            }
        }
        _ => {}
    }
    DefSubclassProc(hwnd, message, wparam, lparam)
}
