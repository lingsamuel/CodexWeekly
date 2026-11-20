use serde::Serialize;
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCounts {
    pub working: u32,
    pub unread_completed: Option<u32>,
    pub observed_at: i64,
    pub working_details: Vec<SessionDetail>,
    pub unread_completed_details: Option<Vec<SessionDetail>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub id: String,
    pub completion_id: Option<String>,
    pub title: Option<String>,
    pub project: Option<String>,
    pub working_directory: Option<String>,
    /// 仅用于通过 daemon 关联 Project 名称。
    #[serde(skip)]
    pub project_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CompletionKey {
    pub thread_id: String,
    revision: String,
    updated_at: i64,
}

#[derive(Debug)]
enum ReadState {
    Read,
    Unread(String),
    Ineligible,
    Unavailable,
}

#[derive(Debug)]
pub struct SessionThread {
    pub id: String,
    status: String,
    updated_at: i64,
    read_state: ReadState,
    title: Option<String>,
    project_id: Option<String>,
    working_directory: Option<String>,
}

impl SessionThread {
    pub fn completion_key(&self) -> Option<CompletionKey> {
        if !matches!(self.status.as_str(), "idle" | "notLoaded") {
            return None;
        }
        match &self.read_state {
            ReadState::Unread(revision) => Some(CompletionKey {
                thread_id: self.id.clone(),
                revision: revision.clone(),
                updated_at: self.updated_at,
            }),
            _ => None,
        }
    }

    fn detail_order(&self) -> (Reverse<i64>, &str) {
        (Reverse(self.updated_at), &self.id)
    }

    fn detail(&self, completion_id: Option<String>) -> SessionDetail {
        SessionDetail {
            id: self.id.clone(),
            completion_id,
            title: self.title.clone(),
            project: None,
            working_directory: self.working_directory.clone(),
            project_id: self.project_id.clone(),
        }
    }
}

pub struct ThreadPage {
    pub threads: Vec<SessionThread>,
    pub read_states_available: bool,
    pub next_cursor: Option<String>,
}

pub fn parse_thread_page(response: &Value) -> Result<ThreadPage, String> {
    let rows = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or("Codex 会话列表格式无效。")?;
    let receipts = response.get("readStates").and_then(Value::as_object);
    let mut threads = Vec::with_capacity(rows.len());
    for row in rows {
        if row
            .get("parentThreadId")
            .is_some_and(|parent| !parent.is_null())
        {
            continue;
        }
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or("Codex 会话标识缺失。")?;
        let status = row
            .get("status")
            .and_then(|status| status.get("type"))
            .and_then(Value::as_str)
            .ok_or("Codex 会话状态缺失。")?;
        if !matches!(status, "active" | "idle" | "notLoaded" | "systemError") {
            return Err("Codex 返回了未知会话状态。".into());
        }
        let read_state = match receipts {
            None => ReadState::Unavailable,
            Some(receipts) => match receipts.get(id) {
                None => ReadState::Ineligible,
                Some(receipt) => match receipt.get("firstUnread") {
                    Some(Value::Null) => ReadState::Read,
                    Some(position)
                        if matches!(
                            position.get("type").and_then(Value::as_str),
                            Some("threadStart" | "turn")
                        ) =>
                    {
                        match receipt.get("revision").and_then(Value::as_str) {
                            Some(revision) => ReadState::Unread(revision.to_string()),
                            None => ReadState::Unavailable,
                        }
                    }
                    _ => ReadState::Unavailable,
                },
            },
        };
        threads.push(SessionThread {
            id: id.to_string(),
            status: status.to_string(),
            updated_at: row.get("updatedAt").and_then(Value::as_i64).unwrap_or(0),
            read_state,
            title: nonempty_string(row, "name").map(str::to_owned),
            project_id: nonempty_string(row, "projectId").map(str::to_owned),
            working_directory: row
                .get("cwd")
                .and_then(Value::as_str)
                .and_then(|cwd| {
                    // WSL 的绝对路径允许文件名包含反斜杠，只有 Windows 路径才将其视为分隔符。
                    let windows_path = !cwd.starts_with('/');
                    cwd.rsplit(|character| character == '/' || (windows_path && character == '\\'))
                        .find(|part| !part.is_empty())
                })
                .map(str::to_owned),
        });
    }
    let next_cursor = match response.get("nextCursor") {
        None | Some(Value::Null) => None,
        Some(Value::String(cursor)) if !cursor.is_empty() => Some(cursor.clone()),
        _ => return Err("Codex 会话分页游标无效。".into()),
    };
    Ok(ThreadPage {
        threads,
        read_states_available: receipts.is_some(),
        next_cursor,
    })
}

fn nonempty_string<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// 从按新到旧排列的轮次列表中识别本次运行期间完成的最后一轮。
pub fn latest_completed_turn_id(
    response: &Value,
    started_at: i64,
) -> Result<Option<String>, String> {
    let rows = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or("Codex 未返回会话最近一轮状态。")?;
    let Some(turn) = rows.first() else {
        return Ok(None);
    };
    match turn.get("status").and_then(Value::as_str) {
        Some("completed") => {
            // 会话更新和开始时间不能代表结果交付；只用轮次真实完成时间排除历史积压。
            let completed_at = turn
                .get("completedAt")
                .and_then(Value::as_i64)
                .filter(|time| *time >= 0)
                .ok_or("Codex 未返回有效的轮次完成时间。")?;
            Ok((completed_at >= started_at).then(|| {
                nonempty_string(turn, "id")
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("completedAt:{completed_at}"))
            }))
        }
        Some("inProgress" | "failed" | "interrupted") => Ok(None),
        _ => Err("Codex 返回了未知轮次状态。".into()),
    }
}

/// 根据已完整读取的会话快照和轮次缓存整理两类气泡的计数与详情。
pub fn summarize_threads(
    threads: &[SessionThread],
    receipts_available: bool,
    completed: &HashMap<CompletionKey, Option<String>>,
    observed_at: i64,
) -> SessionCounts {
    let mut working = Vec::new();
    let mut unread = Vec::new();
    let mut unread_available = receipts_available;
    for thread in threads {
        if matches!(thread.read_state, ReadState::Unavailable) {
            unread_available = false;
        }
        if thread.status == "active" {
            working.push(thread);
        } else if let Some(key) = thread.completion_key() {
            match completed.get(&key) {
                Some(Some(completion_id)) => unread.push((thread, completion_id)),
                Some(None) => {}
                // 不完整的查询不能冒充“没有未读”；蓝色计数仍可独立使用。
                None => unread_available = false,
            }
        }
    }
    working.sort_unstable_by_key(|&thread| thread.detail_order());
    unread.sort_unstable_by_key(|&(thread, _)| thread.detail_order());
    SessionCounts {
        working: working.len() as u32,
        unread_completed: unread_available.then_some(unread.len() as u32),
        observed_at,
        working_details: working
            .into_iter()
            .map(|thread| thread.detail(None))
            .collect(),
        unread_completed_details: unread_available.then(|| {
            unread
                .into_iter()
                .map(|(thread, id)| thread.detail(Some(id.clone())))
                .collect()
        }),
    }
}
