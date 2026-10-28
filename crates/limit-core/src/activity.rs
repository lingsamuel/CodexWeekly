use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCounts {
    pub working: u32,
    pub unread_completed: Option<u32>,
    pub observed_at: i64,
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

pub fn latest_turn_completed(response: &Value) -> Result<bool, String> {
    let rows = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or("Codex 未返回会话最近一轮状态。")?;
    let Some(turn) = rows.first() else {
        return Ok(false);
    };
    match turn.get("status").and_then(Value::as_str) {
        Some("completed") => Ok(true),
        Some("inProgress" | "failed" | "interrupted") => Ok(false),
        _ => Err("Codex 返回了未知轮次状态。".into()),
    }
}

pub fn summarize_threads(
    threads: &[SessionThread],
    receipts_available: bool,
    completed: &HashMap<CompletionKey, bool>,
    observed_at: i64,
) -> SessionCounts {
    let working = threads
        .iter()
        .filter(|thread| thread.status == "active")
        .count() as u32;
    let mut unread_completed = receipts_available.then_some(0);
    for thread in threads {
        if matches!(thread.read_state, ReadState::Unavailable) {
            unread_completed = None;
        }
        if let Some(key) = thread.completion_key() {
            match completed.get(&key) {
                Some(true) => unread_completed = unread_completed.map(|count| count + 1),
                Some(false) => {}
                // 不完整的查询不能冒充“没有未读”；蓝色计数仍可独立使用。
                None => unread_completed = None,
            }
        }
    }
    SessionCounts {
        working,
        unread_completed,
        observed_at,
    }
}
