use crate::{
    collector::bridge_command,
    state::{broadcast, now, AppState},
};
use futures_util::{SinkExt, StreamExt};
use limit_core::{
    activity::{
        latest_turn_completed, parse_thread_page, summarize_threads, CompletionKey, SessionCounts,
        SessionThread,
    },
    settings::Settings,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    process::Stdio,
    time::Duration,
};
use tauri::{AppHandle, Manager};
use tokio::{
    io::Join,
    process::{Child, ChildStdin, ChildStdout},
};
use tokio_tungstenite::{client_async, tungstenite::Message, WebSocketStream};

pub(crate) struct ActivityClient {
    socket: WebSocketStream<Join<ChildStdout, ChildStdin>>,
    _process: Child,
    request_id: u64,
    started_at: i64,
    completed: HashMap<CompletionKey, bool>,
}

impl ActivityClient {
    pub(crate) async fn connect(settings: &Settings, started_at: i64) -> Result<Self, String> {
        let mut process = bridge_command(settings, "--daemon-proxy")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "无法启动 WSL 会话状态代理。".to_string())?;
        let reader = process.stdout.take().ok_or("无法读取会话代理。")?;
        let writer = process.stdin.take().ok_or("无法连接会话代理。")?;
        let (socket, _) = tokio::time::timeout(
            Duration::from_secs(12),
            client_async("ws://localhost/", tokio::io::join(reader, writer)),
        )
        .await
        .map_err(|_| "连接 Codex daemon 超时。".to_string())?
        .map_err(|_| "Codex daemon 暂不可用，会话气泡已隐藏。".to_string())?;
        let mut client = Self {
            socket,
            _process: process,
            request_id: 0,
            started_at,
            completed: HashMap::new(),
        };
        client.rpc("initialize",json!({"clientInfo":{"name":"codex_weekly_overlay","title":"Codex Weekly Overlay","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}})).await?;
        client
            .socket
            .send(Message::Text(
                json!({"method":"initialized","params":{}})
                    .to_string()
                    .into(),
            ))
            .await
            .map_err(|_| "Codex 会话代理已断开。".to_string())?;
        Ok(client)
    }

    async fn rpc(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.request_id += 1;
        let id = self.request_id;
        tokio::time::timeout(Duration::from_secs(8), async {
            self.socket
                .send(Message::Text(
                    json!({"id":id,"method":method,"params":params})
                        .to_string()
                        .into(),
                ))
                .await
                .map_err(|_| "Codex 会话代理已断开。".to_string())?;
            while let Some(message) = self.socket.next().await {
                match message.map_err(|_| "Codex 会话连接异常。".to_string())? {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(&text)
                            .map_err(|_| "Codex 会话协议响应无效。".to_string())?;
                        if value.get("id").and_then(Value::as_u64) == Some(id)
                            && value.get("method").is_none()
                        {
                            if value.get("error").is_some() {
                                return Err("Codex 会话接口暂不支持此只读查询。".into());
                            }
                            return value
                                .get("result")
                                .cloned()
                                .ok_or_else(|| "Codex 会话响应不完整。".into());
                        }
                    }
                    Message::Ping(_) => self
                        .socket
                        .flush()
                        .await
                        .map_err(|_| "Codex 会话连接已断开。".to_string())?,
                    Message::Close(_) => return Err("Codex 会话连接已关闭。".into()),
                    _ => {}
                }
            }
            Err("Codex 会话连接已断开。".into())
        })
        .await
        .map_err(|_| "读取 Codex 会话状态超时。".to_string())?
    }

    pub(crate) async fn fetch_counts(&mut self) -> Result<SessionCounts, String> {
        let mut cursor = None::<String>;
        let mut receipts_available = true;
        let mut threads = HashMap::<String, SessionThread>::new();
        let mut cursors = HashSet::new();
        loop {
            let response = self.rpc("thread/list",json!({"limit":100,"sortKey":"updated_at","sourceKinds":["cli","vscode","appServer"],"archived":false,"useStateDbOnly":true,"cursor":cursor})).await?;
            let page = parse_thread_page(&response)?;
            receipts_available &= page.read_states_available;
            for thread in page.threads {
                threads.entry(thread.id.clone()).or_insert(thread);
            }
            cursor = page.next_cursor;
            let Some(next) = &cursor else {
                break;
            };
            if !cursors.insert(next.clone()) || cursors.len() >= 256 {
                return Err("Codex 会话分页未完成，已隐藏不完整的计数。".into());
            }
        }
        let threads: Vec<_> = threads.into_values().collect();
        let keys: HashSet<_> = threads
            .iter()
            .filter_map(SessionThread::completion_key)
            .collect();
        self.completed.retain(|key, _| keys.contains(key));
        for key in keys {
            if self.completed.contains_key(&key) {
                continue;
            }
            // 只取最后一轮的元数据，不加载聊天内容，也不修改官方已读回执。
            match self.rpc("thread/turns/list",json!({"threadId":key.thread_id,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"})).await {
                Ok(response) => if let Ok(completed) = latest_turn_completed(&response, self.started_at) {
                    self.completed.insert(key,completed);
                },
                // 未读详情不可用不应抹去已经完整读取的工作中数量。
                Err(_) => break,
            }
        }
        Ok(summarize_threads(
            &threads,
            receipts_available,
            &self.completed,
            now(),
        ))
    }
}

pub fn start(app: AppHandle, started_at: i64) {
    tauri::async_runtime::spawn(async move {
        let mut client: Option<ActivityClient> = None;
        let mut distro = String::new();
        loop {
            let state = app.state::<AppState>();
            let settings = match state.lock() {
                Ok(data) => data.preferences.settings.clone(),
                Err(_) => return,
            };
            if distro != settings.distro {
                client = None;
            }
            let result = tokio::select! {
                result = tokio::time::timeout(Duration::from_secs(35),async {
                    if client.is_none() {
                        // 重连或切换发行版仍沿用程序入口的时刻，不截断本次运行已完成的任务。
                        client = Some(ActivityClient::connect(&settings, started_at).await?);
                        distro = settings.distro.clone();
                    }
                    client.as_mut().unwrap().fetch_counts().await
                }) => result.unwrap_or_else(|_|Err("读取 Codex 会话统计超时。".into())),
                _ = state.shutdown.notified() => return,
            };
            if result.is_err() {
                client = None;
            }
            if let Ok(mut data) = state.lock() {
                if data.preferences.settings == settings {
                    match result {
                        Ok(counts) => {
                            data.session_error = counts.unread_completed.is_none().then(|| {
                                "Codex 完成时间或未读状态暂不可用，红色气泡已隐藏。".into()
                            });
                            data.sessions = Some(counts);
                        }
                        Err(error) => {
                            data.sessions = None;
                            data.session_error = Some(error);
                        }
                    }
                }
            }
            broadcast(&app);
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(15)) => {},
                _ = state.activity_refresh.notified() => {},
                _ = state.shutdown.notified() => return,
            }
        }
    });
}
