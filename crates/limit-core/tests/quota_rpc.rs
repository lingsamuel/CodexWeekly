use limit_core::{quota::parse_quota, quota_rpc::read_quota};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[tokio::test]
async fn reads_weekly_quota_after_initialization_and_has_read_only_identity_fallback() {
    let (client, server) = tokio::io::duplex(4096);
    let (reader, mut writer) = tokio::io::split(client);
    let (reader_server, mut writer_server) = tokio::io::split(server);
    let peer = async {
        let mut lines = BufReader::new(reader_server).lines();
        for (method, response) in [
            ("initialize", json!({})),
            ("initialized", Value::Null),
            (
                "account/rateLimits/read",
                json!({"rateLimits":{"primary":{"usedPercent":27,"windowDurationMins":10080}},"credits":"discarded"}),
            ),
            (
                "account/read",
                json!({"account":{"type":"chatgpt","email":"test@example.invalid","planType":"pro"}}),
            ),
        ] {
            let request: Value =
                serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(request["method"], method);
            if method == "initialized" {
                assert!(request.get("id").is_none());
                continue;
            }
            if method == "account/read" {
                assert_eq!(request["params"], json!({"refreshToken":false}));
            }
            let server_request = json!({"id": request["id"], "method":"unrelated"});
            let reply = json!({"id":request["id"],"result":response});
            writer_server
                .write_all(format!("{server_request}\n{reply}\n").as_bytes())
                .await
                .unwrap();
        }
    };
    let mut reader = BufReader::new(reader);
    let (quota, ()) = tokio::join!(read_quota(&mut reader, &mut writer, "windows", || 5), peer);
    let quota = quota.unwrap();
    let expected = parse_quota(&json!({"accountId":"test@example.invalid","rateLimits":{"primary":{"usedPercent":27,"windowDurationMins":10080}}}), 5).unwrap();
    assert_eq!(quota.account_key, expected.account_key);
    assert_eq!(quota.sample.remaining_percent, 73.0);
}

#[tokio::test]
async fn links_old_personal_identity_only_when_account_read_confirms_the_quota_account() {
    let mut writer = tokio::io::sink();
    for (quota_id, routing, plan, expected) in [
        (Some("current-id"), "current-id", "pro", true),
        (Some("current-id"), "different-id", "pro", false),
        (Some("current-id"), "current-id", "business", false),
        (None, "current-id", "pro", false),
    ] {
        let output = format!(
            "{}\n{}\n{}\n",
            json!({"id":1,"result":{}}),
            json!({"id":2,"result":{"accountId":quota_id,"rateLimits":{"primary":{"usedPercent":27,"windowDurationMins":10080}}}}),
            json!({"id":3,"result":{"account":{"type":"chatgpt","planType":plan,"email":"test@example.invalid"},"workspaceRouting":{"chatgptAccountId":routing}}})
        );
        let quota = read_quota(&mut output.as_bytes(), &mut writer, "windows", || 5)
            .await
            .unwrap();
        assert_eq!(quota.personal_email_key.is_some(), expected);
    }
}

#[tokio::test]
async fn legacy_workspace_email_remains_scoped_to_its_source() {
    let response = format!(
        "{}\n{}\n{}\n",
        json!({"id":1,"result":{}}),
        json!({"id":2,"result":{"rateLimits":{"primary":{"usedPercent":27,"windowDurationMins":10080}}}}),
        json!({"id":3,"result":{"account":{"type":"chatgpt","planType":"business","email":"test@example.invalid"}}})
    );
    let mut writer = tokio::io::sink();
    let windows = read_quota(&mut response.as_bytes(), &mut writer, "windows", || 5)
        .await
        .unwrap();
    let wsl = read_quota(&mut response.as_bytes(), &mut writer, "wsl", || 5)
        .await
        .unwrap();
    assert_ne!(windows.account_key, wsl.account_key);
}

#[tokio::test]
async fn reports_rpc_errors_without_echoing_server_details() {
    let mut reader =
        &b"{\"id\":1,\"error\":{\"message\":\"Unauthorized test@example.invalid\"}}\n"[..];
    let mut writer = tokio::io::sink();
    let error = read_quota(&mut reader, &mut writer, "windows", || 5)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("登录"));
    assert!(!error.to_string().contains("example.invalid"));
}
