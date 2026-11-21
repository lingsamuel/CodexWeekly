#![cfg(windows)]

mod activity;
mod collector;
mod commands;
mod overlay_geometry;
mod overlay_input;
mod overlay_recovery;
mod source;
mod state;
mod tray;
mod windows;

use limit_core::{
    settings::{Settings, SourceKind},
    store::Store,
};
use source::CodexSource;
use state::{AppData, AppState};
use std::sync::Mutex;
use tauri::{Manager, RunEvent};

pub fn run() {
    let started_at = state::now();
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if let Some(path) = argument(&arguments, "--diagnose-sessions") {
        let result = tauri::async_runtime::block_on(async {
            let source = diagnostic_source(&arguments)?;
            tokio::time::timeout(std::time::Duration::from_secs(35), async {
                activity::ActivityClient::connect(&source, started_at)
                    .await?
                    .fetch_counts()
                    .await
            })
            .await
            .unwrap_or_else(|_| Err("读取 Codex 会话统计超时。".into()))
        });
        let report = match &result {
            Ok(counts) => {
                serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "startedAt": started_at,
                    "ok": true,
                    "sessions": {
                        "working": counts.working,
                        "unreadCompleted": counts.unread_completed,
                        "observedAt": counts.observed_at,
                    },
                })
            }
            Err(error) => {
                serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"startedAt":started_at,"ok":false,"error":error})
            }
        };
        if std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).is_err() {
            std::process::exit(2);
        }
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    if let Some(path) = argument(&arguments, "--diagnose") {
        let result = tauri::async_runtime::block_on(async {
            collector::fetch(&diagnostic_source(&arguments)?).await
        });
        let output = match &result {
            Ok(quota) => {
                serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"ok":true,"quota":quota.sample})
            }
            Err(error) => {
                serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"ok":false,"error":error})
            }
        };
        if std::fs::write(path, serde_json::to_vec_pretty(&output).unwrap()).is_err() {
            std::process::exit(2);
        }
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = windows::open_dashboard(app);
        }))
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_history,
            commands::update_settings,
            commands::refresh,
            commands::set_locked,
            commands::set_overlay_visible,
            commands::begin_drag,
            commands::set_overlay_details,
            commands::set_overlay_hit_regions,
            commands::open_dashboard,
            commands::list_distros,
            commands::set_tray_image
        ])
        .setup(move |app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let store = Store::open(&directory.join("weekly.sqlite3"))?;
            let preferences = store.preferences()?;
            preferences.settings.validate()?;
            let first_run = store.latest()?.is_none();
            let state = AppState {
                data: Mutex::new(AppData {
                    store,
                    preferences,
                    source: None,
                    source_revision: 0,
                    history_revision: 0,
                    quota: None,
                    status: "connecting",
                    error: None,
                    next_sample_starts_segment: true,
                    sessions: None,
                    session_error: None,
                }),
                refresh: tokio::sync::Notify::new(),
                activity_refresh: tokio::sync::Notify::new(),
                source_changed: tokio::sync::watch::channel(()).0,
                shutdown: tokio::sync::Notify::new(),
            };
            {
                let mut data = state.lock()?;
                let _ = state.resolve_source(&mut data);
            }
            app.manage(state);
            tray::create(app.handle())?;
            windows::create_overlay(app.handle())?;
            if first_run {
                windows::open_dashboard(app.handle())?;
            }
            collector::start(app.handle().clone());
            activity::start(app.handle().clone(), started_at);
            Ok(())
        })
        .build(tauri::generate_context!())
        .unwrap_or_else(|error| {
            startup_failure(&format!(
                "无法启动 Codex Weekly。\n请检查 WebView2 运行时和应用数据目录。\n\n{error}"
            ))
        });
    app.run(|app, event| {
        // 关闭主面板后保留托盘和悬浮窗；托盘显式退出仍正常结束。
        if let RunEvent::ExitRequested { code, api, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            } else {
                app.state::<AppState>().shutdown.notify_waiters();
            }
        }
    });
}

fn argument<'a>(arguments: &'a [String], key: &str) -> Option<&'a str> {
    arguments
        .windows(2)
        .find(|args| args[0] == key)
        .map(|args| args[1].as_str())
}

fn diagnostic_source(arguments: &[String]) -> Result<CodexSource, String> {
    let source = match argument(arguments, "--source") {
        None | Some("wsl") => SourceKind::Wsl,
        Some("windows") => SourceKind::Windows,
        Some(_) => return Err("诊断的 --source 应为 windows 或 wsl。".into()),
    };
    let settings = Settings {
        source,
        distro: argument(arguments, "--distro")
            .unwrap_or_default()
            .to_owned(),
        ..Settings::default()
    };
    settings.validate()?;
    CodexSource::resolve(&settings)
}

fn startup_failure(message: &str) -> ! {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let message: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "Codex Weekly".encode_utf16().chain(Some(0)).collect();
    // GUI 子系统没有可见终端，启动失败时提供可操作的错误，避免静默退出。
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
    std::process::exit(1)
}
