use limit_core::quota::parse_quota;
use serde_json::{json, Value};

fn window(percent: f64, minutes: i64) -> Value {
    json!({"usedPercent":percent,"windowDurationMins":minutes,"resetsAt":2000000000})
}

#[test]
fn weekly_can_be_primary_for_current_account() {
    let response = json!({"accountId":"account-a","rateLimits":{"limitId":"codex","primary":window(27.0,10080),"secondary":null}});
    let quota = parse_quota(&response, 1234).unwrap();
    assert_eq!(quota.sample.remaining_percent, 73.0);
    assert_eq!(quota.sample.recorded_at, 1234);
    assert_eq!(quota.sample.window_minutes, 10080);
    assert_eq!(quota.sample.resets_at, Some(2000000000));
    assert_ne!(quota.account_key, "account-a");
}

#[test]
fn weekly_can_be_secondary_and_codex_bucket_wins() {
    let response = json!({"accountId":"account-a","rateLimits":{"limitId":"other","primary":window(99.0,10080)},"rateLimitsByLimitId":{"other":{"primary":window(80.0,10080)},"codex":{"primary":window(10.0,300),"secondary":window(42.0,10080)}}});
    assert_eq!(
        parse_quota(&response, 1).unwrap().sample.remaining_percent,
        58.0
    );
}

#[test]
fn missing_weekly_is_not_treated_as_full_or_a_short_window() {
    assert!(parse_quota(&json!({"rateLimits":{"primary":window(1.0,300)}}), 1).is_err());
    assert!(parse_quota(&json!({"rateLimits":null}), 1).is_err());
    assert!(parse_quota(
        &json!({"rateLimits":{"limitId":"other","primary":window(1.0,10080)}}),
        1
    )
    .is_err());
}

#[test]
fn percentage_must_be_finite_and_in_range() {
    for percent in [-1.0, 101.0] {
        assert!(parse_quota(&json!({"rateLimits":{"primary":window(percent,10080)}}), 1).is_err());
    }
    let malformed =
        json!({"rateLimits":{"primary":{"usedPercent":"27","windowDurationMins":10080}}});
    assert!(parse_quota(&malformed, 1).is_err());
}

#[test]
fn empty_and_exhausted_are_valid_and_reset_is_nullable() {
    for (used, remaining) in [(0.0, 100.0), (100.0, 0.0), (27.25, 72.75)] {
        let response = json!({"accountId":"a","rateLimits":{"primary":{"usedPercent":used,"windowDurationMins":10080,"resetsAt":null}}});
        let quota = parse_quota(&response, 1).unwrap();
        assert_eq!(quota.sample.remaining_percent, remaining);
        assert_eq!(quota.sample.resets_at, None);
    }
}

#[test]
fn account_identity_is_stable_but_not_shared() {
    let mut a = json!({"accountId":"a","rateLimits":{"primary":window(10.0,10080)}});
    let first = parse_quota(&a, 1).unwrap();
    assert_eq!(first.account_key, parse_quota(&a, 2).unwrap().account_key);
    a["accountId"] = json!("b");
    assert_ne!(first.account_key, parse_quota(&a, 1).unwrap().account_key);
}
