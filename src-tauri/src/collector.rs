use crate::{
    source::{AppServerMode, CodexSource},
    state::{broadcast, now, AppState},
};
use limit_core::{quota::ParsedQuota, quota_rpc::read_quota};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::io::BufReader;

pub async fn fetch(source: &CodexSource) -> Result<ParsedQuota, String> {
    let mut child = source
        .app_server(AppServerMode::Quota)
        .await?
        .spawn()
        .map_err(|_| source.launch_error())?;
    let mut writer = child.stdin.take().ok_or("无法连接 Codex app-server。")?;
    let mut reader = BufReader::new(child.stdout.take().ok_or("无法读取 Codex 输出。")?);
    let result = tokio::time::timeout(
        Duration::from_secs(25),
        read_quota(&mut reader, &mut writer, source.cache_key(), now),
    )
    .await
    .unwrap_or_else(|_| Err("读取 Codex 额度超时，请检查所选实例的网络。".into()));
    // WSL 通过父管道 EOF 回收进程组，原生 app-server 也可正常退出。
    drop(writer);
    drop(reader);
    let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
    result
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
        let state = app.state::<AppState>();
        let mut source_changed = state.source_changed.subscribe();
        loop {
            let (source, revision, changed) = {
                let Ok(mut data) = state.lock() else {
                    return;
                };
                let previous = data.source_revision;
                let source = state.resolve_source(&mut data);
                source_changed.borrow_and_update();
                (
                    source,
                    data.source_revision,
                    previous != data.source_revision,
                )
            };
            if changed {
                broadcast(&app);
            }
            let result = match source {
                Ok(source) => tokio::select! {
                    result = fetch(&source) => result.map(|quota| (source, quota)),
                    _ = source_changed.changed() => continue,
                    _ = state.shutdown.notified() => return,
                },
                Err(error) => Err(error),
            };
            if let Ok(mut data) = state.lock() {
                if data.source_revision == revision {
                    match result {
                        Ok((source, quota)) => {
                            let persisted = data
                                .store
                                .record_for_source(
                                    source.cache_key(),
                                    &quota,
                                    data.next_sample_starts_segment,
                                )
                                .and_then(|_| data.store.prune(now() - 90 * 86400));
                            if data.quota.as_ref().map(|last| &last.account_key)
                                != Some(&quota.account_key)
                            {
                                data.history_revision += 1;
                            }
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
            // 单一循环串行采集。来源切换立即取消旧连接，普通刷新请求合并到下一轮。
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(seconds)) => {},
                _ = source_changed.changed() => {},
                _ = state.refresh.notified() => {},
                _ = state.shutdown.notified() => return,
            }
        }
    });
}
