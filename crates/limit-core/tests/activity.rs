use limit_core::activity::{latest_turn_completed, parse_thread_page, summarize_threads};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn counts_top_level_active_threads_and_completed_unread_only() {
    let response = json!({"data":[
        {"id":"working","status":{"type":"active"},"updatedAt":1},
        {"id":"child","parentThreadId":"working","status":{"type":"active"},"updatedAt":1},
        {"id":"done","status":{"type":"idle"},"updatedAt":2},
        {"id":"failed","status":{"type":"notLoaded"},"updatedAt":3},
        {"id":"read","status":{"type":"idle"},"updatedAt":4}
    ],"readStates":{
        "working":{"firstUnread":{"type":"turn","turnId":"t1"},"revision":"r1"},
        "done":{"firstUnread":{"type":"turn","turnId":"t2"},"revision":"r2"},
        "failed":{"firstUnread":{"type":"threadStart"},"revision":"r3"},
        "read":{"firstUnread":null,"revision":"r4"}
    },"nextCursor":null});
    let page = parse_thread_page(&response).unwrap();
    assert_eq!(page.threads.len(), 4);
    let completed = page
        .threads
        .iter()
        .filter_map(|thread| {
            thread.completion_key().map(|key| {
                let done = key.thread_id == "done";
                (key, done)
            })
        })
        .collect();
    let counts = summarize_threads(&page.threads, page.read_states_available, &completed, 100);
    assert_eq!(counts.working, 1);
    assert_eq!(counts.unread_completed, Some(1));
    assert_eq!(counts.observed_at, 100);
}

#[test]
fn unavailable_receipts_or_partial_completion_is_not_reported_as_zero() {
    let page=parse_thread_page(&json!({"data":[{"id":"a","status":{"type":"active"},"updatedAt":1}],"readStates":null,"nextCursor":null})).unwrap();
    assert_eq!(
        summarize_threads(
            &page.threads,
            page.read_states_available,
            &HashMap::new(),
            1
        )
        .unread_completed,
        None
    );
    let page=parse_thread_page(&json!({"data":[{"id":"a","status":{"type":"idle"},"updatedAt":1}],"readStates":{"a":{"firstUnread":{"type":"threadStart"},"revision":"r"}},"nextCursor":null})).unwrap();
    assert_eq!(
        summarize_threads(&page.threads, true, &HashMap::new(), 1).unread_completed,
        None
    );
}

#[test]
fn completion_cache_key_changes_with_read_revision_or_activity() {
    let make = |revision: &str, updated: i64| {
        parse_thread_page(&json!({"data":[{"id":"a","status":{"type":"idle"},"updatedAt":updated}],"readStates":{"a":{"firstUnread":{"type":"threadStart"},"revision":revision}},"nextCursor":null})).unwrap().threads[0].completion_key().unwrap()
    };
    assert_ne!(make("r1", 1), make("r2", 1));
    assert_ne!(make("r1", 1), make("r1", 2));
}

#[test]
fn validates_pagination_and_runtime_status() {
    assert!(parse_thread_page(&json!({"data":[],"readStates":{},"nextCursor":7})).is_err());
    assert!(parse_thread_page(&json!({"data":[{"id":"a","status":{"type":"unexpected"}}],"readStates":{},"nextCursor":null})).is_err());
    let page =
        parse_thread_page(&json!({"data":[],"readStates":{},"nextCursor":"next-page"})).unwrap();
    assert_eq!(page.next_cursor.as_deref(), Some("next-page"));
}

#[test]
fn latest_turn_must_really_be_completed() {
    assert!(latest_turn_completed(
        &json!({"data":[{"status":"completed","completedAt":101}]}),
        100
    )
    .unwrap());
    for status in ["inProgress", "failed", "interrupted"] {
        assert!(!latest_turn_completed(
            &json!({"data":[{"status":status,"completedAt":101}]}),
            100
        )
        .unwrap());
    }
    assert!(!latest_turn_completed(&json!({"data":[]}), 100).unwrap());
    assert!(latest_turn_completed(&json!({}), 100).is_err());
}

#[test]
fn completion_time_must_be_within_this_application_run() {
    let response = |time| json!({"data":[{"status":"completed","completedAt":time}]});
    assert!(!latest_turn_completed(&response(99), 100).unwrap());
    assert!(latest_turn_completed(&response(100), 100).unwrap());
    assert!(latest_turn_completed(&response(101), 100).unwrap());
}

#[test]
fn missing_or_invalid_completion_time_is_unknown() {
    assert!(latest_turn_completed(&json!({"data":[{"status":"completed"}]}), 100).is_err());
    for time in [json!(null), json!(-1), json!("101"), json!(101.5)] {
        assert!(latest_turn_completed(
            &json!({"data":[{"status":"completed","completedAt":time}]}),
            100
        )
        .is_err());
    }
}

#[test]
fn old_thread_can_complete_again_after_startup_without_counting_old_history() {
    let page = parse_thread_page(&json!({"data":[
        {"id":"old-result","status":{"type":"idle"},"createdAt":1,"updatedAt":110},
        {"id":"new-result","status":{"type":"notLoaded"},"createdAt":1,"updatedAt":111},
        {"id":"working","status":{"type":"active"},"createdAt":1,"updatedAt":1}
    ],"readStates":{
        "old-result":{"firstUnread":{"type":"turn","turnId":"old"},"revision":"r1"},
        "new-result":{"firstUnread":{"type":"turn","turnId":"old"},"revision":"r2"},
        "working":{"firstUnread":null,"revision":"r3"}
    }}))
    .unwrap();
    let completed = page
        .threads
        .iter()
        .filter_map(|thread| {
            let key = thread.completion_key()?;
            let finished_at = if key.thread_id == "old-result" {
                90
            } else {
                105
            };
            let response =
                json!({"data":[{"status":"completed","startedAt":1,"completedAt":finished_at}]});
            Some((key, latest_turn_completed(&response, 100).unwrap()))
        })
        .collect();
    for observed_at in [120, 200] {
        let counts = summarize_threads(&page.threads, true, &completed, observed_at);
        assert_eq!(counts.unread_completed, Some(1));
        assert_eq!(counts.working, 1);
    }
}

#[test]
fn restarting_application_excludes_previous_run_completions() {
    let response = json!({"data":[{"status":"completed","completedAt":150}]});
    assert!(latest_turn_completed(&response, 100).unwrap());
    assert!(!latest_turn_completed(&response, 200).unwrap());
}
