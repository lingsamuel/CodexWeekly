use crate::state::{broadcast, now, AppState};
use limit_core::{
    quota::{parse_quota, ParsedQuota},
    settings::Settings,
};
use serde_json::Value;
use std::{process::Stdio, time::Duration};
use tauri::{AppHandle, Manager};
use tokio::{io::AsyncReadExt, process::Command};

const BRIDGE: &str = include_str!("../../scripts/wsl_bridge.py");
const CREATE_NO_WINDOW: u32 = 0x08000000;

fn wsl_command(settings: &Settings) -> Command {
    let mut command = Command::new("wsl.exe");
    command.creation_flags(CREATE_NO_WINDOW).kill_on_drop(true);
    if !settings.distro.is_empty() {
        command.args(["--distribution", &settings.distro]);
    }
    command.args(["--cd", "~"]);
    command
}

pub async fn fetch(settings: &Settings) -> Result<ParsedQuota, String> {
    let mut child = wsl_command(settings)
        .args([
            "--exec",
            "python3",
            "-u",
            "-c",
            BRIDGE,
            "--watch-parent",
            "--restore-shell-environment",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "无法启动 wsl.exe，请确认 Windows 已启用 WSL。".to_string())?;
    // 保持 stdin 管道直到采集结束；退出/取消时 EOF 会通知 WSL 桥接回收进程组。
    let _parent_pipe = child.stdin.take();
    let stdout = child.stdout.take().ok_or("无法读取 WSL 输出。")?;
    let mut bytes = Vec::new();
    let status = tokio::time::timeout(Duration::from_secs(30), async {
        stdout.take(2 * 1024 * 1024).read_to_end(&mut bytes).await?;
        child.wait().await
    })
    .await
    .map_err(|_| "WSL 启动或读取超时，请检查该发行版能否正常启动。".to_string())?
    .map_err(|_| "WSL 连接已断开，请稍后重试。".to_string())?;
    let response: Value = serde_json::from_slice(&bytes).map_err(|_| {
        "WSL 未能启动额度桥接。请确认所选发行版已安装 Python 3 和 Codex，并能正常运行。".to_string()
    })?;
    if let Some(error) = response.get("error").and_then(Value::as_str) {
        return Err(error.to_string());
    }
    let result = response
        .get("result")
        .filter(|_| status.success())
        .ok_or_else(|| "Codex 返回了不完整的额度响应。".to_string())?;
    parse_quota(result, now()).map_err(|error| error.to_string())
}

pub fn start(app: AppHandle) {
    let monitor_app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let state = monitor_app.state::<AppState>();
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(10)) => {},
                _ = state.shutdown.notified() => return,
            }
            let _ = crate::windows::ensure_overlay_on_screen(&monitor_app);
        }
    });
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            let settings = match state.lock() {
                Ok(data) => data.preferences.settings.clone(),
                Err(_) => return,
            };
            let result = tokio::select! {
                result = fetch(&settings) => result,
                _ = state.shutdown.notified() => return,
            };
            if let Ok(mut data) = state.lock() {
                // 设置变更期间发出的旧请求不能覆盖新发行版的状态。
                if data.preferences.settings == settings {
                    match result {
                        Ok(quota) => {
                            let persisted = data
                                .store
                                .record_with_boundary(&quota, data.next_sample_starts_segment)
                                .and_then(|_| data.store.prune(now() - 90 * 86400));
                            data.quota = Some(quota);
                            match persisted {
                                Ok(()) => {
                                    data.status = "online";
                                    data.error = None;
                                    data.next_sample_starts_segment = false;
                                }
                                Err(_) => {
                                    data.status = "offline";
                                    data.next_sample_starts_segment = true;
                                    data.error = Some("额度已读取，但本地历史保存失败。请检查应用数据目录的可用空间和权限。".into());
                                }
                            }
                        }
                        Err(error) => {
                            data.status = "offline";
                            data.error = Some(error);
                            data.next_sample_starts_segment = true;
                        }
                    }
                }
            }
            broadcast(&app);
            let seconds = state
                .lock()
                .map(|data| data.preferences.settings.poll_interval_secs)
                .unwrap_or(60);
            // 单一循环串行处理采集；Notify 合并刷新请求，不会创建并行 app-server。
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(seconds)) => {},
                _ = state.refresh.notified() => {},
                _ = state.shutdown.notified() => return,
            }
        }
    });
}

pub async fn list_distros() -> Result<Vec<String>, String> {
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new("wsl.exe")
            .creation_flags(CREATE_NO_WINDOW)
            .kill_on_drop(true)
            .args(["--list", "--quiet"])
            .output(),
    )
    .await
    .map_err(|_| "获取 WSL 发行版列表超时。".to_string())?
    .map_err(|_| "无法启动 wsl.exe。".to_string())?;
    if !output.status.success() {
        return Err("无法获取 WSL 发行版列表，请检查 WSL 安装。".into());
    }
    // wsl.exe 的管理命令使用 UTF-16LE，而 Linux 子进程输出为 UTF-8。
    let text = if output.stdout.contains(&0) {
        let words: Vec<u16> = output
            .stdout
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16_lossy(&words)
    } else {
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    Ok(text
        .lines()
        .map(|line| line.trim().trim_start_matches('\u{feff}').to_string())
        .filter(|line| !line.is_empty())
        .collect())
}
