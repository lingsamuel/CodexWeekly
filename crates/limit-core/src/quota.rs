use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const WEEK_MINUTES: u32 = 7 * 24 * 60;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSample {
    pub recorded_at: i64,
    pub remaining_percent: f64,
    pub resets_at: Option<i64>,
    pub window_minutes: u32,
}

#[derive(Clone, Debug)]
pub struct ParsedQuota {
    pub account_key: String,
    pub personal_email_key: Option<String>,
    pub sample: QuotaSample,
}

#[derive(Debug, Error)]
pub enum QuotaError {
    #[error("Codex 尚未提供可识别的 Weekly 额度，请确认所选实例已登录具有 ChatGPT 订阅的账号。")]
    MissingWeekly,
    #[error("Codex 返回的周额度格式无效；已保留上次有效读数。")]
    InvalidWindow,
    #[error("无法识别当前 Codex 账号，已停止记录以避免混合不同账号的历史。")]
    MissingIdentity,
}

pub fn parse_quota(response: &Value, recorded_at: i64) -> Result<ParsedQuota, QuotaError> {
    let bucket = response
        .get("rateLimitsByLimitId")
        .and_then(|buckets| buckets.get("codex"))
        .filter(|bucket| bucket.is_object())
        .or_else(|| {
            response.get("rateLimits").filter(|bucket| {
                bucket.is_object()
                    && bucket
                        .get("limitId")
                        .and_then(Value::as_str)
                        .is_none_or(|id| id == "codex")
            })
        })
        .ok_or(QuotaError::MissingWeekly)?;

    // 订阅类型不同，周窗口可以出现在 primary 或 secondary；不依赖字段位置。
    let window = ["primary", "secondary"]
        .into_iter()
        .filter_map(|key| bucket.get(key))
        .find(|window| {
            window.get("windowDurationMins").and_then(Value::as_u64) == Some(WEEK_MINUTES as u64)
        })
        .ok_or(QuotaError::MissingWeekly)?;
    let used = window
        .get("usedPercent")
        .and_then(Value::as_f64)
        .ok_or(QuotaError::InvalidWindow)?;
    if !used.is_finite() || !(0.0..=100.0).contains(&used) {
        return Err(QuotaError::InvalidWindow);
    }
    let resets_at = match window.get("resetsAt") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_i64()
                .filter(|value| *value > 0)
                .ok_or(QuotaError::InvalidWindow)?,
        ),
    };
    let account_key = match response.get("accountKey").and_then(Value::as_str) {
        Some(key) if key.len() == 64 && key.bytes().all(|ch| ch.is_ascii_hexdigit()) => {
            key.to_string()
        }
        _ => {
            let id = response
                .get("accountId")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .ok_or(QuotaError::MissingIdentity)?;
            hash_account_id(id)
        }
    };
    Ok(ParsedQuota {
        account_key,
        personal_email_key: None,
        sample: QuotaSample {
            recorded_at,
            remaining_percent: 100.0 - used,
            resets_at,
            window_minutes: WEEK_MINUTES,
        },
    })
}

pub(crate) fn hash_account_id(id: &str) -> String {
    format!("{:x}", Sha256::digest(id.as_bytes()))
}
