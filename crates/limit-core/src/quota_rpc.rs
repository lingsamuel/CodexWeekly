use crate::quota::{hash_account_id, parse_quota, ParsedQuota};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const DISCONNECTED: &str = "Codex app-server 已退出或连接断开。";

/// 从已启动的 Codex app-server 读取周额度。调用方负责超时和进程回收。
pub async fn read_quota(
    reader: &mut (impl AsyncBufRead + Unpin),
    writer: &mut (impl AsyncWrite + Unpin),
    source_key: &str,
    recorded_at: impl FnOnce() -> i64,
) -> Result<ParsedQuota, String> {
    send(writer, json!({"id":1,"method":"initialize","params":{"clientInfo":{
        "name":"codex_weekly_overlay","title":"Codex Weekly Overlay","version":env!("CARGO_PKG_VERSION")
    },"capabilities":{"experimentalApi":true}}})).await?;
    receive(reader, 1).await?;
    send(writer, json!({"method":"initialized","params":{}})).await?;
    send(writer, json!({"id":2,"method":"account/rateLimits/read"})).await?;
    let mut limits = receive(reader, 2).await?;
    let quota_id = identity(&limits, "accountId").map(str::to_owned);
    let response = if quota_id.is_some() {
        // 额度自带稳定 ID 时，兼容旧曲线的额外查询即使失败，仍可记录有效采样。
        tokio::time::timeout(Duration::from_secs(2), read_account(reader, writer))
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default()
    } else {
        read_account(reader, writer).await?
    };
    let account = &response["account"];
    let routing_id = identity(&response["workspaceRouting"], "chatgptAccountId");
    let workspace_id = routing_id.or_else(|| identity(account, "id"));
    let email = identity(account, "email");
    let personal = account["type"] == "chatgpt"
        && matches!(
            account["planType"].as_str(),
            Some("free" | "go" | "plus" | "pro" | "prolite" | "promax")
        );

    if quota_id.is_none() {
        if let Some(id) = workspace_id {
            limits["accountId"] = json!(id);
        } else if let Some(email) = email {
            limits["accountId"] = if personal {
                json!(email)
            } else {
                // 老协议只返回登录邮箱时，组织工作区可能共用邮箱。保留来源边界。
                let workspace = json!(["workspace", source_key, email, account["workspaceName"]]);
                Value::String(workspace.to_string())
            };
        }
    }
    let mut quota = parse_quota(&limits, recorded_at()).map_err(|error| error.to_string())?;
    // 同时核对账号路由与额度 ID，避免在两个读取之间切换账号时错误关联邮箱。
    if personal && routing_id.is_some() && routing_id == quota_id.as_deref() {
        quota.personal_email_key = email.map(hash_account_id);
    }
    Ok(quota)
}

async fn read_account(
    reader: &mut (impl AsyncBufRead + Unpin),
    writer: &mut (impl AsyncWrite + Unpin),
) -> Result<Value, String> {
    send(
        writer,
        json!({"id":3,"method":"account/read","params":{"refreshToken":false}}),
    )
    .await?;
    receive(reader, 3).await
}

fn identity<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str().filter(|id| !id.is_empty())
}

async fn send(writer: &mut (impl AsyncWrite + Unpin), value: Value) -> Result<(), String> {
    let mut bytes = value.to_string().into_bytes();
    bytes.push(b'\n');
    writer.write_all(&bytes).await.map_err(|_| DISCONNECTED)?;
    writer.flush().await.map_err(|_| DISCONNECTED)?;
    Ok(())
}

async fn receive(reader: &mut (impl AsyncBufRead + Unpin), id: u64) -> Result<Value, String> {
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        let count = (&mut *reader)
            .take(MAX_RESPONSE_BYTES + 1)
            .read_until(b'\n', &mut bytes)
            .await
            .map_err(|_| DISCONNECTED)?;
        if count == 0 {
            return Err(DISCONNECTED.into());
        }
        if count as u64 > MAX_RESPONSE_BYTES {
            return Err("Codex 协议响应过大，已停止本次读取。".into());
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "Codex 返回了无法识别的协议数据，请更新 Codex。")?;
        if let Some(error) = value.get("bridgeError").and_then(Value::as_str) {
            return Err(error.to_owned());
        }
        if value.get("id").and_then(Value::as_u64) != Some(id) || value.get("method").is_some() {
            continue;
        }
        if let Some(error) = value.get("error") {
            let message = error["message"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if ["auth", "login", "sign in", "unauthorized", "401"]
                .iter()
                .any(|word| message.contains(word))
            {
                return Err("Codex 登录失效，请在所选实例中恢复登录。".into());
            }
            return Err("Codex 无法读取订阅额度，请检查所选实例的登录与网络。".into());
        }
        return value
            .get("result")
            .filter(|result| result.is_object())
            .cloned()
            .ok_or_else(|| "Codex 返回了不完整的额度响应。".into());
    }
}
